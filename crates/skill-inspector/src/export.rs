//! Inventory export (contracts/inventory-export.md): a superset of the existing
//! `GraphExport { version, nodes, edges }` so `graph.html` renders it with minimal change;
//! extra top-level keys carry inventory / overlap / activation / context. Deterministic
//! (stable ordering, rounded scores, no timestamps) so it is snapshot-testable.

use serde::Serialize;

use crate::activation;
use crate::context;
use crate::model::{
    ActivationState, ClusterKind, ContextLoadRecord, OverlapCluster, Skill, SkillSource, SkillState,
};
use crate::overlap;
use crate::scan::source::ScanContext;
use crate::scan::Inventory;

#[derive(Serialize)]
pub struct GeneratedFor {
    pub agents: Vec<String>,
    pub project: String,
}

/// A node of the graph.html projection (derived from `skills`; not the source of truth).
#[derive(Serialize)]
pub struct Node {
    pub id: String,
    pub kind: &'static str,
    pub layer: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub external: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub source: String,
    pub flags: Vec<&'static str>,
}

/// An edge of the projection (overlap links between clustered skills).
#[derive(Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: &'static str,
}

#[derive(Serialize)]
pub struct Export {
    pub version: u32,
    pub generated_for: GeneratedFor,
    pub sources: Vec<SkillSource>,
    pub skills: Vec<Skill>,
    pub clusters: Vec<OverlapCluster>,
    pub clusters_empty_reason: Option<String>,
    pub activation: Vec<ActivationState>,
    pub context_loads: Vec<ContextLoadRecord>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// Build the full export for an already-computed inventory.
pub fn build(agents: &[String], ctx: &ScanContext, inventory: Inventory) -> Export {
    let project = ctx.project_root.to_string_lossy().into_owned();

    let clusters = overlap::detect(&inventory.skills);
    let clusters_empty_reason = if clusters.is_empty() {
        Some(overlap::empty_reason())
    } else {
        None
    };

    let activation = activation::compute(&inventory, &project);
    let context_loads = context::collect(agents, &ctx.home);

    let nodes = project_nodes(&inventory.skills, &inventory.sources);
    let edges = project_edges(&clusters);

    Export {
        version: 1,
        generated_for: GeneratedFor {
            agents: agents.to_vec(),
            project,
        },
        sources: inventory.sources,
        skills: inventory.skills,
        clusters,
        clusters_empty_reason,
        activation,
        context_loads,
        nodes,
        edges,
    }
}

fn project_nodes(skills: &[Skill], sources: &[SkillSource]) -> Vec<Node> {
    use crate::model::SourceKind;
    let external_sources: std::collections::BTreeSet<&str> = sources
        .iter()
        .filter(|s| matches!(s.kind, SourceKind::Plugin | SourceKind::OtherAgent))
        .map(|s| s.id.as_str())
        .collect();

    let mut nodes: Vec<Node> = skills
        .iter()
        .map(|s| {
            let mut flags = Vec::new();
            if !s.metadata_complete {
                flags.push("incomplete");
            }
            match s.state {
                SkillState::DisabledInFolder => flags.push("disabled-in-folder"),
                SkillState::DisabledGlobal => flags.push("disabled-global"),
                SkillState::Active => {}
            }
            Node {
                id: s.key(),
                kind: "skill",
                layer: s.source_id.clone(),
                external: external_sources.contains(s.source_id.as_str()),
                description: s.description.clone(),
                source: s.source_id.clone(),
                flags,
            }
        })
        .collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    nodes
}

fn project_edges(clusters: &[OverlapCluster]) -> Vec<Edge> {
    let mut edges = Vec::new();
    for c in clusters {
        let kind = match c.kind {
            ClusterKind::Similarity => "overlap",
            ClusterKind::DuplicateIdentity => "duplicate",
        };
        // Star from the first member to each other (members are sorted ⇒ deterministic).
        if let Some(first) = c.members.first() {
            for m in c.members.iter().skip(1) {
                edges.push(Edge {
                    from: first.clone(),
                    to: m.clone(),
                    kind,
                });
            }
        }
    }
    edges.sort_by(|a, b| (a.from.as_str(), a.to.as_str()).cmp(&(b.from.as_str(), b.to.as_str())));
    edges
}
