//! US1 (T012) + US4 context (T029) + US5 multi-agent (T033).
//!
//! Multi-source scan over the fixture lists every skill once, flags `broken` incomplete (not
//! dropped), reports a missing root as `missing` with success, marks a no-record agent's
//! context `unavailable` (never fabricated), and spans agents in US5.

use std::path::{Path, PathBuf};

use skill_inspector::model::{Availability, ContextAvailability, SkillState};
use skill_inspector::state::InspectorState;
use skill_inspector::{build_export, scan_context};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
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
fn multi_source_scan_lists_every_skill_once() {
    let fx = fixtures().join("multi-source");
    let ctx = scan_context(fx.join("project"), fx.join("home"));
    let export = build_export(
        &["claude-code".to_string()],
        &ctx,
        &InspectorState::default(),
    );

    // `broken` is present and flagged incomplete (FR-003) — never dropped.
    let broken = export
        .skills
        .iter()
        .find(|s| s.id == "broken")
        .expect("broken is listed");
    assert!(
        !broken.metadata_complete,
        "broken must be flagged incomplete"
    );

    // qa-playwright appears once per installed instance (user + project) = 2 entries.
    let qa = export
        .skills
        .iter()
        .filter(|s| s.id == "qa-playwright")
        .count();
    assert_eq!(qa, 2, "one entry per installed instance");

    // Every skill carries source_id + agent (SC-006); active by default here.
    for s in &export.skills {
        assert!(!s.source_id.is_empty());
        assert_eq!(s.agent, "claude-code");
    }
    let code_review = export
        .skills
        .iter()
        .find(|s| s.id == "code-review")
        .unwrap();
    assert!(matches!(code_review.state, SkillState::Active));

    // Context: claude-code exposes no transcript here → exactly one `unavailable` record with
    // empty keys (FR-018, SC-007) — never fabricated.
    assert_eq!(export.context_loads.len(), 1);
    let cl = &export.context_loads[0];
    assert!(matches!(cl.availability, ContextAvailability::Unavailable));
    assert!(cl.loaded_skill_keys.is_empty());

    // Activation reflects on-disk state: active skills are eligible + active.
    let act = export
        .activation
        .iter()
        .find(|a| a.skill_key.ends_with("/code-review"))
        .unwrap();
    assert!(act.eligible && act.active);

    settings_for(&fx).bind(|| {
        insta::assert_json_snapshot!("multi_source_export", export);
    });
}

#[test]
fn static_html_snapshot_escapes_script_breakout() {
    // A skill field containing `</script>` must not close the injected <script> block in the
    // self-contained snapshot (serde_json does not escape `<`).
    let json = r#"{"d":"</script><script>alert(1)</script>"}"#;
    let html = skill_inspector::render_static_html(json);
    assert!(
        !html.contains("</script><script>alert"),
        "raw </script> must not survive into the snapshot"
    );
    assert!(html.contains(r"<\/script>"), "</ is neutralized as <\\/");
}

#[test]
fn missing_project_root_is_reported_and_scan_succeeds() {
    let fx = fixtures().join("multi-source");
    let tmp = std::env::temp_dir().join(format!("si-missing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let ctx = scan_context(tmp.clone(), fx.join("home"));
    let export = build_export(
        &["claude-code".to_string()],
        &ctx,
        &InspectorState::default(),
    );

    let proj = export
        .sources
        .iter()
        .find(|s| s.id == "claude:project")
        .expect("project source present");
    assert_eq!(proj.availability, Availability::Missing);
    // User-level skills still listed despite the missing project root.
    assert!(export.skills.iter().any(|s| s.id == "code-review"));

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn spans_multiple_agents_and_clusters_across_them() {
    // US5 (T033): both agents appear labeled; a similarity cluster spans agents.
    let fx = fixtures().join("multi-source");
    let ctx = scan_context(fx.join("project"), fx.join("home"));
    let agents = vec!["claude-code".to_string(), "other-agent".to_string()];
    let export = build_export(&agents, &ctx, &InspectorState::default());

    assert!(export.skills.iter().any(|s| s.agent == "claude-code"));
    let other = export
        .skills
        .iter()
        .find(|s| s.agent == "other-agent")
        .expect("other-agent skill present");
    assert_eq!(other.id, "diff-reviewer");

    // The code-review similarity cluster must include the cross-agent diff-reviewer.
    let cross = export.clusters.iter().any(|c| {
        c.members.iter().any(|m| m.starts_with("claude-code/"))
            && c.members.iter().any(|m| m.starts_with("other-agent/"))
    });
    assert!(cross, "a cluster should span agents");

    // other-agent exposes no transcript → its context is unavailable too.
    assert!(export
        .context_loads
        .iter()
        .any(|r| r.turn_ref.starts_with("other-agent")
            && matches!(r.availability, ContextAvailability::Unavailable)));
}
