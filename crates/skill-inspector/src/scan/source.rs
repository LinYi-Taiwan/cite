//! `SourceProvider` trait + registry (contracts/source-provider.md).
//!
//! Adding an agent (US5, FR-019) = implementing this trait and registering it. The walker
//! (`<id>/SKILL.md`, in `skill_md.rs`) is shared; only **root discovery** differs per agent.

use std::path::{Path, PathBuf};

use crate::model::{Availability, SkillSource};

/// Inputs a provider needs to locate its roots.
pub struct ScanContext {
    /// Project root for project-level discovery + activation context.
    pub project_root: PathBuf,
    /// User home for user-level + plugin discovery.
    pub home: PathBuf,
}

/// Discovers the skill roots for one agent on this machine. Read-only; never hard-errors
/// on a missing/unreadable root (classify via `availability`).
pub trait SourceProvider {
    /// Stable agent label, e.g. `"claude-code"`.
    fn agent(&self) -> &str;

    /// Enumerate the roots to scan. Each carries kind + path + availability.
    fn sources(&self, ctx: &ScanContext) -> Vec<SkillSource>;
}

/// Classify a root path into a `readable | missing | unreadable` availability without
/// erroring the scan (the core graceful-degradation contract).
pub fn classify_availability(root: &Path) -> Availability {
    if !root.exists() {
        return Availability::Missing;
    }
    match std::fs::read_dir(root) {
        Ok(_) => Availability::Readable,
        Err(_) => Availability::Unreadable,
    }
}

/// Provider registry keyed by agent id.
#[derive(Default)]
pub struct Registry {
    providers: Vec<Box<dyn SourceProvider>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// The default set of providers shipped with the tool.
    pub fn with_defaults() -> Self {
        let mut r = Self::new();
        r.register(Box::new(crate::scan::claude::ClaudeProvider));
        r.register(Box::new(crate::scan::other_agent::OtherAgentProvider));
        r
    }

    pub fn register(&mut self, provider: Box<dyn SourceProvider>) {
        self.providers.push(provider);
    }

    pub fn get(&self, agent: &str) -> Option<&dyn SourceProvider> {
        self.providers
            .iter()
            .find(|p| p.agent() == agent)
            .map(|b| b.as_ref())
    }
}
