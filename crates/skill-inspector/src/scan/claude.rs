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

/// Component paths a plugin's `.claude-plugin/plugin.json` explicitly declares. Claude loads a
/// plugin's skills/commands from these declared paths *when present*, falling back to the
/// convention dirs (`skills/`, `commands/`) only for a key the manifest omits. So a `None` here
/// means "manifest didn't declare this kind → use convention"; a `Some(vec![])` means "declared
/// none → load none" (do NOT fall back). Each path is resolved against the install dir.
pub struct PluginManifestPaths {
    pub skills: Option<Vec<PathBuf>>,
    pub commands: Option<Vec<PathBuf>>,
}

/// Read the declared skill/command paths from `<install_path>/.claude-plugin/plugin.json`.
/// Missing/garbled manifest, or a key that isn't a string array ⇒ that kind is `None` (convention).
/// A declared path is accepted only if it resolves to a file/dir that genuinely lives inside the
/// install dir: a lexical pre-filter (`is_contained_relative`) rejects absolute paths and `..`
/// components, then a real-path check (`canonicalize` + prefix) rejects anything whose resolved
/// target escapes — catching a `Normal` component (or the leaf itself) that is a symlink to
/// outside. A non-existent path resolves to nothing ⇒ dropped (the agent can't load it either).
pub fn read_plugin_manifest(install_path: &Path) -> PluginManifestPaths {
    let none = PluginManifestPaths {
        skills: None,
        commands: None,
    };
    let manifest = install_path.join(".claude-plugin").join("plugin.json");
    let Ok(text) = std::fs::read_to_string(&manifest) else {
        return none;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return none;
    };
    let Ok(install_canon) = install_path.canonicalize() else {
        return none;
    };
    let declared = |key: &str| -> Option<Vec<PathBuf>> {
        let arr = json.get(key)?.as_array()?;
        Some(
            arr.iter()
                .filter_map(|v| v.as_str())
                .filter(|s| is_contained_relative(s))
                .map(|s| install_path.join(s))
                .filter(|p| resolves_within(&install_canon, p))
                .collect(),
        )
    };
    PluginManifestPaths {
        skills: declared("skills"),
        commands: declared("commands"),
    }
}

/// Lexical pre-filter: a declared manifest path is *potentially* safe only if it's relative (not
/// absolute, not a Windows drive/root) and free of any `..` component. `.` and normal components
/// are fine. Rejects empty too. Necessary but NOT sufficient — a `Normal` component can be a
/// symlink to outside, so `resolves_within` does the real containment check.
fn is_contained_relative(s: &str) -> bool {
    use std::path::Component;
    if s.is_empty() {
        return false;
    }
    Path::new(s)
        .components()
        .all(|c| matches!(c, Component::CurDir | Component::Normal(_)))
}

/// Real-path containment: the resolved (symlink-followed) `path` must exist and sit under
/// `base_canon` (itself already canonicalized). A non-existent path, or one whose canonical target
/// escapes the install dir (e.g. a planted symlink), returns false ⇒ the entry is dropped.
fn resolves_within(base_canon: &Path, path: &Path) -> bool {
    match path.canonicalize() {
        Ok(real) => real.starts_with(base_canon),
        Err(_) => false,
    }
}

/// Resolve a bare plugin `<name>` back to the full `"<name>@<marketplace>"` key that
/// `installed_plugins.json` (and `enabledPlugins`) use. The scanner reduces plugin source ids to
/// the bare name, but writing the `enabledPlugins` switch needs the exact key Claude keys on.
/// Returns the first key (deterministic: keys sorted) whose pre-`@` segment matches — picking the
/// SAME winner as `discover_plugins` on a name collision across marketplaces: `serde_json` has no
/// `preserve_order`, so its `Map` is a sorted `BTreeMap` and `discover_plugins`' "first-seen per
/// name" is also sorted-order-first. `None` if the manifest is missing/garbled or no installed
/// plugin carries that name.
pub fn resolve_plugin_full_key(home: &Path, bare_name: &str) -> Option<String> {
    let manifest = ClaudeProvider::plugins_dir(home).join("installed_plugins.json");
    let text = std::fs::read_to_string(&manifest).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let plugins = json.get("plugins")?.as_object()?;
    let mut keys: Vec<&String> = plugins.keys().collect();
    keys.sort();
    keys.into_iter()
        .find(|k| k.split('@').next().unwrap_or(k) == bare_name)
        .cloned()
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
pub fn is_valid_perm_segment(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

/// `Availability::Readable` predicate sugar used by `scan`.
pub fn is_readable(a: Availability) -> bool {
    matches!(a, Availability::Readable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_declared_paths_win_over_convention_dir() {
        let install = std::env::temp_dir().join(format!("si-manifest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&install);
        std::fs::create_dir_all(install.join(".claude-plugin")).unwrap();
        // A command declared under `test/`, NOT `commands/` — the exact "rename" scenario. The file
        // must exist for the containment check to accept it (the agent couldn't load a phantom).
        std::fs::create_dir_all(install.join("test")).unwrap();
        std::fs::write(install.join("test").join("foo.md"), "x").unwrap();
        std::fs::write(
            install.join(".claude-plugin").join("plugin.json"),
            r#"{"name":"p","commands":["./test/foo.md"],"skills":[]}"#,
        )
        .unwrap();

        let m = read_plugin_manifest(&install);
        assert_eq!(m.commands.as_deref().unwrap(), &[install.join("./test/foo.md")]);
        // `skills: []` is declared-empty → Some(vec![]), which means "load none" (NOT convention).
        assert_eq!(m.skills.as_deref().unwrap().len(), 0);

        let _ = std::fs::remove_dir_all(&install);
    }

    #[test]
    fn manifest_escaping_paths_are_dropped() {
        let install = std::env::temp_dir().join(format!("si-manifest-esc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&install);
        std::fs::create_dir_all(install.join(".claude-plugin")).unwrap();
        std::fs::create_dir_all(install.join("ok")).unwrap();
        std::fs::write(install.join("ok").join("here.md"), "x").unwrap();
        std::fs::write(
            install.join(".claude-plugin").join("plugin.json"),
            r#"{"name":"p","commands":["../../etc/passwd.md","/etc/shadow.md","./ok/here.md"]}"#,
        )
        .unwrap();

        let m = read_plugin_manifest(&install);
        // Only the contained, existing relative path survives; the `..` and absolute entries are
        // dropped lexically, and any non-existent/escaping path is dropped by the real-path check.
        let cmds = m.commands.unwrap();
        assert_eq!(cmds, vec![install.join("./ok/here.md")]);

        let _ = std::fs::remove_dir_all(&install);
    }

    #[cfg(unix)]
    #[test]
    fn manifest_symlink_escape_is_dropped() {
        // A declared component that is a *symlink to outside* the install dir passes the lexical
        // guard but must be rejected by the canonicalize containment check.
        let base = std::env::temp_dir().join(format!("si-manifest-sym-{}", std::process::id()));
        let install = base.join("install");
        let outside = base.join("outside");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(install.join(".claude-plugin")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.md"), "TOP SECRET").unwrap();
        // install/leak -> ../outside  (a symlink whose target escapes the install dir)
        std::os::unix::fs::symlink(&outside, install.join("leak")).unwrap();
        std::fs::write(
            install.join(".claude-plugin").join("plugin.json"),
            r#"{"name":"p","commands":["./leak/secret.md"]}"#,
        )
        .unwrap();

        let m = read_plugin_manifest(&install);
        // The symlinked path canonicalizes outside install ⇒ dropped, not exposed.
        assert_eq!(m.commands.as_deref().unwrap().len(), 0);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn contained_relative_predicate() {
        assert!(is_contained_relative("./commands/x.md"));
        assert!(is_contained_relative("skills/foo"));
        assert!(!is_contained_relative("../escape"));
        assert!(!is_contained_relative("a/../../b"));
        assert!(!is_contained_relative("/abs/path"));
        assert!(!is_contained_relative(""));
    }

    #[test]
    fn missing_manifest_is_none_so_caller_uses_convention() {
        let install = std::env::temp_dir().join(format!("si-manifest-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&install);
        let m = read_plugin_manifest(&install);
        assert!(m.commands.is_none() && m.skills.is_none());
    }
}
