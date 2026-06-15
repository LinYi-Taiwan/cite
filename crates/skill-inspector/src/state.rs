//! `inspector-state.json` — the single tool-owned persistence (contracts/inspector-state.md).
//! Written ONLY by `action/`; read by `scan`/`serve` to reflect `Skill.state` and `labels`.
//! Lives in a tool config dir (never inside a skill root) so the user's roots stay clean.
//!
//! Timestamps (`disabled_at`) are the only nondeterministic field; they live here and are
//! never part of the inventory export, so snapshots stay stable.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

fn default_version() -> u32 {
    1
}

/// One Tier-2 (global) disable — enough to restore the dir to its exact original location.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct QuarantineRecord {
    pub skill_key: String,
    pub id: String,
    pub agent: String,
    pub original_root: String,
    pub original_path: String,
    pub quarantine_path: String,
    pub content_hash: String,
    pub disabled_at: String,
}

/// One Tier-1 (per-folder) disable — bookkeeping/undo for a `skillOverrides` write.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FolderOverrideRecord {
    pub skill_key: String,
    pub agent: String,
    pub folder: String,
    pub settings_path: String,
    pub previous_value: Option<String>,
    pub new_value: String,
    pub disabled_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct InspectorState {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub quarantine: Vec<QuarantineRecord>,
    #[serde(default)]
    pub folder_overrides: Vec<FolderOverrideRecord>,
    #[serde(default)]
    pub labels: BTreeMap<String, Vec<String>>,
}

impl InspectorState {
    /// Tool config dir. Overridable via `SKILL_INSPECTOR_CONFIG_DIR` (used by tests); else
    /// `$XDG_CONFIG_HOME/skill-inspector` or `~/.config/skill-inspector`.
    pub fn config_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("SKILL_INSPECTOR_CONFIG_DIR") {
            return PathBuf::from(dir);
        }
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                PathBuf::from(home).join(".config")
            });
        base.join("skill-inspector")
    }

    pub fn state_path() -> PathBuf {
        Self::config_dir().join("inspector-state.json")
    }

    pub fn quarantine_dir() -> PathBuf {
        Self::config_dir().join("quarantine")
    }

    pub fn trash_dir() -> PathBuf {
        Self::config_dir().join("trash")
    }

    /// Load from the default state path; a missing/unparseable file yields empty state.
    pub fn load() -> Self {
        Self::load_from(&Self::state_path())
    }

    /// Load from an explicit path; tolerate a missing file (empty state). A file that EXISTS
    /// but fails to parse is corrupt — warn loudly to stderr rather than silently dropping
    /// quarantine/override bookkeeping (which would orphan quarantined skills).
    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                eprintln!(
                    "warning: {} is unparseable ({e}); proceeding with empty state. \
                     Any quarantined skills remain on disk but are not tracked.",
                    path.display()
                );
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Persist to an explicit path, creating the parent dir.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    pub fn find_quarantine(&self, skill_key: &str) -> Option<&QuarantineRecord> {
        self.quarantine.iter().find(|r| r.skill_key == skill_key)
    }

    pub fn find_folder_override(
        &self,
        skill_key: &str,
        folder: &str,
    ) -> Option<&FolderOverrideRecord> {
        self.folder_overrides
            .iter()
            .find(|r| r.skill_key == skill_key && r.folder == folder)
    }
}
