//! `skillc why <id>` — the reverse-dependency query (maintainer tooling).
//!
//! Answers, for a block or skill, the questions a maintainer must ask before touching it:
//! *who includes/imports this, which skills ship it transitively, and which targets does
//! it land in?* — without grepping the catalog or diffing manifests. Read-only; built on
//! the same parse/resolve/shake passes as `build`, so the answer is the compiler's truth,
//! not a text-match approximation.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::config::FrameworkConfig;
use crate::model::Catalog;
use crate::resolve::Resolution;
use crate::shake;

/// The `why` report for one unit, render-ready and deterministic.
pub struct WhyReport {
    pub text: String,
}

/// Look up `id` as a block or skill and explain its reverse dependencies. `None` when the
/// id matches no unit (caller renders the not-found error with the catalog inventory).
pub fn why(
    catalog: &Catalog,
    cfg: &FrameworkConfig,
    resolution: &Resolution,
    id: &str,
) -> Option<WhyReport> {
    if catalog.blocks.contains_key(id) {
        Some(why_block(catalog, cfg, resolution, id))
    } else if catalog.skills.contains_key(id) {
        Some(why_skill(catalog, cfg, resolution, id))
    } else if resolution.pulled.contains_key(id) {
        Some(why_pulled(catalog, cfg, resolution, id))
    } else {
        None
    }
}

/// A cross-repo skill pulled in as a dependency: not a catalog member, but the question
/// "who depends on this external skill and where does it ship" is exactly what a
/// maintainer asks before bumping/removing the import.
fn why_pulled(
    catalog: &Catalog,
    cfg: &FrameworkConfig,
    resolution: &Resolution,
    id: &str,
) -> WhyReport {
    let imported_by: Vec<String> = resolution
        .marker_targets
        .iter()
        .filter(|((_, _), tid)| tid.as_str() == id)
        .map(|((sk, alias), _)| format!("{sk} (as {{{{{alias}}}}})"))
        .collect();
    let by_target = targets_shipping(catalog, cfg, resolution, &[id]);

    let mut text = format!("skill `{id}` (external, pulled cross-repo)\n");
    push_list(&mut text, "imported by skills", &imported_by);
    push_targets(&mut text, &by_target);
    WhyReport { text }
}

/// Render the catalog inventory for not-found errors / discovery.
pub fn inventory(catalog: &Catalog) -> String {
    let skills: Vec<&str> = catalog.skills.keys().map(String::as_str).collect();
    let blocks: Vec<&str> = catalog.blocks.keys().map(String::as_str).collect();
    format!(
        "skills: {}\nblocks: {}",
        if skills.is_empty() {
            "(none)".to_string()
        } else {
            skills.join(", ")
        },
        if blocks.is_empty() {
            "(none)".to_string()
        } else {
            blocks.join(", ")
        },
    )
}

fn why_block(
    catalog: &Catalog,
    cfg: &FrameworkConfig,
    resolution: &Resolution,
    id: &str,
) -> WhyReport {
    // Direct includers, by unit kind.
    let direct_skills: Vec<&str> = catalog
        .skills
        .values()
        .filter(|s| s.includes.iter().any(|b| b == id))
        .map(|s| s.id.as_str())
        .collect();
    let direct_blocks: Vec<&str> = catalog
        .blocks
        .values()
        .filter(|b| b.id != id && b.includes.iter().any(|i| i == id))
        .map(|b| b.id.as_str())
        .collect();

    // Skills that transitively inline this block (through nested block chains).
    let shipping_skills: Vec<&str> = catalog
        .skills
        .values()
        .filter(|s| transitive_blocks(catalog, &s.includes).contains(id))
        .map(|s| s.id.as_str())
        .collect();

    // Targets whose bundles contain a shipping skill.
    let by_target = targets_shipping(catalog, cfg, resolution, &shipping_skills);

    let mut text = format!("block `{id}`\n");
    push_list(&mut text, "directly included by skills", &direct_skills);
    push_list(&mut text, "directly included by blocks", &direct_blocks);
    push_list(
        &mut text,
        "transitively inlined into skills",
        &shipping_skills,
    );
    push_targets(&mut text, &by_target);
    WhyReport { text }
}

fn why_skill(
    catalog: &Catalog,
    cfg: &FrameworkConfig,
    resolution: &Resolution,
    id: &str,
) -> WhyReport {
    let skill = &catalog.skills[id];

    // Entries that mount it.
    let mounted_by: Vec<&str> = cfg
        .targets()
        .filter(|t| t.mounts.iter().any(|m| m == id))
        .map(|t| t.name.as_str())
        .collect();

    // Skills importing it via {{Alias}} (resolved edges, not raw text).
    let imported_by: Vec<String> = resolution
        .marker_targets
        .iter()
        .filter(|((_, _), tid)| tid.as_str() == id)
        .map(|((sk, alias), _)| format!("{sk} (as {{{{{alias}}}}})"))
        .collect();

    // Targets whose bundle ships it.
    let by_target = targets_shipping(catalog, cfg, resolution, &[id]);

    // What it pulls in itself: transitive blocks + resolved imports.
    let blocks: Vec<String> = {
        let all = transitive_blocks(catalog, &skill.includes);
        all.into_iter()
            .map(|b| {
                if skill.includes.contains(&b) {
                    b
                } else {
                    format!("{b} (transitive)")
                }
            })
            .collect()
    };
    let imports: Vec<String> = resolution
        .marker_targets
        .iter()
        .filter(|((sk, _), _)| sk.as_str() == id)
        .map(|((_, alias), tid)| format!("{tid} (as {{{{{alias}}}}})"))
        .collect();

    let mut text = format!("skill `{id}`");
    if skill.applies_to_all {
        text.push_str(" (appliesTo: all)");
    }
    text.push('\n');
    push_list(&mut text, "mounted by targets", &mounted_by);
    push_list(&mut text, "imported by skills", &imported_by);
    push_targets(&mut text, &by_target);
    push_list(&mut text, "inlines blocks", &blocks);
    push_list(&mut text, "imports skills", &imports);
    WhyReport { text }
}

/// All blocks reachable from `roots` through nested block `@include`s.
fn transitive_blocks(catalog: &Catalog, roots: &[String]) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<String> = roots.to_vec();
    while let Some(b) = stack.pop() {
        if let Some(block) = catalog.blocks.get(&b) {
            if seen.insert(b) {
                stack.extend(block.includes.iter().cloned());
            }
        }
    }
    seen
}

/// For each target, the subset of `skills` its bundle membership contains, annotated with
/// the membership reason (`mounted` / `pulled-by:<skill>`).
fn targets_shipping<S: AsRef<str>>(
    catalog: &Catalog,
    cfg: &FrameworkConfig,
    resolution: &Resolution,
    skills: &[S],
) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    for target in cfg.targets() {
        // Membership is recomputed per target. `why` is read-only and `check`/`build` own
        // full reporting, but a target whose shake errored (e.g. entry/missing) must not
        // be presented as a clean answer — annotate it instead of discarding the signal.
        let mut scratch = crate::diagnostics::Diagnostics::new();
        let membership = shake::shake(catalog, cfg, resolution, &target.name, &mut scratch);
        let broken = scratch.has_errors();
        let mut hits: Vec<String> = Vec::new();
        for s in skills {
            let s = s.as_ref();
            if membership.members.contains(s) || membership.pulled.contains(s) {
                let reason = membership
                    .reasons
                    .get(s)
                    .cloned()
                    .unwrap_or_else(|| "pulled".to_string());
                hits.push(format!("{s} ({reason})"));
            }
        }
        if broken && !hits.is_empty() {
            hits.push("[target has errors — run `skillc check`]".to_string());
        }
        if !hits.is_empty() {
            out.insert(target.name.clone(), hits);
        }
    }
    out
}

fn push_list<S: AsRef<str>>(text: &mut String, label: &str, items: &[S]) {
    if items.is_empty() {
        let _ = writeln!(text, "  {label}: (none)");
    } else {
        let rendered: Vec<&str> = items.iter().map(AsRef::as_ref).collect();
        let _ = writeln!(text, "  {label}: {}", rendered.join(", "));
    }
}

fn push_targets(text: &mut String, by_target: &BTreeMap<String, Vec<String>>) {
    if by_target.is_empty() {
        let _ = writeln!(text, "  ships in targets: (none)");
    } else {
        let _ = writeln!(text, "  ships in targets:");
        for (target, vias) in by_target {
            let _ = writeln!(text, "    {target}: {}", vias.join(", "));
        }
    }
}
