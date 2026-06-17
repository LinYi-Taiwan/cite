//! Codex source provider (003-codex-skill-support, contracts/codex-source-provider.md).
//!
//! Roots discovered (verified against the live Codex CLI 0.137.0 + the official docs at
//! <https://developers.openai.com/codex/skills>, mirrored in `tests/fixtures`):
//! - `codex:user` → `<codex_home>/skills/`; skills directly under it are user-authored, skills under
//!   `skills/.system/` are built-in → `built_in=true`. This is where Codex materializes its bundled
//!   built-ins (`.system/imagegen`, …).
//! - `codex:user-agents` → `<home>/.agents/skills/` — the documented PERSONAL skill location Codex
//!   reads in addition to `<codex_home>/skills` (docs: "User: `$HOME/.agents/skills`"). Tied to the
//!   real `$HOME`, NOT `$CODEX_HOME`.
//! - `codex:plugin:<name>` → `<codex_home>/plugins/cache/<marketplace>/<name>/<version>/skills`, one
//!   per `[plugins."<name>@<marketplace>"]` table in `config.toml`.
//! - `codex:project` → `<project_root>/.agents/skills/`. Codex scans `.agents/skills` from the cwd up
//!   to the repo root (docs: "Repository (CWD/Parent/Root)"); cite models that as the one project
//!   scope it carries — `ctx.project_root` (the cwd, or `--project`). Running from the repo root (the
//!   common case) makes cwd == repo root, so this single root covers it.
//!
//! Codex home = `$CODEX_HOME` if set, else `<home>/.codex`. Read-only; never hard-errors.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::model::{SkillSource, SourceKind};
use crate::scan::source::{classify_availability, ScanContext, SourceProvider};

pub struct CodexProvider;

/// The `<codex_home>/skills` source id (built-ins under `.system/` + any legacy user skills).
pub const USER_SOURCE_ID: &str = "codex:user";

/// The `<home>/.agents/skills` source id — Codex's documented personal skill location.
pub const AGENTS_USER_SOURCE_ID: &str = "codex:user-agents";

/// The `<project_root>/.agents/skills` source id — this project's Codex skills.
pub const PROJECT_SOURCE_ID: &str = "codex:project";

/// An installed Codex plugin resolved from `config.toml` + the plugin cache.
struct CodexPlugin {
    /// Bare name (pre-`@`), e.g. `self`.
    bare: String,
    /// The plugin's skills dir under the cache (may not exist → classified Missing).
    skills_root: PathBuf,
}

impl CodexProvider {
    pub const AGENT: &'static str = "codex";

    /// Codex home: `$CODEX_HOME` if set (non-empty), else `<home>/.codex`. `$CODEX_HOME` is trusted
    /// the same way the user's `$HOME` is — this is a local, single-user tool bound to loopback, so
    /// an attacker who can set the process environment already has local code execution. We honour
    /// Codex's own env contract verbatim (no canonicalization) so the inspector reads/writes exactly
    /// the config Codex itself would.
    pub fn codex_home(home: &Path) -> PathBuf {
        match std::env::var_os("CODEX_HOME") {
            Some(v) if !v.is_empty() => PathBuf::from(v),
            _ => home.join(".codex"),
        }
    }

    fn config_path(codex_home: &Path) -> PathBuf {
        codex_home.join("config.toml")
    }

    /// Parse `[plugins."<name>@<marketplace>"]` tables → one `CodexPlugin` per installed plugin,
    /// resolving each one's installed skills dir under the cache. Deterministic (sorted by full key,
    /// first bare-name wins on a collision). Robust to a missing/garbled `config.toml` → `[]`
    /// (mirrors `claude.rs::discover_plugins`).
    fn discover_plugins(codex_home: &Path) -> Vec<CodexPlugin> {
        let mut by_name: BTreeMap<String, CodexPlugin> = BTreeMap::new();
        for p in Self::parsed_plugins(codex_home) {
            if by_name.contains_key(&p.bare) {
                continue;
            }
            // `bare`/`mkt` are validated path-safe in `parsed_plugins`, so they cannot escape the
            // cache dir via `..`, a separator, or an absolute override.
            let plugin_dir = codex_home
                .join("plugins")
                .join("cache")
                .join(&p.mkt)
                .join(&p.bare);
            let skills_root = resolve_version_skills_root(&plugin_dir);
            by_name.insert(
                p.bare.clone(),
                CodexPlugin {
                    bare: p.bare,
                    skills_root,
                },
            );
        }
        by_name.into_values().collect()
    }

    /// The single, validated, sorted source of truth for the installed plugins — every consumer
    /// (path discovery, enabled-state map, full-key recovery) reads this so they always agree on the
    /// winner for a duplicate bare name (sorted-by-full-key, first wins). A plugin key whose bare
    /// name or marketplace is NOT a safe path segment (e.g. `../../.ssh@x`, an absolute path, a key
    /// with a separator) is dropped here — the config is user/tool-writable, so a crafted key must
    /// not be able to steer the scanner's filesystem walk outside the Codex home. Missing/garbled
    /// config → `[]`.
    fn parsed_plugins(codex_home: &Path) -> Vec<PluginEntry> {
        let Some(doc) = read_config(codex_home) else {
            return Vec::new();
        };
        let Some(plugins) = doc.get("plugins").and_then(|i| i.as_table_like()) else {
            return Vec::new();
        };
        let mut out: Vec<PluginEntry> = Vec::new();
        for (key, item) in plugins.iter() {
            let (bare, mkt) = split_plugin_key(key);
            if !is_safe_path_segment(&bare) {
                continue;
            }
            // An empty marketplace (key had no `@`) is tolerated — its cache path simply won't
            // resolve (classified Missing); a present marketplace must itself be path-safe.
            if !mkt.is_empty() && !is_safe_path_segment(&mkt) {
                continue;
            }
            let enabled = item
                .as_table_like()
                .and_then(|t| t.get("enabled"))
                .and_then(|v| v.as_bool());
            out.push(PluginEntry {
                bare,
                mkt,
                full_key: key.to_string(),
                enabled,
            });
        }
        out.sort_by(|a, b| a.full_key.cmp(&b.full_key));
        out
    }

    /// Map of bare plugin name → its `enabled` flag, read fresh from `config.toml` (never cached).
    /// Used by the scanner to set a plugin's skills to `DisabledPlugin` (FR-007, FR-010). A plugin
    /// table with NO `enabled` key is absent from the map → the caller treats it as ACTIVE (a plugin
    /// is inert only when explicitly `enabled = false`, never merely by an omitted key). Missing
    /// config → empty map.
    pub fn plugin_enabled_map(codex_home: &Path) -> BTreeMap<String, bool> {
        let mut out = BTreeMap::new();
        for p in Self::parsed_plugins(codex_home) {
            if let Some(enabled) = p.enabled {
                out.entry(p.bare).or_insert(enabled);
            }
        }
        out
    }

    /// Skills switched off via `[[skills.config]] enabled = false` in `config.toml` — Codex's native
    /// per-skill kill switch (verified against `codex debug prompt-input`: such a skill is dropped
    /// from the loaded set). Returns `(disabled SKILL.md path strings, disabled skill names)`; a
    /// config entry selects by EITHER `path` or `name`. The scanner maps a matching skill to
    /// `DisabledGlobal`. Missing/garbled config → empty sets. Re-read every scan (never cached).
    pub fn disabled_skills(codex_home: &Path) -> (BTreeSet<String>, BTreeSet<String>) {
        let mut paths = BTreeSet::new();
        let mut names = BTreeSet::new();
        let Some(doc) = read_config(codex_home) else {
            return (paths, names);
        };
        let Some(arr) = doc
            .get("skills")
            .and_then(|s| s.as_table_like())
            .and_then(|s| s.get("config"))
            .and_then(|c| c.as_array_of_tables())
        else {
            return (paths, names);
        };
        for t in arr.iter() {
            // Only `enabled = false` entries disable; a path-AND-name or selector-less entry is
            // ignored by Codex itself, so we ignore it too (mirror its `config_rules` behaviour).
            if t.get("enabled").and_then(|v| v.as_bool()) != Some(false) {
                continue;
            }
            let path = t.get("path").and_then(|v| v.as_str());
            let name = t.get("name").and_then(|v| v.as_str());
            match (path, name) {
                (Some(p), None) if !p.is_empty() => {
                    paths.insert(p.to_string());
                }
                (None, Some(n)) if !n.is_empty() => {
                    names.insert(n.to_string());
                }
                _ => {}
            }
        }
        (paths, names)
    }

    /// Recover the full `"<name>@<marketplace>"` config key from a bare plugin name (the analogue of
    /// `claude.rs::resolve_plugin_full_key`). Deterministic: sorted by full key, first pre-`@` match
    /// wins — the same winner `discover_plugins` picks. `None` if config is missing/garbled or no
    /// (path-safe) plugin carries that name.
    pub fn resolve_plugin_full_key(codex_home: &Path, bare_name: &str) -> Option<String> {
        Self::parsed_plugins(codex_home)
            .into_iter()
            .find(|p| p.bare == bare_name)
            .map(|p| p.full_key)
    }
}

/// One installed plugin parsed from `config.toml`: bare name, marketplace, the exact config key, and
/// its `enabled` flag (`None` when the key is omitted → treated as active).
struct PluginEntry {
    bare: String,
    mkt: String,
    full_key: String,
    enabled: Option<bool>,
}

impl SourceProvider for CodexProvider {
    fn agent(&self) -> &str {
        Self::AGENT
    }

    fn sources(&self, ctx: &ScanContext) -> Vec<SkillSource> {
        let codex_home = Self::codex_home(&ctx.home);
        let mut sources = Vec::new();

        // global/user — `<codex_home>/skills`. Built-in `.system` skills are surfaced under the SAME
        // source id by the scanner (it walks the `.system` subtree), flagged `built_in`.
        let user_root = codex_home.join("skills");
        sources.push(SkillSource {
            id: USER_SOURCE_ID.to_string(),
            agent: Self::AGENT.to_string(),
            kind: SourceKind::User,
            availability: classify_availability(&user_root),
            root: user_root.to_string_lossy().into_owned(),
        });

        // personal — `<home>/.agents/skills`, the documented user skill location (separate from
        // `<codex_home>/skills`). Bound to the real `$HOME`, never `$CODEX_HOME`.
        let agents_user_root = ctx.home.join(".agents").join("skills");
        sources.push(SkillSource {
            id: AGENTS_USER_SOURCE_ID.to_string(),
            agent: Self::AGENT.to_string(),
            kind: SourceKind::User,
            availability: classify_availability(&agents_user_root),
            root: agents_user_root.to_string_lossy().into_owned(),
        });

        // project — `<project_root>/.agents/skills`. Codex walks `.agents/skills` from the cwd up to
        // the repo root; cite emits the one project scope it carries (the cwd / `--project`), which
        // is the repo root in the common case. Mirrors `claude.rs`'s single project source.
        // Pushed before plugins so the raw vector reads user → user-agents → project → plugin, the
        // same scope-hierarchy order `claude.rs` uses (the final sort in `scan` is by id regardless).
        let project_root = ctx.project_root.join(".agents").join("skills");
        sources.push(SkillSource {
            id: PROJECT_SOURCE_ID.to_string(),
            agent: Self::AGENT.to_string(),
            kind: SourceKind::Project,
            availability: classify_availability(&project_root),
            root: project_root.to_string_lossy().into_owned(),
        });

        // plugin — one per installed plugin declared in config.toml. A disabled plugin is still
        // emitted (its skills are shown, mapped to DisabledPlugin by the scanner).
        for p in Self::discover_plugins(&codex_home) {
            sources.push(SkillSource {
                id: format!("codex:plugin:{}", p.bare),
                agent: Self::AGENT.to_string(),
                kind: SourceKind::Plugin,
                availability: classify_availability(&p.skills_root),
                root: p.skills_root.to_string_lossy().into_owned(),
            });
        }

        sources
    }
}

/// True for a Codex plugin source (`codex:plugin:<name>`).
pub fn is_plugin_source(source_id: &str) -> bool {
    source_id.starts_with("codex:plugin:")
}

/// Codex has no per-FOLDER (per-repo) skill toggle: a per-skill disable is global, written natively
/// as `[[skills.config]] enabled=false` in `config.toml` (see `action::codex_config::set_skill_enabled`,
/// research §3) — not a `skillOverrides`-style folder scope. So no Codex source supports folder scope.
pub fn supports_folder_scope(_source_id: &str) -> bool {
    false
}

/// Split a `"<name>@<marketplace>"` plugin key into `(bare_name, marketplace)`. A key with no `@`
/// yields an empty marketplace.
fn split_plugin_key(key: &str) -> (String, String) {
    match key.split_once('@') {
        Some((bare, mkt)) => (bare.to_string(), mkt.to_string()),
        None => (key.to_string(), String::new()),
    }
}

/// A path component derived from the (user/tool-writable) `config.toml` is safe to `join` only if it
/// is a single normal segment: non-empty, not `.`/`..`, and free of any path separator. This blocks
/// a crafted plugin key from steering a `Path::join` outside the Codex home via `..`, an embedded
/// separator, or an absolute-path override (`/etc` ⇒ contains `/` ⇒ rejected). Mirrors
/// `action/mod.rs::is_safe_component`.
fn is_safe_path_segment(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && !s.contains('/') && !s.contains('\\')
}

/// Resolve `<plugin_dir>/<version>/skills`, picking the highest-sorted installed version dir. A
/// real install carries exactly one version; on the (unexpected) multi-version case the lexically
/// greatest is chosen for determinism. No version dir ⇒ a `<plugin_dir>/skills` path that won't
/// exist (classified Missing), never an error.
fn resolve_version_skills_root(plugin_dir: &Path) -> PathBuf {
    let version = std::fs::read_dir(plugin_dir).ok().and_then(|rd| {
        rd.filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name())
            .max()
    });
    match version {
        Some(v) => plugin_dir.join(v).join("skills"),
        None => plugin_dir.join("skills"),
    }
}

/// Read + parse `<codex_home>/config.toml` format-preservingly. `None` on a missing/unreadable or
/// syntactically-invalid file (caller degrades gracefully).
fn read_config(codex_home: &Path) -> Option<toml_edit::DocumentMut> {
    let text = std::fs::read_to_string(CodexProvider::config_path(codex_home)).ok()?;
    text.parse::<toml_edit::DocumentMut>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_unsafe_components_are_rejected() {
        assert!(is_safe_path_segment("self"));
        assert!(is_safe_path_segment("1.0.0"));
        assert!(!is_safe_path_segment(".."));
        assert!(!is_safe_path_segment("."));
        assert!(!is_safe_path_segment(""));
        assert!(!is_safe_path_segment("../../.ssh"));
        assert!(!is_safe_path_segment("/etc"));
        assert!(!is_safe_path_segment("a\\b"));
    }

    #[test]
    fn parsed_plugins_drops_traversal_keys_and_treats_missing_enabled_as_active() {
        let dir = std::env::temp_dir().join(format!("si-codex-parse-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.toml"),
            r#"
[plugins."alpha@mkt"]
enabled = true

# no `enabled` key — must be treated as ACTIVE (absent from the enabled map), not disabled.
[plugins."bare@mkt"]

[plugins."off@mkt"]
enabled = false

# a crafted traversal key — must be dropped, never used to build a filesystem path.
[plugins."../../.ssh@mkt"]
enabled = true
"#,
        )
        .unwrap();

        let bares: Vec<String> = CodexProvider::parsed_plugins(&dir)
            .into_iter()
            .map(|p| p.bare)
            .collect();
        assert!(bares.contains(&"alpha".to_string()));
        assert!(bares.contains(&"bare".to_string()));
        assert!(bares.contains(&"off".to_string()));
        assert!(
            !bares.iter().any(|b| b.contains("..") || b.contains('/')),
            "traversal key must be dropped: {bares:?}"
        );

        let enabled = CodexProvider::plugin_enabled_map(&dir);
        assert_eq!(enabled.get("alpha"), Some(&true));
        assert_eq!(enabled.get("off"), Some(&false));
        assert_eq!(
            enabled.get("bare"),
            None,
            "a plugin with no `enabled` key is absent → treated as active"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
