//! US3 (T022): the action layer is the only writer and is reversible/safe.
//!
//! - Tier-1 folder disable → enable round-trips losslessly, preserving other settings keys.
//! - Tier-2 quarantine disable → enable restores the dir to its exact original path, byte-identical.
//! - `folder` scope on a source with no per-folder toggle is refused (not silently global).
//! - Unconfirmed remove is a no-op; confirmed remove backs up then deletes.
//! - A write to a read-only root returns `ok:false` and leaves the tree untouched.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use serde_json::json;

use skill_inspector::action::{dispatch, ActionCtx};
use skill_inspector::fsutil::copy_dir;
use skill_inspector::state::InspectorState;
use skill_inspector::{build_inventory, scan_context};

static COUNTER: AtomicUsize = AtomicUsize::new(0);
/// `CODEX_HOME` is process-global; serialize the Codex test's set/remove against it.
static CODEX_ENV_LOCK: Mutex<()> = Mutex::new(());

struct Env {
    home: PathBuf,
    project: PathBuf,
    state_path: PathBuf,
    quarantine_dir: PathBuf,
    trash_dir: PathBuf,
    root: PathBuf,
}

impl Drop for Env {
    fn drop(&mut self) {
        // Best-effort cleanup; restore perms first in case a test left a read-only dir.
        let _ = restore_perms(&self.root);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn setup() -> Env {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/multi-source");
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("si-action-{}-{}", std::process::id(), id));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    copy_dir(&src, &root.join("ms")).unwrap();
    let base = root.join("ms");
    Env {
        home: base.join("home"),
        project: base.join("project"),
        state_path: root.join("inspector-state.json"),
        quarantine_dir: root.join("quarantine"),
        trash_dir: root.join("trash"),
        root,
    }
}

impl Env {
    fn ctx(&self) -> skill_inspector::scan::source::ScanContext {
        scan_context(self.project.clone(), self.home.clone())
    }
    fn inventory(&self) -> skill_inspector::scan::Inventory {
        let state = InspectorState::load_from(&self.state_path);
        build_inventory(
            &["claude-code".to_string(), "other-agent".to_string()],
            &self.ctx(),
            &state,
        )
    }
    fn act(&self, action: &str, req: serde_json::Value) -> serde_json::Value {
        let inv = self.inventory();
        let ctx = ActionCtx {
            inventory: &inv,
            project_root: self.project.clone(),
            home: self.home.clone(),
            state_path: self.state_path.clone(),
            quarantine_dir: self.quarantine_dir.clone(),
            trash_dir: self.trash_dir.clone(),
        };
        dispatch(&ctx, action, &req)
    }
    fn skill_state(&self, id: &str) -> Option<skill_inspector::model::SkillState> {
        self.inventory()
            .skills
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.state)
    }
}

#[test]
fn tier1_folder_disable_enable_roundtrips_and_preserves_other_keys() {
    use skill_inspector::model::SkillState;
    let env = setup();
    let key = "claude-code/claude:user/react-code-review";
    let folder = env.project.to_string_lossy().to_string();

    // Pre-existing settings key must be preserved across the override write.
    let settings = env.project.join(".claude/settings.local.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    std::fs::write(&settings, r#"{"permissions":{"allow":["X"]}}"#).unwrap();

    let resp = env.act(
        "disable",
        json!({ "skill_key": key, "scope": "folder", "folder": folder }),
    );
    assert_eq!(resp["ok"], json!(true), "{resp}");
    assert_eq!(resp["state"], json!("disabled-in-folder"));

    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    assert_eq!(
        after["permissions"]["allow"][0],
        json!("X"),
        "other keys preserved"
    );
    assert_eq!(after["skillOverrides"]["react-code-review"], json!("off"));
    assert!(matches!(
        env.skill_state("react-code-review"),
        Some(SkillState::DisabledInFolder)
    ));

    // Enable restores: override key cleared, other keys intact, skill active again.
    let resp = env.act("enable", json!({ "skill_key": key, "folder": folder }));
    assert_eq!(resp["ok"], json!(true), "{resp}");
    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    assert_eq!(after["permissions"]["allow"][0], json!("X"));
    assert!(after["skillOverrides"].get("react-code-review").is_none());
    assert!(matches!(
        env.skill_state("react-code-review"),
        Some(SkillState::Active)
    ));
}

#[test]
fn enable_clears_a_file_side_override_with_no_state_record() {
    // Regression: the UI shows a user/project skill as off because scan read
    // `skillOverrides[<id>]="off"` from the file, but no folder_override record exists (the entry
    // pre-dates the tool, the state file was cleared, or it was hand-edited). Enable must clear the
    // file override and report success — NOT fail `not_disabled` (which broke "turn all on").
    use skill_inspector::model::SkillState;
    let env = setup();
    let key = "claude-code/claude:user/react-code-review";
    let folder = env.project.to_string_lossy().to_string();

    // Seed an off-override directly in the file; leave inspector-state.json absent (no record).
    let settings = env.project.join(".claude/settings.local.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    std::fs::write(
        &settings,
        r#"{"permissions":{"allow":["X"]},"skillOverrides":{"react-code-review":"off"}}"#,
    )
    .unwrap();
    assert!(matches!(
        env.skill_state("react-code-review"),
        Some(SkillState::DisabledInFolder)
    ));
    assert!(InspectorState::load_from(&env.state_path)
        .folder_overrides
        .is_empty());

    let resp = env.act("enable", json!({ "skill_key": key, "folder": folder }));
    assert_eq!(resp["ok"], json!(true), "{resp}");
    assert_eq!(resp["state"], json!("active"));

    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    assert_eq!(
        after["permissions"]["allow"][0],
        json!("X"),
        "other keys kept"
    );
    assert!(
        after["skillOverrides"].get("react-code-review").is_none(),
        "override cleared: {after}"
    );
    assert!(matches!(
        env.skill_state("react-code-review"),
        Some(SkillState::Active)
    ));
}

#[test]
fn folder_disable_rejects_a_foreign_folder() {
    // A crafted `folder` pointing outside the server's project root must be refused — never
    // silently write skillOverrides into another project's settings.
    let env = setup();
    let key = "claude-code/claude:user/react-code-review";
    let foreign = std::env::temp_dir();
    let resp = env.act(
        "disable",
        json!({ "skill_key": key, "scope": "folder", "folder": foreign.to_string_lossy() }),
    );
    assert_eq!(resp["ok"], json!(false));
    assert_eq!(resp["error"], json!("folder_not_allowed"));
}

#[test]
fn disable_refuses_to_clobber_a_corrupt_settings_file() {
    // A present-but-corrupt settings.local.json must NOT be silently overwritten (which would
    // drop every key the user had). The write fails and the file is left byte-for-byte intact.
    let env = setup();
    let key = "claude-code/claude:user/react-code-review";
    let folder = env.project.to_string_lossy().to_string();
    let settings = env.project.join(".claude/settings.local.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    let corrupt = "{ this is not valid json";
    std::fs::write(&settings, corrupt).unwrap();

    let resp = env.act(
        "disable",
        json!({ "skill_key": key, "scope": "folder", "folder": folder }),
    );
    assert_eq!(resp["ok"], json!(false), "{resp}");
    assert!(
        resp["error"].as_str().unwrap().contains("write_failed"),
        "{resp}"
    );
    assert_eq!(
        std::fs::read_to_string(&settings).unwrap(),
        corrupt,
        "corrupt file left untouched"
    );
}

#[test]
fn folder_scope_on_unsupported_source_is_refused() {
    // An other-agent source has neither `skillOverrides` nor a `permissions.deny` mechanism,
    // so a per-repo disable must be refused rather than silently applied globally (FR-024).
    let env = setup();
    let key = "other-agent/other-agent:user/diff-reviewer";
    let resp = env.act(
        "disable",
        json!({ "skill_key": key, "scope": "folder", "folder": env.project.to_string_lossy() }),
    );
    assert_eq!(resp["ok"], json!(false));
    assert_eq!(resp["error"], json!("folder_scope_unsupported"));
    assert_eq!(resp["would_be_scope"], json!("global"));
}

#[test]
fn plugin_per_repo_disable_enable_roundtrips_via_permission_deny() {
    use skill_inspector::model::SkillState;
    let env = setup();
    let key = "claude-code/claude:plugin:devkit-typescript/devkit.typescript.code-review";
    let folder = env.project.to_string_lossy().to_string();
    let settings = env.project.join(".claude/settings.local.json");
    let deny_rule = "Skill(devkit-typescript:devkit.typescript.code-review)";

    // Pre-existing settings and an unrelated deny rule must survive the write.
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    std::fs::write(&settings, r#"{"permissions":{"deny":["Bash(rm *)"]}}"#).unwrap();

    let resp = env.act(
        "disable",
        json!({ "skill_key": key, "scope": "folder", "folder": folder }),
    );
    assert_eq!(resp["ok"], json!(true), "{resp}");
    assert_eq!(resp["state"], json!("disabled-in-folder"));
    assert_eq!(resp["mechanism"], json!("permission-deny"));

    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    let deny = after["permissions"]["deny"].as_array().unwrap();
    assert!(
        deny.iter().any(|v| v == "Bash(rm *)"),
        "unrelated deny preserved: {after}"
    );
    assert!(
        deny.iter().any(|v| v == deny_rule),
        "skill deny rule added: {after}"
    );
    // The plugin's files are NEVER moved (unlike Tier-2 quarantine).
    let original_dir = env.home.join(
        ".claude/plugins/cache/devkit/devkit-typescript/1.0/skills/devkit.typescript.code-review",
    );
    assert!(original_dir.is_dir(), "plugin files untouched");
    assert!(matches!(
        env.skill_state("devkit.typescript.code-review"),
        Some(SkillState::DisabledInFolder)
    ));

    // Enable removes only the skill rule, preserves the unrelated deny, skill active again.
    let resp = env.act("enable", json!({ "skill_key": key, "folder": folder }));
    assert_eq!(resp["ok"], json!(true), "{resp}");
    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    let deny = after["permissions"]["deny"].as_array().unwrap();
    assert!(
        deny.iter().any(|v| v == "Bash(rm *)"),
        "unrelated deny kept"
    );
    assert!(
        !deny.iter().any(|v| v == deny_rule),
        "skill deny rule removed: {after}"
    );
    assert!(matches!(
        env.skill_state("devkit.typescript.code-review"),
        Some(SkillState::Active)
    ));
}

#[test]
fn tier2_quarantine_disable_enable_restores_byte_identical() {
    use skill_inspector::model::SkillState;
    let env = setup();
    let key = "claude-code/claude:plugin:devkit-typescript/devkit.typescript.code-review";

    let original_dir = env.home.join(
        ".claude/plugins/cache/devkit/devkit-typescript/1.0/skills/devkit.typescript.code-review",
    );
    let original_md = original_dir.join("SKILL.md");
    let original_bytes = std::fs::read(&original_md).unwrap();
    assert!(original_dir.is_dir());

    let resp = env.act("disable", json!({ "skill_key": key, "scope": "global" }));
    assert_eq!(resp["ok"], json!(true), "{resp}");
    assert_eq!(resp["state"], json!("disabled-global"));
    assert_eq!(resp["effective_scope"], json!("global"));
    assert!(!original_dir.exists(), "dir moved out of active root");
    assert!(matches!(
        env.skill_state("devkit.typescript.code-review"),
        Some(SkillState::DisabledGlobal)
    ));

    let resp = env.act("enable", json!({ "skill_key": key }));
    assert_eq!(resp["ok"], json!(true), "{resp}");
    assert!(original_dir.is_dir(), "restored to exact original path");
    assert_eq!(
        std::fs::read(&original_md).unwrap(),
        original_bytes,
        "byte-identical restore"
    );
    assert!(matches!(
        env.skill_state("devkit.typescript.code-review"),
        Some(SkillState::Active)
    ));
}

#[test]
fn remove_requires_confirmation_then_backs_up_and_deletes() {
    let env = setup();
    let key = "claude-code/claude:user/broken";
    let dir = env.home.join(".claude/skills/broken");
    assert!(dir.is_dir());

    // Unconfirmed: no change.
    let resp = env.act("remove", json!({ "skill_key": key }));
    assert_eq!(resp["ok"], json!(false));
    assert_eq!(resp["error"], json!("confirmation_required"));
    assert!(dir.is_dir(), "unconfirmed remove changes nothing");

    // Confirmed: backup then delete.
    let resp = env.act("remove", json!({ "skill_key": key, "confirm": true }));
    assert_eq!(resp["ok"], json!(true), "{resp}");
    let backup = resp["backup"].as_str().unwrap();
    assert!(
        Path::new(backup).join("SKILL.md").is_file(),
        "restorable backup"
    );
    assert!(!dir.exists(), "deleted from active root");
}

#[test]
fn write_to_readonly_root_fails_and_leaves_tree_unchanged() {
    let env = setup();
    let key = "claude-code/claude:user/code-review";
    let skills_root = env.home.join(".claude/skills");
    let dir = skills_root.join("code-review");
    let before = std::fs::read(dir.join("SKILL.md")).unwrap();

    // Make the active root read-only so moving a child out fails.
    set_readonly(&skills_root, true);
    let resp = env.act("disable", json!({ "skill_key": key, "scope": "global" }));
    set_readonly(&skills_root, false);

    assert_eq!(
        resp["ok"],
        json!(false),
        "read-only write must fail: {resp}"
    );
    assert!(dir.is_dir(), "tree unchanged");
    assert_eq!(std::fs::read(dir.join("SKILL.md")).unwrap(), before);
    // No quarantine record was written.
    let state = InspectorState::load_from(&env.state_path);
    assert!(state.quarantine.is_empty());
}

#[test]
fn codex_quarantine_roundtrips_and_builtin_remove_is_refused() {
    // T019 (FR-008, FR-013): a Codex skill quarantine disable→restore is lossless, and a destructive
    // `remove` on a built-in (`.system`) Codex skill is refused.
    use skill_inspector::model::SkillState;
    let _guard = CODEX_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex_home");
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("si-codex-act-{}-{}", std::process::id(), id));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let codex_home = root.join("codex");
    copy_dir(&src, &codex_home).unwrap();
    std::env::set_var("CODEX_HOME", &codex_home);

    let state_path = root.join("inspector-state.json");
    let quarantine_dir = root.join("quarantine");
    let trash_dir = root.join("trash");
    let inventory = || {
        let state = InspectorState::load_from(&state_path);
        // home is irrelevant once CODEX_HOME is set; pass root.
        build_inventory(
            &["codex".to_string()],
            &scan_context(root.clone(), root.clone()),
            &state,
        )
    };
    let act = |action: &str, req: serde_json::Value| {
        let inv = inventory();
        let ctx = ActionCtx {
            inventory: &inv,
            project_root: root.clone(),
            home: root.clone(),
            state_path: state_path.clone(),
            quarantine_dir: quarantine_dir.clone(),
            trash_dir: trash_dir.clone(),
        };
        dispatch(&ctx, action, &req)
    };
    let skill_state = |id: &str| -> Option<SkillState> {
        inventory()
            .skills
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.state)
    };

    // Native disable: writes `[[skills.config]] enabled=false` to config.toml (NO file move), then
    // re-enable removes it — reversible, and the skill's files never budge.
    let key = "codex/codex:user/my-skill";
    let original = codex_home.join("skills/my-skill/SKILL.md");
    let original_bytes = std::fs::read(&original).unwrap();
    let config = codex_home.join("config.toml");
    let config_before = std::fs::read_to_string(&config).unwrap();

    let resp = act("disable", json!({ "skill_key": key, "scope": "global" }));
    assert_eq!(resp["ok"], json!(true), "{resp}");
    assert_eq!(resp["state"], json!("disabled-global"));
    assert!(
        original.is_file(),
        "disable must NOT move the skill's files"
    );
    assert_eq!(
        std::fs::read(&original).unwrap(),
        original_bytes,
        "skill file untouched by disable"
    );
    // config.toml gained a `[[skills.config]]` entry that selects this skill's SKILL.md + disables it.
    let config_disabled = std::fs::read_to_string(&config).unwrap();
    assert!(
        config_disabled.contains("[[skills.config]]")
            && config_disabled.contains("my-skill/SKILL.md")
            && config_disabled.contains("enabled = false"),
        "config.toml carries the disable entry: {config_disabled}"
    );
    assert!(matches!(
        skill_state("my-skill"),
        Some(SkillState::DisabledGlobal)
    ));

    let resp = act("enable", json!({ "skill_key": key }));
    assert_eq!(resp["ok"], json!(true), "{resp}");
    assert!(original.is_file(), "still in place after enable");
    // Re-enable removes the entry, returning config.toml to its prior shape (byte-identical).
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        config_before,
        "config.toml restored byte-for-byte after re-enable"
    );
    assert!(matches!(skill_state("my-skill"), Some(SkillState::Active)));

    // A built-in `.system` skill may be disabled but its destructive remove is refused.
    let builtin_key = "codex/codex:user/skill-creator";
    let builtin_dir = codex_home.join("skills/.system/skill-creator");
    let resp = act(
        "remove",
        json!({ "skill_key": builtin_key, "confirm": true }),
    );
    assert_eq!(resp["ok"], json!(false), "{resp}");
    assert_eq!(resp["error"], json!("built_in_remove_refused"));
    assert!(builtin_dir.is_dir(), "built-in left intact");

    std::env::remove_var("CODEX_HOME");
    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(unix)]
fn set_readonly(dir: &Path, readonly: bool) {
    use std::os::unix::fs::PermissionsExt;
    let mode = if readonly { 0o555 } else { 0o755 };
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode));
}

#[cfg(not(unix))]
fn set_readonly(dir: &Path, readonly: bool) {
    if let Ok(meta) = std::fs::metadata(dir) {
        let mut perms = meta.permissions();
        perms.set_readonly(readonly);
        let _ = std::fs::set_permissions(dir, perms);
    }
}

#[cfg(unix)]
fn restore_perms(root: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fn walk(dir: &Path) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.filter_map(|e| e.ok()) {
                let p = e.path();
                let _ = std::fs::set_permissions(
                    &p,
                    std::fs::Permissions::from_mode(if p.is_dir() { 0o755 } else { 0o644 }),
                );
                if p.is_dir() {
                    walk(&p);
                }
            }
        }
    }
    walk(root);
    Ok(())
}

#[cfg(not(unix))]
fn restore_perms(_root: &Path) -> std::io::Result<()> {
    Ok(())
}
