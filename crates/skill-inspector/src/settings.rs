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

fn load_root(folder: &Path) -> serde_json::Value {
    match std::fs::read_to_string(settings_path(folder)) {
        Ok(text) => serde_json::from_str(&text).unwrap_or(serde_json::Value::Null),
        Err(_) => serde_json::Value::Null,
    }
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
    write_root(folder, &root)
        .map(|()| true)
}

fn write_root(folder: &Path, root: &serde_json::Value) -> std::io::Result<()> {
    let path = settings_path(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = serde_json::to_string_pretty(root).map_err(std::io::Error::other)?;
    text.push('\n');
    std::fs::write(path, text)
}
