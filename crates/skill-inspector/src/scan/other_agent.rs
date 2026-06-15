//! A second `SourceProvider` (US5, FR-019) proving the trait+registry extends without a
//! rewrite. Models a hypothetical agent that stores skills at `<home>/.other-agent/skills/`
//! using the same `skills/<id>/SKILL.md` layout (the convergence note in research.md §2).
//!
//! It has no native per-folder toggle, so disable falls back to Tier-2 quarantine.

use crate::model::{SkillSource, SourceKind};
use crate::scan::source::{classify_availability, ScanContext, SourceProvider};

pub struct OtherAgentProvider;

impl OtherAgentProvider {
    pub const AGENT: &'static str = "other-agent";
}

impl SourceProvider for OtherAgentProvider {
    fn agent(&self) -> &str {
        Self::AGENT
    }

    fn sources(&self, ctx: &ScanContext) -> Vec<SkillSource> {
        let root = ctx.home.join(".other-agent").join("skills");
        vec![SkillSource {
            id: "other-agent:user".to_string(),
            agent: Self::AGENT.to_string(),
            kind: SourceKind::OtherAgent,
            availability: classify_availability(&root),
            root: root.to_string_lossy().into_owned(),
        }]
    }
}
