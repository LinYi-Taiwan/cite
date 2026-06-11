//! Maintainer tooling: `skillc check` (validate-only), `skillc why` (reverse-dependency
//! query), and install pruning of stale skills via the destination receipt.

#[path = "common.rs"]
mod common;

use skillc_core::{CheckOptions, InstallOptions, PipelineError, WhyOptions};

fn check(
    fixture: &str,
    target: Option<&str>,
) -> Result<skillc_core::diagnostics::Diagnostics, PipelineError> {
    skillc_core::run_check(&CheckOptions {
        catalog: common::fixture(fixture),
        config: None,
        target: target.map(str::to_string),
        locked: false,
        frozen: false,
    })
}

fn why(fixture: &str, id: &str) -> Result<String, PipelineError> {
    skillc_core::run_why(&WhyOptions {
        catalog: common::fixture(fixture),
        config: None,
        id: id.to_string(),
    })
}

// --- check ---

#[test]
fn check_passes_on_valid_catalog_without_writing_dist() {
    let diags = check("us1-reuse", None).expect("valid catalog should check clean");
    // Warnings (e.g. unused block) are surfaced, not fatal.
    assert!(!diags.has_errors());
    // check never writes an artifact: no dist appears in the fixture.
    assert!(!common::fixture("us1-reuse").join("dist").exists());
}

#[test]
fn check_fails_on_broken_catalog() {
    let err = check("us1-missing-block", None).expect_err("missing include must fail check");
    match err {
        PipelineError::Hard(diags) => assert!(diags.has_errors()),
        other => panic!("expected Hard, got {other:?}"),
    }
}

#[test]
fn check_covers_every_target_and_dedups_repeated_lines() {
    // us3 fixture has multiple targets and an orphan skill; the orphan warning is
    // catalog-wide and must appear once, not once per target.
    let diags = check("us3-entry-pull", None).expect("check should pass with warnings");
    let orphans = diags
        .iter()
        .filter(|d| d.code == skillc_core::diagnostics::Code::SkillOrphan)
        .count();
    assert_eq!(orphans, 1, "orphan warning must be deduped across targets");
}

#[test]
fn check_unknown_target_is_usage_error_naming_registry() {
    let err = check("us1-reuse", Some("nope")).expect_err("unknown target");
    match err {
        PipelineError::Usage(d) => assert!(
            d.message.contains("registered targets:"),
            "should list the registry, got: {}",
            d.message
        ),
        other => panic!("expected Usage, got {other:?}"),
    }
}

// --- why ---

#[test]
fn why_block_names_includers_and_targets() {
    let text = why("us1-reuse", "pr-rules").expect("why should succeed");
    assert!(
        text.contains("block `pr-rules`"),
        "header names the block: {text}"
    );
    assert!(
        text.contains("frontend-coding") && text.contains("git"),
        "both includers listed: {text}"
    );
    assert!(text.contains("ships in targets:"), "target section: {text}");
}

#[test]
fn why_skill_names_mounts_and_importers() {
    let text = why("us2-markers-deps", "code-review").expect("why should succeed");
    assert!(text.contains("skill `code-review`"), "{text}");
    assert!(
        text.contains("imported by skills:") && text.contains("reviewer"),
        "importers listed: {text}"
    );
}

#[test]
fn why_unknown_id_is_usage_error_with_inventory() {
    let err = why("us1-reuse", "no-such-unit").expect_err("unknown id");
    match err {
        PipelineError::Usage(d) => {
            assert!(d.message.contains("not a skill or block"), "{}", d.message);
            assert!(
                d.message.contains("skills:"),
                "inventory listed: {}",
                d.message
            );
        }
        other => panic!("expected Usage, got {other:?}"),
    }
}

// --- install prune ---

fn install(artifact: &std::path::Path, dest: &std::path::Path) {
    skillc_core::run_install(&InstallOptions {
        artifact: artifact.to_path_buf(),
        dest: dest.to_path_buf(),
        agent: None,
    })
    .expect("install should succeed");
}

#[test]
fn install_prunes_skills_dropped_from_the_artifact() {
    // Build the us3 fixture for scm (ships frontend-coding + git via mounts).
    let report = common::build("us3-entry-pull", "scm", "claude").expect("build scm");
    let dest = common::out_dir("prune-dest");
    install(&report.artifact_dir, &dest);
    assert!(dest.join("skills/frontend-coding/SKILL.md").is_file());

    // A hand-authored skill the receipt never recorded must survive pruning.
    let hand = dest.join("skills/hand-authored");
    std::fs::create_dir_all(&hand).unwrap();
    std::fs::write(hand.join("SKILL.md"), "---\nname: hand-authored\n---\n").unwrap();

    // Re-install an artifact for the SAME (target, agent) that no longer carries
    // frontend-coding: build the admin target (different membership) and rewrite its
    // manifest target to scm — simulating the unmount without editing the fixture.
    let report2 = common::build("us3-entry-pull", "admin", "claude").expect("build admin");
    let manifest_path = report2.artifact_dir.join("manifest.json");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap();
    std::fs::write(
        &manifest_path,
        manifest.replace("\"target\": \"admin\"", "\"target\": \"scm\""),
    )
    .unwrap();
    install(&report2.artifact_dir, &dest);

    assert!(
        !dest.join("skills/frontend-coding").exists(),
        "skill dropped from the artifact must be pruned from the destination"
    );
    assert!(
        hand.join("SKILL.md").is_file(),
        "hand-authored skills are never pruned"
    );
}

#[test]
fn install_keeps_skills_owned_by_another_target_in_shared_dest() {
    // scm and admin both install into ONE destination; both bundles contain `git`.
    let scm = common::build("us3-entry-pull", "scm", "claude").expect("build scm");
    let admin = common::build("us3-entry-pull", "admin", "claude").expect("build admin");
    let dest = common::out_dir("prune-shared-dest");
    install(&scm.artifact_dir, &dest);
    install(&admin.artifact_dir, &dest);

    // Re-install scm from an artifact stripped to empty skills (manifest edit): scm's
    // skills retire, but anything admin still owns must stay.
    let manifest_path = scm.artifact_dir.join("manifest.json");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    let mut v = v;
    v["skills"] = serde_json::json!([]);
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    install(&scm.artifact_dir, &dest);

    let admin_manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(admin.artifact_dir.join("manifest.json")).unwrap(),
    )
    .unwrap();
    for skill in admin_manifest["skills"].as_array().unwrap() {
        let id = skill["id"].as_str().unwrap();
        assert!(
            dest.join("skills").join(id).join("SKILL.md").is_file(),
            "skill `{id}` still owned by admin must survive scm's prune"
        );
    }
}

#[test]
fn install_is_idempotent_with_receipt() {
    let report = common::build("us1-reuse", "admin", "claude").expect("build");
    let dest = common::out_dir("prune-idempotent");
    install(&report.artifact_dir, &dest);
    let receipt1 =
        std::fs::read_to_string(dest.join(skillc_core::install::RECEIPT_FILENAME)).unwrap();
    install(&report.artifact_dir, &dest);
    let receipt2 =
        std::fs::read_to_string(dest.join(skillc_core::install::RECEIPT_FILENAME)).unwrap();
    assert_eq!(receipt1, receipt2, "re-install must be receipt-idempotent");
}
