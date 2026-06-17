//! Action dispatcher (contracts/action-api.md) — the ONLY disk writers. Every action mutates
//! only on an explicit request, routes `disable` by the source's strategy (Tier-1 folder vs
//! Tier-2 global), keeps `InspectorState` consistent, and on any failure leaves the setup in
//! its prior state (FR-011..015, FR-024). Responses are JSON the UI can apply without a rescan.

pub mod codex_config;
pub mod overrides;
pub mod permission;
pub mod quarantine;

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::fsutil::copy_dir;
use crate::model::{split_key, SkillState};
use crate::scan::claude::{is_plugin_source, plugin_skill_perm_name, supports_folder_scope};
use crate::scan::Inventory;
use crate::state::InspectorState;
use crate::util_time::now_iso;

/// Everything an action needs that isn't in the request body.
pub struct ActionCtx<'a> {
    pub inventory: &'a Inventory,
    /// The server's configured project root — the only folder a Tier-1 disable may target.
    pub project_root: PathBuf,
    /// User home — locates `~/.claude/settings.json` for the whole-plugin enable/disable switch.
    pub home: PathBuf,
    pub state_path: PathBuf,
    pub quarantine_dir: PathBuf,
    pub trash_dir: PathBuf,
}

/// A skill `id`/`agent` must be a single, normal path component before it is ever joined into
/// a quarantine/trash destination. Rejects `.`/`..`/empty and anything carrying a path
/// separator, so a malformed skill directory name (e.g. `../target`) can never escape the
/// tool-owned destination dir (security: path-escape on disable/remove).
fn is_safe_component(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && !s.contains('/') && !s.contains('\\')
}

/// True when two paths resolve to the same real location. Used to confine a per-repo `folder`
/// write to the server's project root. This is a security boundary, so a canonicalize failure
/// (e.g. the root was deleted mid-session, or `folder` is a dangling symlink) is DENIED rather
/// than falling back to raw string equality — string-equality could be satisfied by a symlink
/// that points at another repo once neither path canonicalizes.
fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}

/// Route an action by name. Unknown actions return a uniform error.
pub fn dispatch(ctx: &ActionCtx, action: &str, req: &Value) -> Value {
    match action {
        "disable" => disable(ctx, req),
        "enable" => enable(ctx, req),
        "plugin" => plugin_set_enabled(ctx, req),
        "remove" => remove(ctx, req),
        "label" => label(ctx, req),
        other => json!({ "ok": false, "error": format!("unknown_action: {other}") }),
    }
}

fn err(error: &str) -> Value {
    json!({ "ok": false, "error": error })
}

fn save(state: &InspectorState, path: &Path) -> Result<(), Value> {
    state
        .save_to(path)
        .map_err(|e| json!({ "ok": false, "error": format!("state_write_failed: {e}") }))
}

fn disable(ctx: &ActionCtx, req: &Value) -> Value {
    let Some(skill_key) = req.get("skill_key").and_then(|v| v.as_str()) else {
        return err("skill_key_required");
    };
    let Some(skill) = ctx.inventory.find(skill_key) else {
        return err("skill_not_found");
    };
    // Guard the identifiers that flow into the quarantine destination path.
    if !is_safe_component(&skill.id) || !is_safe_component(&skill.agent) {
        return err("unsafe_skill_identifier");
    }
    let scope = req
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("folder");
    let now = now_iso();
    let mut state = InspectorState::load_from(&ctx.state_path);

    if scope == "folder" {
        // Per-repo disable: only ever writes the server's own project root's settings.local.json.
        let Some(folder) = req.get("folder").and_then(|v| v.as_str()) else {
            return err("folder_required");
        };
        // Confine the write to the server's project root — never another project's settings.
        if !same_path(Path::new(folder), &ctx.project_root) {
            return err("folder_not_allowed");
        }
        let folder_path = Path::new(folder);

        if supports_folder_scope(&skill.source_id) {
            // claude:user / claude:project → `skillOverrides[<id>]="off"`. Disabling a *global*
            // (user-level) skill here turns it off in THIS repo only; the skill's files and every
            // other repo are untouched.
            match overrides::disable(skill_key, &skill.agent, folder_path, &skill.id, &now) {
                Ok(record) => {
                    state
                        .folder_overrides
                        .retain(|r| !(r.skill_key == skill_key && r.folder == folder));
                    state.folder_overrides.push(record);
                    if let Err(e) = save(&state, &ctx.state_path) {
                        return e;
                    }
                    json!({ "ok": true, "state": "disabled-in-folder", "folder": folder })
                }
                Err(e) => json!({ "ok": false, "error": format!("write_failed: {e}") }),
            }
        } else if is_plugin_source(&skill.source_id) {
            // Plugin skills can't be toggled via skillOverrides → `permissions.deny` rule instead
            // (per-repo, file-only, reversible). State is derived from the file, so no record.
            let Some(name) = plugin_skill_perm_name(&skill.source_id, &skill.id) else {
                return err("bad_plugin_source");
            };
            match permission::disable(folder_path, &name) {
                Ok(_) => json!({
                    "ok": true, "state": "disabled-in-folder", "folder": folder,
                    "mechanism": "permission-deny"
                }),
                Err(e) => json!({ "ok": false, "error": format!("write_failed: {e}") }),
            }
        } else {
            // No per-repo mechanism for this source (e.g. other-agent) — refuse rather than
            // silently fall back to a global change (FR-024).
            json!({ "ok": false, "error": "folder_scope_unsupported", "would_be_scope": "global" })
        }
    } else if skill.agent == crate::scan::codex::CodexProvider::AGENT
        && !is_plugin_source(&skill.source_id)
    {
        // Codex per-skill disable uses its NATIVE switch — `[[skills.config]] enabled=false` in
        // `<codex_home>/config.toml`, selected by the skill's SKILL.md path — NOT a quarantine move.
        // It is global (config.toml is user-level) but non-destructive and fully reversible; the
        // skill's files stay put. This supersedes the quarantine fallback for Codex (research §3 had
        // wrongly concluded Codex has no per-skill toggle).
        let codex_home = crate::scan::codex::CodexProvider::codex_home(&ctx.home);
        let config = codex_home.join("config.toml");
        let skill_md = Path::new(&skill.path).join("SKILL.md");
        match codex_config::set_skill_enabled(&config, &skill_md, false) {
            Ok(_) => json!({ "ok": true, "state": "disabled-global", "effective_scope": "global" }),
            Err(e) => json!({ "ok": false, "error": format!("write_failed: {e}") }),
        }
    } else {
        // Tier-2 global quarantine.
        let original_path = PathBuf::from(&skill.path);
        let original_root = original_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| original_path.clone());
        match quarantine::disable(
            skill_key,
            &skill.agent,
            &skill.id,
            &original_path,
            &original_root,
            &skill.content_hash,
            &ctx.quarantine_dir,
            &now,
        ) {
            Ok(record) => {
                state.quarantine.retain(|r| r.skill_key != skill_key);
                state.quarantine.push(record);
                if let Err(e) = save(&state, &ctx.state_path) {
                    return e;
                }
                json!({ "ok": true, "state": "disabled-global", "effective_scope": "global" })
            }
            // Move failed (e.g. read-only root): tree untouched, no state written.
            Err(e) => json!({ "ok": false, "error": format!("write_failed: {e}") }),
        }
    }
}

fn enable(ctx: &ActionCtx, req: &Value) -> Value {
    let Some(skill_key) = req.get("skill_key").and_then(|v| v.as_str()) else {
        return err("skill_key_required");
    };
    let Some((_, _, id)) = split_key(skill_key) else {
        return err("bad_skill_key");
    };
    let mut state = InspectorState::load_from(&ctx.state_path);

    // Plugin per-repo re-enable: drop the `permissions.deny` rule from the project root's
    // settings.local.json (file-driven, no state record). A plugin skill stays in the inventory
    // while denied, so this lookup succeeds. Idempotent if it wasn't actually denied. Skipped
    // when a (legacy) quarantine record exists for this key — that is restored below instead.
    let has_quarantine = state.quarantine.iter().any(|r| r.skill_key == skill_key);
    if let Some(skill) = ctx.inventory.find(skill_key) {
        if !has_quarantine && is_plugin_source(&skill.source_id) {
            let Some(name) = plugin_skill_perm_name(&skill.source_id, &skill.id) else {
                return err("bad_plugin_source");
            };
            return match permission::enable(&ctx.project_root, &name) {
                Ok(_) => json!({ "ok": true, "state": "active" }),
                Err(e) => json!({ "ok": false, "error": format!("write_failed: {e}") }),
            };
        }
        // Codex non-plugin skill: re-enable by REMOVING its `[[skills.config]] enabled=false` entry
        // from `config.toml` (mirrors the native disable above). Idempotent — a no-op when it wasn't
        // config-disabled. This is THE disable mechanism for Codex now, so with no legacy quarantine
        // record we're done. If a (pre-003) quarantine record ALSO exists, clear the config entry but
        // fall through to restore the moved dir too — otherwise the skill would stay DisabledGlobal.
        if skill.agent == crate::scan::codex::CodexProvider::AGENT
            && !is_plugin_source(&skill.source_id)
        {
            let codex_home = crate::scan::codex::CodexProvider::codex_home(&ctx.home);
            let config = codex_home.join("config.toml");
            let skill_md = Path::new(&skill.path).join("SKILL.md");
            match codex_config::set_skill_enabled(&config, &skill_md, true) {
                Ok(_) if !has_quarantine => return json!({ "ok": true, "state": "active" }),
                Ok(_) => {} // legacy quarantine record present → restore it below
                Err(e) => return json!({ "ok": false, "error": format!("write_failed: {e}") }),
            }
        }
    }

    // Tier-1 first: a folder override (optionally scoped to the requested folder).
    let req_folder = req.get("folder").and_then(|v| v.as_str());
    let folder_idx = state.folder_overrides.iter().position(|r| {
        r.skill_key == skill_key && req_folder.map(|f| f == r.folder).unwrap_or(true)
    });
    if let Some(idx) = folder_idx {
        let record = state.folder_overrides[idx].clone();
        if let Err(e) = overrides::enable(&record, &id) {
            return json!({ "ok": false, "error": format!("write_failed: {e}") });
        }
        state.folder_overrides.remove(idx);
        if let Err(e) = save(&state, &ctx.state_path) {
            return e;
        }
        return json!({ "ok": true, "state": "active" });
    }

    // Tier-2: a quarantine record.
    if let Some(idx) = state
        .quarantine
        .iter()
        .position(|r| r.skill_key == skill_key)
    {
        let record = state.quarantine[idx].clone();
        // Defense in depth: the state file is user-writable; only restore a record whose
        // source actually lives under our quarantine dir (don't follow an injected path).
        if !Path::new(&record.quarantine_path).starts_with(&ctx.quarantine_dir) {
            return err("quarantine_path_outside_tool_dir");
        }
        return match quarantine::restore(&record) {
            Ok(outcome) => {
                state.quarantine.remove(idx);
                if let Err(e) = save(&state, &ctx.state_path) {
                    return e;
                }
                let mut resp = json!({ "ok": true, "state": "active" });
                if outcome.drifted {
                    resp["warning"] = json!("content drift detected on restore");
                }
                resp
            }
            Err(e) => json!({ "ok": false, "error": format!("restore_failed: {e}") }),
        };
    }

    // Fallback: a file-side `skillOverrides[<id>]="off"` with no matching state record — the entry
    // pre-dates the tool, the state file was cleared, or it was hand-edited. The skill still shows
    // as disabled, so enable must clear the file override directly rather than report
    // `not_disabled` (which broke "turn all on"). Gated like every other write path: a SCANNED
    // skill (not an arbitrary request string), one that uses `skillOverrides` at all (user/project —
    // never a plugin, which is `permissions.deny`), with a safe id, confined to our project root.
    if let Some(skill) = ctx.inventory.find(skill_key) {
        if supports_folder_scope(&skill.source_id)
            && is_safe_component(&skill.id)
            && crate::settings::read_override(&ctx.project_root, &skill.id).is_some()
        {
            return match crate::settings::clear_override(&ctx.project_root, &skill.id, None) {
                Ok(_) => json!({ "ok": true, "state": "active" }),
                Err(e) => json!({ "ok": false, "error": format!("write_failed: {e}") }),
            };
        }
    }
    err("not_disabled")
}

/// Whole-plugin enable/disable: flip `enabledPlugins["<name>@<mkt>"]` in `~/.claude/settings.json`.
/// Request: `{ "plugin": "<bare-name>", "enabled": <bool> }`. Unlike the per-repo skill toggles,
/// this is a GLOBAL switch (every project) — it affects all of the plugin's skills at once. The
/// change applies to a running session only after `/reload-plugins` (surfaced in the response).
fn plugin_set_enabled(ctx: &ActionCtx, req: &Value) -> Value {
    let Some(plugin) = req.get("plugin").and_then(|v| v.as_str()) else {
        return err("plugin_required");
    };
    let Some(enabled) = req.get("enabled").and_then(|v| v.as_bool()) else {
        return err("enabled_required");
    };
    // Guard the request-supplied name to the same allowlist plugin/skill segments must satisfy,
    // rejecting junk before it ever reaches the manifest lookup (consistent with the write path).
    if !crate::scan::claude::is_valid_perm_segment(plugin) {
        return err("invalid_plugin_name");
    }
    // Route by agent. A Codex plugin toggle writes `<codex_home>/config.toml` (format-preservingly),
    // not the Claude `settings.json`; the Claude branch below is unchanged. Default: claude-code.
    let agent = req
        .get("agent")
        .and_then(|v| v.as_str())
        .unwrap_or(crate::scan::claude::ClaudeProvider::AGENT);
    if agent == crate::scan::codex::CodexProvider::AGENT {
        let codex_home = crate::scan::codex::CodexProvider::codex_home(&ctx.home);
        let Some(full_key) =
            crate::scan::codex::CodexProvider::resolve_plugin_full_key(&codex_home, plugin)
        else {
            return err("plugin_not_found");
        };
        let config = codex_home.join("config.toml");
        return match codex_config::set_plugin_enabled(&config, &full_key, enabled) {
            Ok(previous) => json!({
                "ok": true,
                "plugin": full_key,
                "enabled": enabled,
                "previous": previous,
                "agent": "codex",
                "scope": "global",
                "note": "takes effect after restarting Codex or refreshing /skills"
            }),
            Err(e) => json!({ "ok": false, "error": format!("write_failed: {e}") }),
        };
    }
    // Resolve the bare name to the exact `"<name>@<marketplace>"` Claude keys on. The written key
    // comes from the trusted manifest (not the request), so there is no injection surface here.
    let Some(full_key) = crate::scan::claude::resolve_plugin_full_key(&ctx.home, plugin) else {
        return err("plugin_not_found");
    };
    let settings_json = ctx.home.join(".claude").join("settings.json");
    match crate::settings::set_plugin_enabled(&settings_json, &full_key, enabled) {
        Ok(previous) => json!({
            "ok": true,
            "plugin": full_key,
            "enabled": enabled,
            "previous": previous,
            "note": "takes effect after /reload-plugins or restarting Claude Code"
        }),
        Err(e) => json!({ "ok": false, "error": format!("write_failed: {e}") }),
    }
}

fn remove(ctx: &ActionCtx, req: &Value) -> Value {
    let Some(skill_key) = req.get("skill_key").and_then(|v| v.as_str()) else {
        return err("skill_key_required");
    };
    if req.get("confirm").and_then(|v| v.as_bool()) != Some(true) {
        return err("confirmation_required");
    }
    let Some(skill) = ctx.inventory.find(skill_key) else {
        return err("skill_not_found");
    };
    // A built-in (e.g. a Codex `.system` skill) must never be destructively removed (FR-008). A
    // reversible quarantine `disable` of it is still allowed (handled in `disable`).
    if skill.built_in {
        return err("built_in_remove_refused");
    }
    let (_, _, id) = match split_key(skill_key) {
        Some(p) => p,
        None => return err("bad_skill_key"),
    };
    if !is_safe_component(&id) || !is_safe_component(&skill.agent) {
        return err("unsafe_skill_identifier");
    }
    let src = PathBuf::from(&skill.path);
    let backup = ctx.trash_dir.join(&skill.agent).join(&id);

    // Back up first; only delete if the backup succeeded. A failure to clear a stale backup is
    // surfaced (a corrupt/partial backup must not be silently trusted as recovery).
    if backup.exists() {
        if let Err(e) = std::fs::remove_dir_all(&backup) {
            return json!({ "ok": false, "error": format!("stale_backup_unremovable: {e}") });
        }
    }
    if let Err(e) = copy_dir(&src, &backup) {
        return json!({ "ok": false, "error": format!("backup_failed: {e}") });
    }
    if let Err(e) = std::fs::remove_dir_all(&src) {
        // Read-only root: original remains, leave the (harmless) backup. Tree unchanged.
        return json!({ "ok": false, "error": format!("delete_failed: {e}") });
    }

    // Clear any stale state referring to the removed skill.
    let mut state = InspectorState::load_from(&ctx.state_path);
    state.quarantine.retain(|r| r.skill_key != skill_key);
    state.folder_overrides.retain(|r| r.skill_key != skill_key);
    state.labels.remove(skill_key);
    if let Err(e) = save(&state, &ctx.state_path) {
        return e;
    }
    json!({ "ok": true, "state": "removed", "backup": backup.to_string_lossy() })
}

fn label(ctx: &ActionCtx, req: &Value) -> Value {
    let Some(skill_key) = req.get("skill_key").and_then(|v| v.as_str()) else {
        return err("skill_key_required");
    };
    let labels: Vec<String> = req
        .get("labels")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let mut state = InspectorState::load_from(&ctx.state_path);
    if labels.is_empty() {
        state.labels.remove(skill_key);
    } else {
        state.labels.insert(skill_key.to_string(), labels.clone());
    }
    if let Err(e) = save(&state, &ctx.state_path) {
        return e;
    }
    json!({ "ok": true, "labels": labels })
}

/// Skill state name as it appears in the inventory export (for UI convenience).
pub fn state_str(state: SkillState) -> &'static str {
    match state {
        SkillState::Active => "active",
        SkillState::DisabledInFolder => "disabled-in-folder",
        SkillState::DisabledGlobal => "disabled-global",
        SkillState::DisabledPlugin => "disabled-plugin",
    }
}
