//! US3 (T018): the Codex plugin on/off write flips skill state, is format-preserving (only the one
//! `enabled` value changes — config.toml otherwise byte-stable), and is idempotent on a no-op.
//!
//! Runs against a temp COPY of the fixture `codex_home`; `CODEX_HOME` is set to it under a shared
//! lock (the env var is process-global) so parallel tests can't race on it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use serde_json::json;

use skill_inspector::action::{dispatch, ActionCtx};
use skill_inspector::fsutil::copy_dir;
use skill_inspector::model::SkillState;
use skill_inspector::state::InspectorState;
use skill_inspector::{build_inventory, scan_context};

static CODEX_ENV_LOCK: Mutex<()> = Mutex::new(());
static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Env {
    root: PathBuf,
    codex_home: PathBuf,
    home: PathBuf,
    state_path: PathBuf,
    quarantine_dir: PathBuf,
    trash_dir: PathBuf,
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn setup() -> Env {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex_home");
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("si-codex-{}-{}", std::process::id(), id));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let codex_home = root.join("codex");
    copy_dir(&src, &codex_home).unwrap();
    Env {
        home: root.join("home"),
        state_path: root.join("inspector-state.json"),
        quarantine_dir: root.join("quarantine"),
        trash_dir: root.join("trash"),
        codex_home,
        root,
    }
}

impl Env {
    fn config(&self) -> PathBuf {
        self.codex_home.join("config.toml")
    }
    fn inventory(&self) -> skill_inspector::scan::Inventory {
        let state = InspectorState::load_from(&self.state_path);
        build_inventory(
            &["codex".to_string()],
            &scan_context(self.home.clone(), self.home.clone()),
            &state,
        )
    }
    fn act(&self, action: &str, req: serde_json::Value) -> serde_json::Value {
        let inv = self.inventory();
        let ctx = ActionCtx {
            inventory: &inv,
            project_root: self.home.clone(),
            home: self.home.clone(),
            state_path: self.state_path.clone(),
            quarantine_dir: self.quarantine_dir.clone(),
            trash_dir: self.trash_dir.clone(),
        };
        dispatch(&ctx, action, &req)
    }
    fn skill_state(&self, id: &str) -> Option<SkillState> {
        self.inventory()
            .skills
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.state)
    }
}

#[test]
fn toggle_flips_state_is_format_preserving_and_idempotent() {
    let _guard = CODEX_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let env = setup();
    std::env::set_var("CODEX_HOME", &env.codex_home);

    // alpha starts enabled → its skills are Active.
    assert!(matches!(env.skill_state("a-one"), Some(SkillState::Active)));
    let before = std::fs::read_to_string(env.config()).unwrap();

    // Disable alpha.
    let resp = env.act(
        "plugin",
        json!({ "plugin": "alpha", "enabled": false, "agent": "codex" }),
    );
    assert_eq!(resp["ok"], json!(true), "{resp}");
    assert_eq!(resp["plugin"], json!("alpha@mkt"), "full key recovered");
    assert_eq!(resp["scope"], json!("global"));

    // Format-preserving: the ONLY change is alpha's `enabled = true` → `enabled = false`. alpha is
    // the first (and only) `enabled = true` in the fixture, so replacing the first occurrence is the
    // exact expected diff — every comment, table, and key ordering is otherwise byte-identical.
    let after = std::fs::read_to_string(env.config()).unwrap();
    assert_eq!(
        after,
        before.replacen("enabled = true", "enabled = false", 1),
        "only the targeted enabled flag changed"
    );

    // The plugin's skills now report DisabledPlugin.
    assert!(matches!(
        env.skill_state("a-one"),
        Some(SkillState::DisabledPlugin)
    ));

    // Idempotent: disabling again is a no-op success that leaves the file untouched.
    let resp = env.act(
        "plugin",
        json!({ "plugin": "alpha", "enabled": false, "agent": "codex" }),
    );
    assert_eq!(resp["ok"], json!(true), "{resp}");
    assert_eq!(
        std::fs::read_to_string(env.config()).unwrap(),
        after,
        "no-op toggle does not rewrite the file"
    );

    // Re-enable alpha → skills active again, config returns to the original bytes (reversible).
    let resp = env.act(
        "plugin",
        json!({ "plugin": "alpha", "enabled": true, "agent": "codex" }),
    );
    assert_eq!(resp["ok"], json!(true), "{resp}");
    assert_eq!(
        std::fs::read_to_string(env.config()).unwrap(),
        before,
        "re-enable restores the original config bytes"
    );
    assert!(matches!(env.skill_state("a-one"), Some(SkillState::Active)));

    std::env::remove_var("CODEX_HOME");
}

#[test]
fn unknown_plugin_key_errors_and_writes_nothing() {
    let _guard = CODEX_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let env = setup();
    std::env::set_var("CODEX_HOME", &env.codex_home);
    let before = std::fs::read_to_string(env.config()).unwrap();

    let resp = env.act(
        "plugin",
        json!({ "plugin": "nope", "enabled": false, "agent": "codex" }),
    );
    assert_eq!(resp["ok"], json!(false));
    assert_eq!(resp["error"], json!("plugin_not_found"));
    assert_eq!(
        std::fs::read_to_string(env.config()).unwrap(),
        before,
        "a failed toggle writes nothing"
    );

    std::env::remove_var("CODEX_HOME");
}
