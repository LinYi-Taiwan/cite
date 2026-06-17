//! Codex plugin on/off writer (003-codex-skill-support, contracts/codex-action-api.md Action 1).
//!
//! Flips ONLY the `enabled` value in `[plugins."<name>@<marketplace>"]` of `<codex_home>/config.toml`,
//! format-preservingly (research §4): the user's comments, key ordering, and every other table stay
//! byte-identical. In-memory `toml_edit` edit → a single atomic temp+rename write. Idempotent no-op
//! when already in the desired state; a clear error (writing nothing) on a missing/unwritable config
//! or unknown plugin key (FR-012, FR-016, FR-017).

use std::io;
use std::path::Path;

/// Set `[plugins."<full_key>"].enabled = <enabled>` in `config.toml`. `full_key` is the exact
/// `"<name>@<marketplace>"` config key (recovered by the caller via
/// `CodexProvider::resolve_plugin_full_key`). Returns the previous `enabled` value (for undo). The
/// config file must already exist — Codex owns it; the inspector never creates it.
pub fn set_plugin_enabled(
    config_path: &Path,
    full_key: &str,
    enabled: bool,
) -> io::Result<Option<bool>> {
    // Resolve the real target first. `config.toml` is commonly symlinked out to a dotfiles repo;
    // canonicalizing means the temp+rename below operates in the REAL file's directory and updates
    // the linked file in place (preserving the symlink) rather than replacing the link with a plain
    // file. A dangling or missing symlink fails here (NotFound) → we error and write nothing, never
    // following a planted dangling link to an arbitrary destination (parity with
    // `settings.rs::set_plugin_enabled`). The inspector never creates a Codex config from scratch.
    let target = std::fs::canonicalize(config_path)?;
    let text = std::fs::read_to_string(&target)?;
    let mut doc = text.parse::<toml_edit::DocumentMut>().map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("refusing to overwrite malformed config.toml: {e}"),
        )
    })?;

    // Navigate to the plugin's table. A missing `[plugins]` table or unknown plugin key is an
    // error (the caller resolved the key from this same file, so this is a real "not found").
    let plugin = doc
        .get_mut("plugins")
        .and_then(|p| p.as_table_like_mut())
        .and_then(|t| t.get_mut(full_key))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "plugin key not found"))?;
    let plugin = plugin
        .as_table_like_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "plugin entry is not a table"))?;

    // No-op when already at the requested value — skip the rewrite (and the mtime churn a watching
    // Codex session would see), mirroring the Claude settings writer.
    let previous = plugin.get("enabled").and_then(|v| v.as_bool());
    if previous == Some(enabled) {
        return Ok(previous);
    }
    plugin.insert("enabled", toml_edit::value(enabled));

    // Atomic: write a sibling temp then rename over the target, so a crash mid-write can never
    // truncate the user's hand-edited config.toml.
    write_atomic(&target, doc.to_string().as_bytes())?;
    Ok(previous)
}

/// Disable (`enabled = false`) or re-enable a single skill by editing the `[[skills.config]]` array
/// in `config.toml`, selected by the skill's `SKILL.md` path. This is Codex's NATIVE per-skill
/// switch — verified against `codex debug prompt-input`: an `enabled = false` entry drops the skill
/// from the loaded set with NO file movement, so it is the reversible, non-destructive analogue of
/// Claude's per-skill toggle (and supersedes the quarantine-move fallback for Codex skills).
///
/// - `enabled = false` → ensure a `[[skills.config]]` table `{ path = <skill_md>, enabled = false }`
///   exists (insert one, or flip an existing same-path entry).
/// - `enabled = true`  → REMOVE the matching disable entry (and prune an emptied `skills.config` /
///   `skills` table) so the file returns to its prior shape, never leaving inert `enabled = true`
///   cruft.
///
/// Format-preserving (`toml_edit`), atomic temp+rename. The config file must already exist — Codex
/// owns it; the inspector never creates it. Returns the skill's previous `enabled` value (`Some(false)`
/// if it was already disabled, `None` if no entry existed) for undo/idempotence.
pub fn set_skill_enabled(
    config_path: &Path,
    skill_md_path: &Path,
    enabled: bool,
) -> io::Result<Option<bool>> {
    // Resolve the real target first (config.toml is commonly symlinked to a dotfiles repo) so the
    // temp+rename updates the linked file in place — same rationale as `set_plugin_enabled`.
    let target = std::fs::canonicalize(config_path)?;
    let text = std::fs::read_to_string(&target)?;
    let mut doc = text.parse::<toml_edit::DocumentMut>().map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("refusing to overwrite malformed config.toml: {e}"),
        )
    })?;
    // The selector is stored verbatim as the absolute `SKILL.md` path the scanner also builds, so a
    // round-trip (write here → read in the scanner) matches by exact string — no canonicalization.
    let selector = skill_md_path.to_string_lossy();

    // Locate an existing `skills.config` entry with this path selector, if any.
    let existing = doc
        .get("skills")
        .and_then(|s| s.as_table_like())
        .and_then(|s| s.get("config"))
        .and_then(|c| c.as_array_of_tables())
        .map(|arr| {
            arr.iter().enumerate().find_map(|(i, t)| {
                (t.get("path").and_then(|v| v.as_str()) == Some(selector.as_ref()))
                    .then(|| (i, t.get("enabled").and_then(|v| v.as_bool())))
            })
        })
        .flatten();
    let previous = existing.and_then(|(_, prev)| prev);

    if enabled {
        // Re-enable = remove the disable entry (if present); prune emptied containers.
        let Some((idx, _)) = existing else {
            return Ok(previous); // nothing to remove → idempotent no-op, write nothing
        };
        if let Some(arr) = doc
            .get_mut("skills")
            .and_then(|s| s.as_table_like_mut())
            .and_then(|s| s.get_mut("config"))
            .and_then(|c| c.as_array_of_tables_mut())
        {
            arr.remove(idx);
            if arr.is_empty() {
                if let Some(skills) = doc.get_mut("skills").and_then(|s| s.as_table_like_mut()) {
                    skills.remove("config");
                    if skills.is_empty() {
                        doc.as_table_mut().remove("skills");
                    }
                }
            }
        }
        write_atomic(&target, doc.to_string().as_bytes())?;
        return Ok(previous);
    }

    // Disable: flip an existing entry, or append a new `[[skills.config]]` table.
    if previous == Some(false) {
        return Ok(previous); // already disabled → no-op
    }
    if let Some((idx, _)) = existing {
        // Resolve the entry explicitly: if the node was somehow demoted between the read above and
        // here, error rather than silently write a doc that still carries the old `enabled` value.
        let Some(entry) = doc
            .get_mut("skills")
            .and_then(|s| s.as_table_like_mut())
            .and_then(|s| s.get_mut("config"))
            .and_then(|c| c.as_array_of_tables_mut())
            .and_then(|arr| arr.get_mut(idx))
        else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "skills.config entry vanished between read and write",
            ));
        };
        entry.insert("enabled", toml_edit::value(false));
    } else {
        // Ensure `skills` is a table holding an array-of-tables `config`, then push our entry. The
        // `skills` table is marked implicit so a bare `[skills]` header isn't emitted when it only
        // carries `[[skills.config]]`.
        let skills = doc.as_table_mut().entry("skills").or_insert_with(|| {
            let mut t = toml_edit::Table::new();
            t.set_implicit(true);
            toml_edit::Item::Table(t)
        });
        let Some(skills_tbl) = skills.as_table_mut() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "`skills` is not a table",
            ));
        };
        let config = skills_tbl
            .entry("config")
            .or_insert_with(|| toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()));
        let Some(arr) = config.as_array_of_tables_mut() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "`skills.config` is not an array of tables",
            ));
        };
        let mut entry = toml_edit::Table::new();
        entry.insert("path", toml_edit::value(selector.as_ref()));
        entry.insert("enabled", toml_edit::value(false));
        arr.push(entry);
    }
    write_atomic(&target, doc.to_string().as_bytes())?;
    Ok(previous)
}

/// Write `bytes` to `path` via a sibling temp file + `rename` (atomic within a filesystem). On a
/// rename failure (e.g. a cross-device target) the temp file is removed so no temp litter is left
/// beside the user's config. The temp name carries the PID + a per-call sequence so two concurrent
/// writers (e.g. two `serve` requests, or another process) never share — and thus never clobber —
/// the same temp file; the final `rename` onto `path` is still last-writer-wins.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

    let Some(name) = path.file_name() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "config path has no file name",
        ));
    };
    let mut tmp_name = name.to_os_string();
    tmp_name.push(format!(
        ".cite.{}.{}.tmp",
        std::process::id(),
        TMP_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let tmp = path.with_file_name(tmp_name);
    std::fs::write(&tmp, bytes)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}
