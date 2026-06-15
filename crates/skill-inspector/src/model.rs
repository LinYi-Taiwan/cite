//! Core entities (data-model.md). All `Serialize` with stable field names matching
//! `contracts/inventory-export.md`. No timestamps live here — those are confined to
//! `state.rs` so the inventory export stays snapshot-deterministic.

use serde::Serialize;

/// Real on-disk + settings state of an installed skill (data-model.md §Skill).
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum SkillState {
    /// Present in an active root, not overridden off anywhere relevant.
    Active,
    /// Tier-1: `skillOverrides[<id>]="off"` in the target folder's `settings.local.json`.
    DisabledInFolder,
    /// Tier-2: directory moved to the tool quarantine.
    DisabledGlobal,
}

/// One installed skill instance discovered under a source root (FR-001..005, FR-019).
#[derive(Serialize, Clone, Debug)]
pub struct Skill {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub source_id: String,
    pub agent: String,
    pub path: String,
    pub content_hash: String,
    pub state: SkillState,
    pub metadata_complete: bool,
    pub labels: Vec<String>,
}

impl Skill {
    /// Stable cross-source key `agent/source_id/id` used everywhere (clusters, activation,
    /// context, edges, labels, action requests).
    pub fn key(&self) -> String {
        format!("{}/{}/{}", self.agent, self.source_id, self.id)
    }
}

/// `agent/source_id/id` — parse a skill key back into its parts. `source_id` may itself
/// contain `:` (e.g. `claude:plugin:foo`) but never `/`, so a 2-split on `/` from each end
/// is unambiguous.
pub fn split_key(key: &str) -> Option<(String, String, String)> {
    let (agent, rest) = key.split_once('/')?;
    let (source_id, id) = rest.split_once('/')?;
    if agent.is_empty() || source_id.is_empty() || id.is_empty() {
        return None;
    }
    Some((agent.to_string(), source_id.to_string(), id.to_string()))
}

/// The kind of origin a source represents.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    User,
    Project,
    Plugin,
    OtherAgent,
}

/// Whether a source root could be read this scan.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Availability {
    Readable,
    Missing,
    Unreadable,
}

/// An origin scanned for skills (data-model.md §SkillSource).
#[derive(Serialize, Clone, Debug)]
pub struct SkillSource {
    pub id: String,
    pub agent: String,
    pub kind: SourceKind,
    pub root: String,
    pub availability: Availability,
}

/// Advisory grouping of skills (data-model.md §OverlapCluster). Never causes mutation.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum ClusterKind {
    DuplicateIdentity,
    Similarity,
}

#[derive(Serialize, Clone, Debug)]
pub struct OverlapCluster {
    pub id: String,
    pub kind: ClusterKind,
    pub members: Vec<String>,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
}

/// Per-skill activation in a given context, computed purely from disk (data-model.md §ActivationState).
#[derive(Serialize, Clone, Debug)]
pub struct ActivationState {
    pub skill_key: String,
    pub context: String,
    pub eligible: bool,
    pub active: bool,
}

/// Whether an agent exposed any runtime-load record for a turn.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ContextAvailability {
    Present,
    Unavailable,
}

/// Best-effort, vendor-gated record of what an agent actually loaded (data-model.md §ContextLoadRecord).
#[derive(Serialize, Clone, Debug)]
pub struct ContextLoadRecord {
    pub turn_ref: String,
    pub availability: ContextAvailability,
    pub loaded_skill_keys: Vec<String>,
}
