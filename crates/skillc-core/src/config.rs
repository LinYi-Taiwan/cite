//! Framework config (T008): the single authoritative registry (Vite-config analog).
//!
//! It is the only place targets are registered (FR-019/020); entry mounts and per-target
//! reference filenames validate against it. See `contracts/framework-config.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::diagnostics::{Code, Diagnostic};
use crate::model::{is_safe_segment, ReferenceKind};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    targets: BTreeMap<String, RawTarget>,
    #[serde(default)]
    agents: Vec<String>,
    #[serde(rename = "sharedReferences", default)]
    shared_references: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTarget {
    #[serde(default)]
    entry: RawEntry,
    #[serde(rename = "defaultAgent", default)]
    default_agent: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    #[serde(default)]
    mounts: Vec<String>,
}

/// A compilation destination (FR-019/020).
#[derive(Debug, Clone)]
pub struct Target {
    pub name: String,
    /// Top-level skills the target wants mounted (FR-014a).
    pub mounts: Vec<String>,
    pub default_agent: Option<String>,
}

/// The processed, validated framework config.
#[derive(Debug, Clone)]
pub struct FrameworkConfig {
    targets: BTreeMap<String, Target>,
    agents: Vec<String>,
    shared_references: BTreeSet<String>,
}

impl FrameworkConfig {
    /// Load and validate a config from an explicit path.
    pub fn load(path: &Path) -> Result<Self, Diagnostic> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            Diagnostic::error(
                Code::ConfigInvalid,
                format!("cannot read config `{}`: {e}", path.display()),
            )
        })?;
        Self::from_yaml(&text, path)
    }

    /// Discover the config in `dir` using the default filenames, then load it.
    pub fn discover(dir: &Path, explicit: Option<&Path>) -> Result<Self, Diagnostic> {
        if let Some(p) = explicit {
            return Self::load(p);
        }
        for name in ["skillc.config.yaml", "skillc.config.yml"] {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Self::load(&candidate);
            }
        }
        Err(Diagnostic::error(
            Code::ConfigInvalid,
            format!(
                "no framework config found in `{}` (expected skillc.config.yaml)",
                dir.display()
            ),
        ))
    }

    pub fn from_yaml(text: &str, path: &Path) -> Result<Self, Diagnostic> {
        let raw: RawConfig = serde_norway::from_str(text).map_err(|e| {
            Diagnostic::error(
                Code::ConfigInvalid,
                format!("invalid config `{}`: {e}", path.display()),
            )
        })?;

        let agents = raw.agents;
        let shared_references: BTreeSet<String> = raw.shared_references.into_iter().collect();

        // Target/agent names and shared-reference stems become filesystem path components
        // (dist/<target>/<agent>/, references/<stem>.md), so they must be safe segments —
        // a name like `../../x` would otherwise let a crafted config escape the out dir
        // (including the `remove_dir_all` in emit). See the security review.
        for name in raw.targets.keys() {
            if !is_safe_segment(name) {
                return Err(Diagnostic::error(
                    Code::ConfigInvalid,
                    format!("target name `{name}` is not a safe path segment"),
                ));
            }
        }
        for agent in &agents {
            if !is_safe_segment(agent) {
                return Err(Diagnostic::error(
                    Code::ConfigInvalid,
                    format!("agent name `{agent}` is not a safe path segment"),
                ));
            }
        }
        for stem in &shared_references {
            if !is_safe_segment(stem) {
                return Err(Diagnostic::error(
                    Code::ConfigInvalid,
                    format!("sharedReferences stem `{stem}` is not a safe path segment"),
                ));
            }
        }

        let mut targets = BTreeMap::new();
        for (name, rt) in raw.targets {
            if let Some(da) = &rt.default_agent {
                if !agents.iter().any(|a| a == da) {
                    return Err(Diagnostic::error(
                        Code::ConfigInvalid,
                        format!("target `{name}` defaultAgent `{da}` is not in agents"),
                    ));
                }
            }
            targets.insert(
                name.clone(),
                Target {
                    name,
                    mounts: rt.entry.mounts,
                    default_agent: rt.default_agent,
                },
            );
        }

        Ok(Self {
            targets,
            agents,
            shared_references,
        })
    }

    pub fn target(&self, name: &str) -> Option<&Target> {
        self.targets.get(name)
    }

    pub fn targets(&self) -> impl Iterator<Item = &Target> {
        self.targets.values()
    }

    pub fn target_names(&self) -> BTreeSet<String> {
        self.targets.keys().cloned().collect()
    }

    pub fn is_target(&self, name: &str) -> bool {
        self.targets.contains_key(name)
    }

    pub fn agents(&self) -> &[String] {
        &self.agents
    }

    pub fn is_agent(&self, name: &str) -> bool {
        self.agents.iter().any(|a| a == name)
    }

    pub fn shared_references(&self) -> &BTreeSet<String> {
        &self.shared_references
    }

    /// Classify a reference stem against the registry (FR-015/018).
    ///
    /// A stem that is a registered target is `per-target`; one declared shared is
    /// `Shared`; anything else is `Stray`. A stem that is *both* a target and declared
    /// shared resolves as `PerTarget` (the registry wins; the collision is warned
    /// separately in validate).
    pub fn classify_reference(&self, stem: &str) -> ReferenceKind {
        if self.is_target(stem) {
            ReferenceKind::PerTarget
        } else if self.shared_references.contains(stem) {
            ReferenceKind::Shared
        } else {
            ReferenceKind::Stray
        }
    }
}

/// Convenience: resolve the default config path under a catalog dir (for messages).
pub fn default_config_path(catalog: &Path) -> PathBuf {
    catalog.join("skillc.config.yaml")
}
