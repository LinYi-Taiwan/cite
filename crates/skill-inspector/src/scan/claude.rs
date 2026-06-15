//! Claude Code source provider (contracts/source-provider.md §MVP provider).
//!
//! Roots discovered (verified against the live machine, mirrored in `tests/fixtures`):
//! - `claude:user`        → `<home>/.claude/skills/`
//! - `claude:project`     → `<project_root>/.claude/skills/`
//! - `claude:plugin:<name>` → `<installPath>/skills/` for each entry in
//!   `<home>/.claude/plugins/installed_plugins.json` (one per installed plugin).
//!
//! On-machine layout confirmed: `installed_plugins.json` maps `"<name>@<marketplace>"` →
//! `[{ installPath, scope, ... }]`, and skills live at `<installPath>/skills/<id>/SKILL.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::model::{Availability, SkillSource, SourceKind};
use crate::scan::source::{classify_availability, ScanContext, SourceProvider};

pub struct ClaudeProvider;

impl ClaudeProvider {
    pub const AGENT: &'static str = "claude-code";

    fn plugins_dir(home: &Path) -> PathBuf {
        home.join(".claude").join("plugins")
    }

    /// Parse `installed_plugins.json` → `(name, skills_root)` per installed plugin.
    /// A relative `installPath` is resolved against the plugins dir (real entries are
    /// absolute and pass through unchanged). Robust to a missing/garbled file (returns []).
    fn discover_plugins(home: &Path) -> Vec<(String, PathBuf)> {
        let plugins_dir = Self::plugins_dir(home);
        let manifest = plugins_dir.join("installed_plugins.json");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            return Vec::new();
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
            return Vec::new();
        };
        let Some(plugins) = json.get("plugins").and_then(|p| p.as_object()) else {
            return Vec::new();
        };

        // BTreeMap → deterministic ordering, dedup by source name (first install wins).
        let mut by_name: BTreeMap<String, PathBuf> = BTreeMap::new();
        for (key, installs) in plugins {
            let name = key.split('@').next().unwrap_or(key).to_string();
            if by_name.contains_key(&name) {
                continue;
            }
            let Some(arr) = installs.as_array() else {
                continue;
            };
            for install in arr {
                let Some(install_path) = install.get("installPath").and_then(|p| p.as_str()) else {
                    continue;
                };
                let resolved = {
                    let p = PathBuf::from(install_path);
                    if p.is_absolute() {
                        p
                    } else {
                        plugins_dir.join(p)
                    }
                };
                by_name.insert(name.clone(), resolved.join("skills"));
                break;
            }
        }
        by_name.into_iter().collect()
    }
}

impl SourceProvider for ClaudeProvider {
    fn agent(&self) -> &str {
        Self::AGENT
    }

    fn sources(&self, ctx: &ScanContext) -> Vec<SkillSource> {
        let mut sources = Vec::new();

        let user_root = ctx.home.join(".claude").join("skills");
        sources.push(SkillSource {
            id: "claude:user".to_string(),
            agent: Self::AGENT.to_string(),
            kind: SourceKind::User,
            availability: classify_availability(&user_root),
            root: user_root.to_string_lossy().into_owned(),
        });

        let project_root = ctx.project_root.join(".claude").join("skills");
        sources.push(SkillSource {
            id: "claude:project".to_string(),
            agent: Self::AGENT.to_string(),
            kind: SourceKind::Project,
            availability: classify_availability(&project_root),
            root: project_root.to_string_lossy().into_owned(),
        });

        for (name, skills_root) in Self::discover_plugins(&ctx.home) {
            sources.push(SkillSource {
                id: format!("claude:plugin:{name}"),
                agent: Self::AGENT.to_string(),
                kind: SourceKind::Plugin,
                availability: classify_availability(&skills_root),
                root: skills_root.to_string_lossy().into_owned(),
            });
        }

        sources
    }
}

/// Per-repo disable for Claude user/project skills uses `skillOverrides` (research.md §4).
/// Plugin skills are NOT affected by `skillOverrides` (docs/en/skills) — they are disabled
/// per-repo via a `permissions.deny` rule instead (see `plugin_skill_perm_name`).
pub fn supports_folder_scope(source_id: &str) -> bool {
    source_id == "claude:user" || source_id == "claude:project"
}

/// True for a Claude plugin source (`claude:plugin:<plugin>`).
pub fn is_plugin_source(source_id: &str) -> bool {
    source_id.starts_with("claude:plugin:")
}

/// The `Skill(<name>)` permission-name for a plugin skill: `<plugin>:<id>`, where `<plugin>`
/// is the `claude:plugin:<plugin>` source suffix and `<id>` the skill directory name. Plugin
/// skills are namespaced `plugin-name:skill-name` (docs/en/skills L110/L244), so a per-repo
/// disable writes `permissions.deny += "Skill(<plugin>:<id>)"`. `None` for non-plugin sources.
///
/// NOTE: the `<plugin>` segment is derived from cite's `installed_plugins.json` key (before
/// `@`). On a real machine this should match the namespace Claude uses for the rule; confirm
/// against a live plugin before relying on it (the doc fixes the *form*, not this exact value).
///
/// Both segments are validated against a strict allowlist before being embedded — an attacker
/// who can write the plugins manifest or create a skill dir must not be able to inject `)`,
/// whitespace, etc. into the `Skill(<name>)` deny rule (which would otherwise round-trip
/// inconsistently and silently neuter the per-repo kill-switch). Invalid ⇒ `None`, and callers
/// surface that as `bad_plugin_source` rather than writing a broken rule.
pub fn plugin_skill_perm_name(source_id: &str, id: &str) -> Option<String> {
    let plugin = source_id.strip_prefix("claude:plugin:")?;
    if !is_valid_perm_segment(plugin) || !is_valid_perm_segment(id) {
        return None;
    }
    Some(format!("{plugin}:{id}"))
}

/// A `Skill(<plugin>:<id>)` segment must be a non-empty run of `[A-Za-z0-9_.-]` — the shape real
/// plugin names and skill ids take, and nothing that can break out of the rule's parentheses or
/// the rule's `:` separator.
fn is_valid_perm_segment(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

/// `Availability::Readable` predicate sugar used by `scan`.
pub fn is_readable(a: Availability) -> bool {
    matches!(a, Availability::Readable)
}
