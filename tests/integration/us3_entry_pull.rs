//! US3 — Per-target bundles via entry-point pull (T028).
//!
//! Asserts membership-by-target, `warning[skill/orphan]`, `error[entry/missing]`, and
//! per-target reference pruning (foreign-target reference count = 0).

#[path = "common.rs"]
mod common;

use skillc_core::diagnostics::Code;
use skillc_core::{BuildReport, PipelineError};

fn has(report: &BuildReport, id: &str) -> bool {
    report.bundle.skills.iter().any(|s| s.id == id)
}

#[test]
fn membership_follows_the_target_entry() {
    let scm = common::build("us3-entry-pull", "scm", "claude").expect("scm build");
    assert!(
        has(&scm, "frontend-coding"),
        "frontend-coding is mounted in scm"
    );
    assert!(!has(&scm, "git"), "git is mounted in admin only, not scm");

    let admin = common::build("us3-entry-pull", "admin", "claude").expect("admin build");
    assert!(has(&admin, "git"), "git is mounted in admin");
    assert!(
        !has(&admin, "frontend-coding"),
        "frontend-coding is mounted in scm only"
    );
}

#[test]
fn orphan_skill_warns() {
    let scm = common::build("us3-entry-pull", "scm", "claude").expect("scm build");
    let warned = scm
        .diagnostics
        .iter()
        .any(|d| d.code == Code::SkillOrphan && d.message.contains("orphan-skill"));
    assert!(warned, "expected skill/orphan warning for orphan-skill");
}

#[test]
fn references_pruned_to_compiled_target() {
    let scm = common::build("us3-entry-pull", "scm", "claude").expect("scm build");
    let fc = scm
        .bundle
        .skills
        .iter()
        .find(|s| s.id == "frontend-coding")
        .unwrap();
    let stems: Vec<&str> = fc.references.iter().map(|r| r.stem.as_str()).collect();

    // This target's per-target reference + the shared one are retained...
    assert!(stems.contains(&"scm"), "scm reference retained");
    assert!(stems.contains(&"glossary"), "shared reference retained");
    // ...and the foreign target's reference is dropped (FR-015, SC-006).
    assert!(
        !stems.contains(&"admin"),
        "foreign-target reference must be pruned"
    );
}

#[test]
fn missing_mount_is_hard_error() {
    let err = common::build("us3-missing-mount", "admin", "claude")
        .expect_err("mounting a missing skill must fail");
    match err {
        PipelineError::Hard(diags) => {
            assert!(
                diags.iter().any(|d| d.code == Code::EntryMissing),
                "expected entry/missing"
            );
        }
        other => panic!("expected Hard failure, got {other:?}"),
    }
}
