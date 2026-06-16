//! Read/write the Claude `skillOverrides` map in a folder's `.claude/settings.local.json`
//! (research.md §4). Used by `scan` to derive `disabled-in-folder` state and by `action`
//! to perform the Tier-1 folder-scoped disable. Other keys in the file are always preserved.
//!
//! The override map is keyed by the skill `id` (its directory name) — the stable invocation
//! identifier the tool uses end-to-end; `scan` reads the same key `action` writes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub fn settings_path(folder: &Path) -> PathBuf {
    folder.join(".claude").join("settings.local.json")
}

/// Lenient read of an arbitrary settings file (any of the `.json` scopes), returning `Null` when
/// absent or malformed. The single lenient-parse implementation — `load_root` delegates here so
/// the two can't drift (e.g. if a size guard is ever added).
fn load_file(path: &Path) -> serde_json::Value {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or(serde_json::Value::Null),
        Err(_) => serde_json::Value::Null,
    }
}

fn load_root(folder: &Path) -> serde_json::Value {
    load_file(&settings_path(folder))
}

/// Names (the `<name>` before `@<marketplace>`) of plugins switched OFF via `enabledPlugins`.
///
/// Bare `<name>`s of plugins that are ENABLED — i.e. `enabledPlugins["<name>@<mkt>"] == true`.
///
/// Claude Code loads a plugin ONLY when it is explicitly enabled: a plugin that is `false` OR
/// simply absent from `enabledPlugins` is not loaded, and none of its skills trigger in any
/// project (verified on-machine: `enabledPlugins` had 6 `true` entries and `/reload-plugins`
/// loaded exactly 6 plugins; an installed-but-absent plugin was uninvocable). So the scanner
/// treats "not in this set" — false *or* absent — as `disabled-plugin`; only membership here
/// means active.
///
/// `enabledPlugins` is keyed by the full `"<name>@<marketplace>"` and merges across settings
/// scopes; effective value wins highest-precedence-last: user (`~/.claude/settings.json`) <
/// project (`<root>/.claude/settings.json`) < project-local (`<root>/.claude/settings.local.json`).
/// Keys are reduced to the bare `<name>` to match the `claude:plugin:<name>` source id (which
/// `discover_plugins` derives the same way, and which collapses a name to one plugin — first
/// install wins). The reduction happens BEFORE merging, so last-scope-wins precedence operates on
/// the bare name the caller matches against: a project-scope `name@x=true` correctly overrides a
/// user-scope `name@y=false`.
pub fn read_enabled_plugin_names(home: &Path, project_root: &Path) -> BTreeSet<String> {
    let mut effective: BTreeMap<String, bool> = BTreeMap::new();
    let scopes = [
        home.join(".claude").join("settings.json"),
        project_root.join(".claude").join("settings.json"),
        project_root.join(".claude").join("settings.local.json"),
    ];
    for path in scopes {
        let root = load_file(&path);
        if let Some(map) = root.get("enabledPlugins").and_then(|v| v.as_object()) {
            for (k, v) in map {
                if let Some(b) = v.as_bool() {
                    let name = k.split('@').next().unwrap_or(k).to_string();
                    effective.insert(name, b);
                }
            }
        }
    }
    effective
        .into_iter()
        .filter(|(_, enabled)| *enabled)
        .map(|(name, _)| name)
        .collect()
}

/// Loader for the WRITE paths. Unlike `load_root`, it distinguishes an absent file (`Ok(Null)`
/// → start a fresh object) from a present-but-corrupt one (`Err`). Without this, a syntactically
/// broken settings.local.json (partial write, bad manual edit, merge markers) would parse as
/// `Null` and the next write would silently overwrite it — dropping every key the user had. The
/// pure-read callers keep using the lenient `load_root` (a corrupt file simply reads as empty).
fn load_root_for_write(folder: &Path) -> std::io::Result<serde_json::Value> {
    match std::fs::read_to_string(settings_path(folder)) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("refusing to overwrite malformed settings.local.json: {e}"),
            )
        }),
        Err(_) => Ok(serde_json::Value::Null), // absent → callers create it
    }
}

/// Current `skillOverrides` map (empty when the file/key is absent or malformed).
pub fn read_skill_overrides(folder: &Path) -> BTreeMap<String, String> {
    let root = load_root(folder);
    let mut out = BTreeMap::new();
    if let Some(map) = root.get("skillOverrides").and_then(|v| v.as_object()) {
        for (k, v) in map {
            if let Some(s) = v.as_str() {
                out.insert(k.clone(), s.to_string());
            }
        }
    }
    out
}

pub fn read_override(folder: &Path, id: &str) -> Option<String> {
    read_skill_overrides(folder).get(id).cloned()
}

/// Set `skillOverrides[<id>] = value`, creating the file/key if absent and preserving every
/// other key. Returns the previous value (for undo). Writes pretty JSON + trailing newline.
pub fn set_override(folder: &Path, id: &str, value: &str) -> std::io::Result<Option<String>> {
    let mut root = load_root_for_write(folder)?;
    if !root.is_object() {
        root = serde_json::Value::Object(serde_json::Map::new());
    }
    let obj = root.as_object_mut().expect("ensured object");
    let overrides = obj
        .entry("skillOverrides")
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if !overrides.is_object() {
        *overrides = serde_json::Value::Object(serde_json::Map::new());
    }
    let map = overrides.as_object_mut().expect("ensured object");
    let previous = map
        .insert(id.to_string(), serde_json::Value::String(value.to_string()))
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    write_root(folder, &root)?;
    Ok(previous)
}

/// Restore `skillOverrides[<id>]` to `previous` (or remove the key when `previous` is None),
/// preserving every other key. Leaves an empty `skillOverrides` object in place if it empties
/// out (harmless, and avoids guessing whether the user wants the key gone).
pub fn clear_override(folder: &Path, id: &str, previous: Option<&str>) -> std::io::Result<()> {
    let mut root = load_root_for_write(folder)?;
    if !root.is_object() {
        return Ok(()); // nothing to clear
    }
    let Some(overrides) = root
        .get_mut("skillOverrides")
        .and_then(|v| v.as_object_mut())
    else {
        return Ok(()); // no override map → nothing to clear, no needless write
    };
    match previous {
        Some(prev) => {
            overrides.insert(id.to_string(), serde_json::Value::String(prev.to_string()));
        }
        None => {
            if overrides.remove(id).is_none() {
                return Ok(()); // key wasn't present → no change, skip the write
            }
        }
    }
    write_root(folder, &root)
}

// --- Plugin per-repo disable via `permissions.deny` -------------------------------------------
//
// Plugin skills are NOT affected by `skillOverrides` (docs/en/skills §"Override skill visibility
// from settings"), so the per-repo toggle for a plugin skill is a permission rule instead:
// `permissions.deny += "Skill(<plugin>:<id>)"`. `Skill(name)` is exact-match permission syntax
// (docs/en/skills §"Control skill access"). These helpers read/add/remove that rule in the same
// `settings.local.json`, always preserving every other key — `scan` reads, `action` writes.

/// Parse `<name>` out of an exact `Skill(<name>)` deny rule. Prefix-match forms (`Skill(x *)`)
/// are intentionally ignored — the inspector only ever writes exact rules, and a user-authored
/// wildcard rule is theirs to manage.
fn parse_exact_skill_rule(rule: &str) -> Option<&str> {
    let inner = rule.strip_prefix("Skill(")?.strip_suffix(')')?;
    // Reject anything that couldn't have come from `is_valid_perm_segment`: whitespace, the
    // prefix-match `*`, and stray parens (defense-in-depth against a malformed/injected rule).
    if inner.is_empty() || inner.contains([' ', '*', '(', ')']) {
        return None;
    }
    Some(inner)
}

/// The set of skill permission-names denied via exact `Skill(<name>)` rules in this folder's
/// `permissions.deny` (empty when the file/key is absent or malformed).
pub fn read_skill_denies(folder: &Path) -> BTreeSet<String> {
    let root = load_root(folder);
    let mut out = BTreeSet::new();
    let denies = root
        .get("permissions")
        .and_then(|p| p.get("deny"))
        .and_then(|d| d.as_array());
    if let Some(arr) = denies {
        for v in arr {
            if let Some(name) = v.as_str().and_then(parse_exact_skill_rule) {
                out.insert(name.to_string());
            }
        }
    }
    out
}

/// True when an exact `Skill(<name>)` deny rule is present in this folder.
pub fn skill_denied(folder: &Path, name: &str) -> bool {
    read_skill_denies(folder).contains(name)
}

/// Add `Skill(<name>)` to `permissions.deny`, creating the nested objects/array if absent and
/// preserving every other key. Idempotent — returns false (no write) when already present.
pub fn add_skill_deny(folder: &Path, name: &str) -> std::io::Result<bool> {
    let rule = format!("Skill({name})");
    let mut root = load_root_for_write(folder)?;
    if !root.is_object() {
        root = serde_json::Value::Object(serde_json::Map::new());
    }
    let obj = root.as_object_mut().expect("ensured object");
    let perms = obj
        .entry("permissions")
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if !perms.is_object() {
        *perms = serde_json::Value::Object(serde_json::Map::new());
    }
    let pobj = perms.as_object_mut().expect("ensured object");
    let deny = pobj
        .entry("deny")
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    if !deny.is_array() {
        *deny = serde_json::Value::Array(Vec::new());
    }
    let arr = deny.as_array_mut().expect("ensured array");
    if arr.iter().any(|v| v.as_str() == Some(rule.as_str())) {
        return Ok(false); // already denied → no needless write
    }
    arr.push(serde_json::Value::String(rule));
    write_root(folder, &root)?;
    Ok(true)
}

/// Remove every exact `Skill(<name>)` rule from `permissions.deny`, preserving all other keys
/// (and any other deny rules). Returns false (no write) when nothing matched.
pub fn remove_skill_deny(folder: &Path, name: &str) -> std::io::Result<bool> {
    let rule = format!("Skill({name})");
    let mut root = load_root_for_write(folder)?;
    if !root.is_object() {
        return Ok(false);
    }
    let Some(arr) = root
        .get_mut("permissions")
        .and_then(|p| p.get_mut("deny"))
        .and_then(|d| d.as_array_mut())
    else {
        return Ok(false);
    };
    let before = arr.len();
    arr.retain(|v| v.as_str() != Some(rule.as_str()));
    if arr.len() == before {
        return Ok(false); // wasn't present → no change, skip the write
    }
    write_root(folder, &root).map(|()| true)
}

// --- Whole-plugin enable/disable via `enabledPlugins` -----------------------------------------
//
// Unlike the per-repo skill toggles above (which write the project's settings.local.json), turning
// a whole plugin on/off is a GLOBAL switch Claude keys on `enabledPlugins["<name>@<mkt>"]`. The UI
// writes it to the user's `~/.claude/settings.json`. This is a deliberate machine-wide change, not
// a per-repo one — the only writer in the tool that touches a file outside the project root.

/// Set `enabledPlugins["<full_key>"] = enabled` in the given settings.json, creating the file/key
/// if absent and preserving every other key. `full_key` is the `"<name>@<marketplace>"` Claude
/// keys on. The path may be a symlink (dotfiles commonly symlink `~/.claude/settings.json`); the
/// write follows it to the real target so we update the user's actual file rather than replacing
/// the link. Refuses to clobber a present-but-malformed file. Returns the previous value (for undo).
pub fn set_plugin_enabled(
    settings_json: &Path,
    full_key: &str,
    enabled: bool,
) -> std::io::Result<Option<bool>> {
    // Resolve the write target. An existing file (incl. a symlink — dotfiles commonly symlink this
    // out to a repo) is canonicalized so we update the real file the link points at. If it does
    // NOT exist, only a genuinely-absent regular path may be created fresh; a DANGLING symlink is
    // refused rather than followed to an arbitrary destination (security: don't write through a
    // planted link). Any other canonicalize error propagates.
    let target = match std::fs::canonicalize(settings_json) {
        Ok(real) => real,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if settings_json.is_symlink() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "refusing to write through a dangling settings.json symlink",
                ));
            }
            settings_json.to_path_buf()
        }
        Err(e) => return Err(e),
    };

    // Read current contents. Distinguish absent (→ start fresh) from a real read error such as
    // PermissionDenied (→ propagate): collapsing the latter to an empty object would overwrite the
    // user's whole settings file with a one-key stub. A present-but-malformed file is also refused.
    let mut root = match std::fs::read_to_string(&target) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("refusing to overwrite malformed settings.json: {e}"),
            )
        })?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::Value::Null,
        Err(e) => return Err(e),
    };
    if !root.is_object() {
        root = serde_json::Value::Object(serde_json::Map::new());
    }
    let obj = root.as_object_mut().expect("ensured object");
    let map = obj
        .entry("enabledPlugins")
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if !map.is_object() {
        *map = serde_json::Value::Object(serde_json::Map::new());
    }
    let map = map.as_object_mut().expect("ensured object");
    // No-op when already at the requested value — skip the rewrite (and the mtime churn a running
    // Claude session's file-watch would see), matching `add_skill_deny`'s idempotent behaviour.
    let previous = map.get(full_key).and_then(|v| v.as_bool());
    if previous == Some(enabled) {
        return Ok(previous);
    }
    map.insert(full_key.to_string(), serde_json::Value::Bool(enabled));

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = serde_json::to_string_pretty(&root).map_err(std::io::Error::other)?;
    text.push('\n');
    // Atomic: write a sibling temp file then rename over the target, so a crash mid-write can never
    // leave the shared ~/.claude/settings.json truncated/corrupt.
    write_atomic(&target, text.as_bytes())?;
    Ok(previous)
}

/// Write `bytes` to `path` by writing a sibling `*.cite.tmp` then `rename`-ing over the target.
/// Rename is atomic within a filesystem (the temp is a sibling, so same fs as the target).
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let Some(name) = path.file_name() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "settings path has no file name",
        ));
    };
    let mut tmp_name = name.to_os_string();
    tmp_name.push(".cite.tmp");
    let tmp = path.with_file_name(tmp_name);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

fn write_root(folder: &Path, root: &serde_json::Value) -> std::io::Result<()> {
    let path = settings_path(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = serde_json::to_string_pretty(root).map_err(std::io::Error::other)?;
    text.push('\n');
    // Atomic temp+rename (matching `set_plugin_enabled`): a crash mid-write must never truncate
    // settings.local.json to empty and drop the user's other keys (permissions, other overrides).
    write_atomic(&path, text.as_bytes())
}
