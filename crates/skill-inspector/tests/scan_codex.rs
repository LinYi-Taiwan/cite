//! US1 (T008) + US2 (T013): the Codex provider discovers Codex's real source kinds, attributes
//! every skill to the `codex` agent, flags `.system` built-ins, maps a disabled plugin's skills to
//! `DisabledPlugin`, and degrades gracefully when Codex isn't installed.
//!
//! Codex home is resolved from `$CODEX_HOME`; these tests set it to the fixture (or a missing path)
//! under a shared lock so the process-global env var can't race across parallel tests.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use skill_inspector::model::{Availability, SkillState, SourceKind};
use skill_inspector::state::InspectorState;
use skill_inspector::{build_export, scan_context};

/// Serialize the `CODEX_HOME` mutations: set_var/remove_var touch process-global state, so two
/// tests flipping it concurrently would see each other's value.
static CODEX_ENV_LOCK: Mutex<()> = Mutex::new(());

fn fixture_codex_home() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex_home")
}

fn regex_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if "\\.+*?()|[]{}^$".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn settings_for(root: &Path) -> insta::Settings {
    let mut s = insta::Settings::clone_current();
    s.add_filter(&regex_escape(&root.to_string_lossy()), "<FIX>");
    s.set_prepend_module_to_snapshot(false);
    s
}

#[test]
fn discovers_codex_sources_attributes_and_built_ins() {
    let _guard = CODEX_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let codex_home = fixture_codex_home();
    std::env::set_var("CODEX_HOME", &codex_home);

    // home/project are irrelevant once CODEX_HOME is set; use a throwaway dir.
    let tmp = std::env::temp_dir();
    let ctx = scan_context(tmp.clone(), tmp.clone());
    let export = build_export(&["codex".to_string()], &ctx, &InspectorState::default());
    std::env::remove_var("CODEX_HOME");

    // Every Codex skill carries agent="codex" and a source_id + state (SC-003).
    let codex: Vec<_> = export
        .skills
        .iter()
        .filter(|s| s.agent == "codex")
        .collect();
    assert!(!codex.is_empty(), "Codex skills discovered");
    for s in &codex {
        assert!(!s.source_id.is_empty());
    }

    // The global/user skill is present and NOT built-in.
    let user = codex
        .iter()
        .find(|s| s.id == "my-skill")
        .expect("user skill discovered");
    assert_eq!(user.source_id, "codex:user");
    assert!(!user.built_in);

    // The `.system` skill is discovered under codex:user AND flagged built_in (FR-008).
    let sys = codex
        .iter()
        .find(|s| s.id == "skill-creator")
        .expect("built-in .system skill discovered");
    assert_eq!(sys.source_id, "codex:user");
    assert!(sys.built_in, "skill under .system/ must be built_in");

    // Per-plugin attribution (US2): alpha (enabled) → active; beta (disabled) → DisabledPlugin.
    let a_one = codex.iter().find(|s| s.id == "a-one").expect("alpha skill");
    assert_eq!(a_one.source_id, "codex:plugin:alpha");
    assert!(matches!(a_one.state, SkillState::Active));

    let b_one = codex.iter().find(|s| s.id == "b-one").expect("beta skill");
    assert_eq!(b_one.source_id, "codex:plugin:beta");
    assert!(
        matches!(b_one.state, SkillState::DisabledPlugin),
        "a disabled plugin's skills are DisabledPlugin"
    );

    // SC-002: every fixture plugin skill (a-one, a-two, b-one, b-two) is found.
    let plugin_skills = codex
        .iter()
        .filter(|s| s.source_id.starts_with("codex:plugin:"))
        .count();
    assert_eq!(plugin_skills, 4, "all installed plugin skills discovered");

    // A disabled plugin source is still EMITTED (its skills are shown, not hidden).
    assert!(export.sources.iter().any(|s| s.id == "codex:plugin:beta"));

    settings_for(&codex_home).bind(|| {
        insta::assert_json_snapshot!("scan_codex_export", export.skills);
    });
}

#[test]
fn discovers_agents_user_and_project_skills() {
    // Codex reads personal skills from `$HOME/.agents/skills` and project skills from
    // `<project_root>/.agents/skills` (developers.openai.com/codex/skills) — both must surface,
    // attributed to the codex agent under their own source ids/kinds.
    let _guard = CODEX_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex_agents");
    // Pin codex_home to a guaranteed-absent path so `codex:user`/plugins contribute nothing and only
    // the .agents/skills roots carry skills — deterministic regardless of the host's real $CODEX_HOME
    // or the fixture tree growing a `home/.codex` later (mirrors missing_codex_home_degrades_gracefully).
    std::env::set_var("CODEX_HOME", base.join("codex-home-absent"));

    let ctx = scan_context(base.join("project"), base.join("home"));
    let export = build_export(&["codex".to_string()], &ctx, &InspectorState::default());
    std::env::remove_var("CODEX_HOME");

    // Personal skill from ~/.agents/skills → codex:user-agents, kind User, not built-in.
    let personal = export
        .skills
        .iter()
        .find(|s| s.id == "personal-skill")
        .expect("personal ~/.agents/skills skill discovered");
    assert_eq!(personal.source_id, "codex:user-agents");
    assert_eq!(personal.agent, "codex");
    assert!(!personal.built_in);

    // Project skill from <project>/.agents/skills → codex:project, kind Project.
    let proj = export
        .skills
        .iter()
        .find(|s| s.id == "proj-skill")
        .expect("project .agents/skills skill discovered");
    assert_eq!(proj.source_id, "codex:project");
    assert_eq!(proj.agent, "codex");

    // Both new sources are emitted with the right kind and classified Readable.
    let agents_user = export
        .sources
        .iter()
        .find(|s| s.id == "codex:user-agents")
        .expect("codex:user-agents source emitted");
    assert_eq!(agents_user.kind, SourceKind::User);
    assert_eq!(agents_user.availability, Availability::Readable);

    let project = export
        .sources
        .iter()
        .find(|s| s.id == "codex:project")
        .expect("codex:project source emitted");
    assert_eq!(project.kind, SourceKind::Project);
    assert_eq!(project.availability, Availability::Readable);
}

#[test]
fn skills_config_disable_marks_skill_disabled_global() {
    // Codex's native per-skill switch: `[[skills.config]] enabled=false` (by SKILL.md path OR by
    // name) drops the skill from its loaded set — the scanner must surface it as DisabledGlobal.
    let _guard = CODEX_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex_agents");

    // A throwaway CODEX_HOME carrying ONLY a config.toml that disables two skills:
    //  - `proj-skill` by its exact SKILL.md path (path selector)
    //  - `personal-skill` by name (name selector)
    let codex_home = std::env::temp_dir().join(format!("si-codex-disable-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&codex_home);
    std::fs::create_dir_all(&codex_home).unwrap();
    let proj_skill_md = base
        .join("project/.agents/skills/proj-skill/SKILL.md")
        .to_string_lossy()
        .into_owned();
    std::fs::write(
        codex_home.join("config.toml"),
        format!(
            "[[skills.config]]\npath = \"{proj_skill_md}\"\nenabled = false\n\n\
             [[skills.config]]\nname = \"personal-skill\"\nenabled = false\n"
        ),
    )
    .unwrap();
    std::env::set_var("CODEX_HOME", &codex_home);

    let ctx = scan_context(base.join("project"), base.join("home"));
    let export = build_export(&["codex".to_string()], &ctx, &InspectorState::default());
    std::env::remove_var("CODEX_HOME");

    let state_of = |id: &str| export.skills.iter().find(|s| s.id == id).map(|s| s.state);
    assert_eq!(
        state_of("proj-skill"),
        Some(SkillState::DisabledGlobal),
        "path-selector disable → DisabledGlobal"
    );
    assert_eq!(
        state_of("personal-skill"),
        Some(SkillState::DisabledGlobal),
        "name-selector disable → DisabledGlobal"
    );
    // A skill NOT named in skills.config stays active (control).
    assert_eq!(state_of("keep-active"), Some(SkillState::Active));

    let _ = std::fs::remove_dir_all(&codex_home);
}

#[test]
fn missing_codex_home_degrades_gracefully() {
    let _guard = CODEX_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let missing = std::env::temp_dir().join(format!("si-codex-absent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&missing);
    std::env::set_var("CODEX_HOME", &missing);

    let tmp = std::env::temp_dir();
    let ctx = scan_context(tmp.clone(), tmp.clone());
    let export = build_export(&["codex".to_string()], &ctx, &InspectorState::default());
    std::env::remove_var("CODEX_HOME");

    // No Codex installed → no skills, but a valid result (no panic/error) with the user source
    // present and classified Missing.
    assert!(!export.skills.iter().any(|s| s.agent == "codex"));
    let user = export
        .sources
        .iter()
        .find(|s| s.id == "codex:user")
        .expect("user source always emitted");
    assert_eq!(user.availability, Availability::Missing);
}
