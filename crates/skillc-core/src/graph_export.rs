//! `skillc graph` — export the whole dependency graph (maintainer tooling).
//!
//! Where `why <id>` answers the reverse-dependency question for ONE unit, `graph` dumps
//! the entire catalog as nodes + edges so a human can *see* it: targets, skills (with
//! their layer derived from structure, mirroring the atomic-pattern conventions),
//! blocks, references, and every mounts/imports/includes/hosts edge. Read-only; built
//! on the same parse/resolve/shake passes as `build`, so the picture is the compiler's
//! truth, not a text-match approximation.
//!
//! Canonical output is JSON; DOT / Mermaid / a self-contained HTML viewer are renderings
//! of the same export.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;

use crate::config::FrameworkConfig;
use crate::graph::transitive_blocks;
use crate::model::{Catalog, ReferenceKind};
use crate::resolve::Resolution;
use crate::shake;

/// Output format for `skillc graph`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphFormat {
    Json,
    Dot,
    Mermaid,
    Html,
}

impl GraphFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "json" => Some(Self::Json),
            "dot" => Some(Self::Dot),
            "mermaid" => Some(Self::Mermaid),
            "html" => Some(Self::Html),
            _ => None,
        }
    }
}

/// One node of the export. `layer` is derived from structure, never declared — the same
/// rule the atomic-pattern catalogs document: mounted ⇒ template, has imports ⇒
/// organism, no imports ⇒ molecule; blocks are atoms; targets are pages.
#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub id: String,
    /// `target` | `skill` | `block` | `reference`
    pub kind: &'static str,
    /// `page` | `template` | `organism` | `molecule` | `atom` | `reference`
    pub layer: &'static str,
    /// True for units pulled cross-repo (the layer is still derived — a pulled skill
    /// with imports is an organism that happens to live in another repo).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub external: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Where the unit is authored: a path relative to the catalog root, or the external
    /// source id for pulled skills.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Targets whose bundle ships this unit (empty for target nodes themselves).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    /// For references: `per-target` | `shared` | `stray`.
    #[serde(rename = "refKind", skip_serializing_if = "Option::is_none")]
    pub ref_kind: Option<&'static str>,
    /// Health flags: `orphan` (skill reached by no entry/import), `unused` (block
    /// included by nobody), `stray` (reference stem matching no target and not shared).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<&'static str>,
}

/// One edge of the export.
#[derive(Debug, Clone, Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    /// `mounts` | `imports` | `includes` | `hosts`
    pub kind: &'static str,
    /// The `{{Alias}}` an `imports` edge is referenced by in the body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

/// The full export, render-ready and deterministic (nodes and edges in BTree order).
#[derive(Debug, Clone, Serialize)]
pub struct GraphExport {
    pub version: u32,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// Restrict the export to a slice a human asked for.
#[derive(Debug, Clone, Default)]
pub struct GraphFilter {
    /// Keep only this target's bundle closure.
    pub target: Option<String>,
    /// Keep only nodes within `depth` (undirected) hops of this unit.
    pub focus: Option<String>,
    pub depth: usize,
}

/// Build the full export from the compiler's own passes.
pub fn export(
    catalog: &Catalog,
    cfg: &FrameworkConfig,
    resolution: &Resolution,
    catalog_root: &Path,
) -> GraphExport {
    // Per-target membership, computed once. `graph` is read-only and `check` owns full
    // reporting, so shake diagnostics go to scratch.
    let mut ship_targets: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut block_targets: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for target in cfg.targets() {
        let mut scratch = crate::diagnostics::Diagnostics::new();
        let membership = shake::shake(catalog, cfg, resolution, &target.name, &mut scratch);
        for s in membership.members.iter().chain(membership.pulled.iter()) {
            ship_targets
                .entry(s.clone())
                .or_default()
                .insert(target.name.clone());
        }
        // Blocks ship wherever a member skill transitively inlines them.
        for member in &membership.members {
            if let Some(skill) = catalog.skills.get(member) {
                for b in transitive_blocks(catalog, &skill.includes) {
                    block_targets
                        .entry(b)
                        .or_default()
                        .insert(target.name.clone());
                }
            }
        }
    }

    // Health flags, computed structurally (not by parsing diagnostic strings).
    let orphan_skills = orphans(catalog, cfg, resolution);
    let used_blocks = reachable_from_skills(catalog);

    let mounted: BTreeSet<&str> = cfg
        .targets()
        .flat_map(|t| t.mounts.iter().map(String::as_str))
        .collect();

    let mut nodes: Vec<Node> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();

    // Target nodes (Pages) + mounts edges.
    for target in cfg.targets() {
        nodes.push(Node {
            id: target.name.clone(),
            kind: "target",
            layer: "page",
            external: false,
            description: None,
            source: None,
            targets: Vec::new(),
            ref_kind: None,
            flags: Vec::new(),
        });
        for mount in &target.mounts {
            if catalog.skills.contains_key(mount) {
                edges.push(Edge {
                    from: target.name.clone(),
                    to: mount.clone(),
                    kind: "mounts",
                    alias: None,
                });
            }
        }
    }

    // Catalog skills: layer derived (mounted ⇒ template wins; then imports ⇒ organism).
    for skill in catalog.skills.values() {
        let layer = if mounted.contains(skill.id.as_str()) {
            "template"
        } else if skill.imports.is_empty() {
            "molecule"
        } else {
            "organism"
        };
        let mut flags = Vec::new();
        if orphan_skills.contains(skill.id.as_str()) {
            flags.push("orphan");
        }
        nodes.push(Node {
            id: skill.id.clone(),
            kind: "skill",
            layer,
            external: false,
            description: Some(skill.description.clone()),
            source: relative_source(&skill.source_path, catalog_root),
            targets: sorted(ship_targets.get(&skill.id)),
            ref_kind: None,
            flags,
        });

        // hosts edges + reference nodes. A per-target reference only ships where its stem
        // matches; shared/optional ride the host's targets.
        for reference in &skill.references {
            let ref_id = format!("{}/{}", skill.id, reference.stem);
            let (ref_kind, flags): (&'static str, Vec<&'static str>) =
                match cfg.classify_reference(&reference.stem) {
                    ReferenceKind::PerTarget => ("per-target", Vec::new()),
                    ReferenceKind::Shared => ("shared", Vec::new()),
                    ReferenceKind::Stray => ("stray", vec!["stray"]),
                };
            let host_targets = sorted(ship_targets.get(&skill.id));
            let targets = if ref_kind == "per-target" {
                host_targets
                    .into_iter()
                    .filter(|t| t == &reference.stem)
                    .collect()
            } else {
                host_targets
            };
            nodes.push(Node {
                id: ref_id.clone(),
                kind: "reference",
                layer: "reference",
                external: false,
                description: None,
                source: relative_source(&reference.path, catalog_root),
                targets,
                ref_kind: Some(ref_kind),
                flags,
            });
            edges.push(Edge {
                from: skill.id.clone(),
                to: ref_id,
                kind: "hosts",
                alias: None,
            });
        }

        // includes edges (skill → block). Missing blocks were gated by analyze_includes.
        for inc in &skill.includes {
            if catalog.blocks.contains_key(inc) {
                edges.push(Edge {
                    from: skill.id.clone(),
                    to: inc.clone(),
                    kind: "includes",
                    alias: None,
                });
            }
        }
    }

    // imports edges: the RESOLVED marker map, not raw frontmatter — same truth as build.
    for ((skill_id, alias), target_id) in &resolution.marker_targets {
        edges.push(Edge {
            from: skill_id.clone(),
            to: target_id.clone(),
            kind: "imports",
            alias: Some(alias.clone()),
        });
    }

    // External pulled skills. The layer is still derived (imports ⇒ organism) — pulled
    // units are full citizens of the atomic pattern, they just live in another repo.
    // Their @includes were expanded against the source mirror at resolve time, so the
    // source blocks are re-surfaced here as external atoms (namespaced `<source>:<id>`
    // — block ids are only unique per source) to keep shared atoms visible.
    let mut external_blocks: BTreeMap<String, (String, BTreeSet<String>)> = BTreeMap::new();
    for pulled in resolution.pulled.values() {
        let layer = if pulled.imports.is_empty() {
            "molecule"
        } else {
            "organism"
        };
        nodes.push(Node {
            id: pulled.id.clone(),
            kind: "skill",
            layer,
            external: true,
            description: Some(pulled.description.clone()),
            source: Some(pulled.source.clone()),
            targets: sorted(ship_targets.get(&pulled.id)),
            ref_kind: None,
            flags: Vec::new(),
        });
        for inc in &pulled.includes {
            let block_id = format!("{}:{}", pulled.source, inc);
            let entry = external_blocks
                .entry(block_id.clone())
                .or_insert_with(|| (pulled.source.clone(), BTreeSet::new()));
            entry.1.extend(sorted(ship_targets.get(&pulled.id)));
            edges.push(Edge {
                from: pulled.id.clone(),
                to: block_id,
                kind: "includes",
                alias: None,
            });
        }
        for reference in &pulled.references {
            let ref_id = format!("{}/{}", pulled.id, reference.stem);
            let (ref_kind, flags): (&'static str, Vec<&'static str>) =
                match cfg.classify_reference(&reference.stem) {
                    ReferenceKind::PerTarget => ("per-target", Vec::new()),
                    ReferenceKind::Shared => ("shared", Vec::new()),
                    ReferenceKind::Stray => ("stray", vec!["stray"]),
                };
            nodes.push(Node {
                id: ref_id.clone(),
                kind: "reference",
                layer: "reference",
                external: true,
                description: None,
                source: Some(pulled.source.clone()),
                targets: sorted(ship_targets.get(&pulled.id)),
                ref_kind: Some(ref_kind),
                flags,
            });
            edges.push(Edge {
                from: pulled.id.clone(),
                to: ref_id,
                kind: "hosts",
                alias: None,
            });
        }
    }
    for (block_id, (source, targets)) in external_blocks {
        nodes.push(Node {
            id: block_id,
            kind: "block",
            layer: "atom",
            external: true,
            description: None,
            source: Some(source),
            targets: targets.into_iter().collect(),
            ref_kind: None,
            flags: Vec::new(),
        });
    }
    for (pulled_id, deps) in &resolution.pulled_deps {
        for dep in deps {
            edges.push(Edge {
                from: pulled_id.clone(),
                to: dep.clone(),
                kind: "imports",
                alias: None,
            });
        }
    }

    // Block nodes (Atoms) + block → block includes edges.
    for block in catalog.blocks.values() {
        let mut flags = Vec::new();
        if !used_blocks.contains(block.id.as_str()) {
            flags.push("unused");
        }
        nodes.push(Node {
            id: block.id.clone(),
            kind: "block",
            layer: "atom",
            external: false,
            description: None,
            source: relative_source(&block.source_path, catalog_root),
            targets: sorted(block_targets.get(&block.id)),
            ref_kind: None,
            flags,
        });
        for inc in &block.includes {
            if catalog.blocks.contains_key(inc) {
                edges.push(Edge {
                    from: block.id.clone(),
                    to: inc.clone(),
                    kind: "includes",
                    alias: None,
                });
            }
        }
    }

    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    edges.sort_by(|a, b| {
        (&a.from, &a.to, a.kind, &a.alias).cmp(&(&b.from, &b.to, b.kind, &b.alias))
    });
    GraphExport {
        version: 1,
        nodes,
        edges,
    }
}

/// Apply `--target` / `--focus` restrictions. Unknown ids were validated by the caller.
pub fn filter(export: GraphExport, f: &GraphFilter) -> GraphExport {
    let mut keep: BTreeSet<String> = export.nodes.iter().map(|n| n.id.clone()).collect();

    if let Some(target) = &f.target {
        keep = export
            .nodes
            .iter()
            .filter(|n| {
                if n.kind == "target" {
                    &n.id == target
                } else {
                    n.targets.iter().any(|t| t == target)
                }
            })
            .map(|n| n.id.clone())
            .collect();
    }

    if let Some(focus) = &f.focus {
        // Undirected BFS over edges, `depth` hops, intersected with the target slice.
        let mut adjacency: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for e in &export.edges {
            adjacency.entry(&e.from).or_default().push(&e.to);
            adjacency.entry(&e.to).or_default().push(&e.from);
        }
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut frontier = vec![focus.clone()];
        seen.insert(focus.clone());
        for _ in 0..f.depth {
            let mut next = Vec::new();
            for id in &frontier {
                for n in adjacency.get(id.as_str()).into_iter().flatten() {
                    if seen.insert((*n).to_string()) {
                        next.push((*n).to_string());
                    }
                }
            }
            frontier = next;
        }
        keep.retain(|id| seen.contains(id));
    }

    let nodes: Vec<Node> = export
        .nodes
        .into_iter()
        .filter(|n| keep.contains(&n.id))
        .collect();
    let edges: Vec<Edge> = export
        .edges
        .into_iter()
        .filter(|e| keep.contains(&e.from) && keep.contains(&e.to))
        .collect();
    GraphExport {
        version: 1,
        nodes,
        edges,
    }
}

/// Render the export in the requested format.
pub fn render(export: &GraphExport, format: GraphFormat) -> String {
    match format {
        GraphFormat::Json => render_json(export),
        GraphFormat::Dot => render_dot(export),
        GraphFormat::Mermaid => render_mermaid(export),
        GraphFormat::Html => render_html(export),
    }
}

fn render_json(export: &GraphExport) -> String {
    let mut s = serde_json::to_string_pretty(export).expect("export serializes");
    s.push('\n');
    s
}

/// Visual vocabulary shared by DOT and Mermaid: layer → fill, edge kind → line style.
fn layer_fill(layer: &str) -> &'static str {
    match layer {
        "page" => "#1d3557",
        "template" => "#457b9d",
        "organism" => "#2a9d8f",
        "molecule" => "#8ab17d",
        "atom" => "#e9c46a",
        _ => "#cccccc", // reference
    }
}

const LAYER_ORDER: [&str; 6] = [
    "page",
    "template",
    "organism",
    "molecule",
    "atom",
    "reference",
];

fn render_dot(export: &GraphExport) -> String {
    use std::fmt::Write as _;
    let mut s = String::from("digraph skills {\n  rankdir=TB;\n  node [shape=box, style=\"rounded,filled\", fontname=\"Helvetica\", fontcolor=white];\n");

    // One rank per layer keeps the atomic hierarchy readable top-down.
    for layer in LAYER_ORDER {
        let members: Vec<&Node> = export.nodes.iter().filter(|n| n.layer == layer).collect();
        if members.is_empty() {
            continue;
        }
        let _ = writeln!(s, "  {{ rank=same;");
        for n in members {
            let fontcolor = if matches!(n.layer, "atom" | "reference") {
                ", fontcolor=black"
            } else {
                ""
            };
            let dashed = if n.external {
                ", style=\"rounded,filled,dashed\""
            } else {
                ""
            };
            let outline = if n.flags.is_empty() {
                String::new()
            } else {
                format!(", color=red, penwidth=2, tooltip=\"{}\"", n.flags.join(","))
            };
            let _ = writeln!(
                s,
                "    \"{}\" [fillcolor=\"{}\"{fontcolor}{dashed}{outline}];",
                dot_escape(&n.id),
                layer_fill(n.layer)
            );
        }
        let _ = writeln!(s, "  }}");
    }

    for e in &export.edges {
        let (style, extra) = match e.kind {
            "mounts" => ("bold", String::new()),
            "includes" => ("dashed", String::new()),
            "hosts" => ("dotted", ", color=gray, arrowhead=none".to_string()),
            _ => (
                "solid",
                e.alias
                    .as_ref()
                    .map(|a| format!(", label=\"{}\", fontsize=9", dot_escape(a)))
                    .unwrap_or_default(),
            ),
        };
        let _ = writeln!(
            s,
            "  \"{}\" -> \"{}\" [style={style}{extra}];",
            dot_escape(&e.from),
            dot_escape(&e.to)
        );
    }
    s.push_str("}\n");
    s
}

fn render_mermaid(export: &GraphExport) -> String {
    use std::fmt::Write as _;
    // Mermaid identifiers can't contain `/` etc.; label carries the real id.
    let ident = |id: &str| -> String {
        id.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect()
    };
    let mut s = String::from("flowchart TB\n");
    let layers = [
        ("page", "Pages"),
        ("template", "Templates"),
        ("organism", "Organisms"),
        ("molecule", "Molecules"),
        ("atom", "Atoms"),
        ("reference", "References"),
    ];
    for (layer, title) in layers {
        let members: Vec<&Node> = export.nodes.iter().filter(|n| n.layer == layer).collect();
        if members.is_empty() {
            continue;
        }
        let _ = writeln!(s, "  subgraph {title}");
        for n in members {
            let _ = writeln!(s, "    {}[\"{}\"]", ident(&n.id), mermaid_text(&n.id));
        }
        let _ = writeln!(s, "  end");
    }
    for e in &export.edges {
        let arrow = match e.kind {
            "mounts" => "==>".to_string(),
            "includes" => "-.->".to_string(),
            "hosts" => "---".to_string(),
            _ => match &e.alias {
                Some(a) => format!("-- {} -->", mermaid_text(a)),
                None => "-->".to_string(),
            },
        };
        let _ = writeln!(s, "  {} {arrow} {}", ident(&e.from), ident(&e.to));
    }
    for (layer, _) in layers {
        let members: Vec<&Node> = export.nodes.iter().filter(|n| n.layer == layer).collect();
        if members.is_empty() {
            continue;
        }
        let ids: Vec<String> = members.iter().map(|n| ident(&n.id)).collect();
        let _ = writeln!(
            s,
            "  classDef l_{layer} fill:{},color:#fff;\n  class {} l_{layer}",
            layer_fill(layer),
            ids.join(",")
        );
    }
    s
}

/// Escape a string for a DOT double-quoted string (id, label, tooltip). Current unit-id
/// rules make `"` unreachable, but the renderer must not depend on that staying true.
fn dot_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Escape a string for Mermaid label/edge-text positions (Mermaid uses `#...;` entities).
fn mermaid_text(s: &str) -> String {
    s.replace('"', "#quot;")
        .replace('<', "#lt;")
        .replace('>', "#gt;")
}

/// The self-contained viewer: the template ships inside the binary; the export is
/// embedded as JSON. `</` is escaped so a description can't close the script tag, and
/// `<!--` so an XML-mode parser can't treat it as a script-block terminator either;
/// HTML-escaping of field values is the viewer's job at its innerHTML sinks.
fn render_html(export: &GraphExport) -> String {
    const TEMPLATE: &str = include_str!("../assets/graph.html");
    let json = serde_json::to_string(export)
        .expect("export serializes")
        .replace("</", "<\\/")
        .replace("<!--", "<\\!--");
    TEMPLATE.replace("__GRAPH_JSON__", &json)
}

/// Skills reached by no entry mount and no import — same rule as shake's orphan warning,
/// recomputed here so the flag is structural rather than scraped from diagnostics.
fn orphans(catalog: &Catalog, cfg: &FrameworkConfig, resolution: &Resolution) -> BTreeSet<String> {
    let mut roots: Vec<String> = cfg
        .targets()
        .flat_map(|t| t.mounts.iter())
        .filter(|m| catalog.skills.contains_key(*m))
        .cloned()
        .collect();
    roots.extend(
        catalog
            .skills
            .values()
            .filter(|s| s.applies_to_all)
            .map(|s| s.id.clone()),
    );
    let mut reachable = BTreeSet::new();
    let mut stack = roots;
    while let Some(id) = stack.pop() {
        if !catalog.skills.contains_key(&id) {
            continue;
        }
        if reachable.insert(id.clone()) {
            for ((sk, _), tid) in &resolution.marker_targets {
                if *sk == id {
                    stack.push(tid.clone());
                }
            }
        }
    }
    catalog
        .skills
        .keys()
        .filter(|id| !reachable.contains(*id))
        .cloned()
        .collect()
}

/// Blocks transitively reachable from any skill's `@include`s.
fn reachable_from_skills(catalog: &Catalog) -> BTreeSet<String> {
    let roots: Vec<String> = catalog
        .skills
        .values()
        .flat_map(|s| s.includes.iter().cloned())
        .collect();
    transitive_blocks(catalog, &roots)
}

fn sorted(set: Option<&BTreeSet<String>>) -> Vec<String> {
    set.map(|s| s.iter().cloned().collect()).unwrap_or_default()
}

fn relative_source(path: &Path, root: &Path) -> Option<String> {
    let p = path.strip_prefix(root).unwrap_or(path);
    Some(p.display().to_string())
}
