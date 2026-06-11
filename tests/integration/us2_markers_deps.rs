//! US2 — Unambiguous references to imported units (T020).
//!
//! Asserts `{{Alias}}` resolution, cross-repo pull, single dedup copy, undefined-marker
//! and version-conflict hard failures, and the unused-import warning.

#[path = "common.rs"]
mod common;

use skillc_core::diagnostics::Code;
use skillc_core::PipelineError;

fn find<'a>(
    report: &'a skillc_core::BuildReport,
    id: &str,
) -> Option<&'a skillc_core::assemble::BundledSkill> {
    report.bundle.skills.iter().find(|s| s.id == id)
}

#[test]
fn marker_resolves_and_dep_is_pulled_once() {
    let report = common::build("us2-markers-deps", "scm", "claude").expect("build should succeed");

    // The cross-repo dep is pulled in as a top-level skill (FR-008)...
    let cr = find(&report, "code-review").expect("code-review should be pulled into the bundle");
    assert!(
        cr.reason.starts_with("pulled-by:"),
        "expected pulled-by reason, got `{}`",
        cr.reason
    );
    // ...with its own @include expanded against its source mirror.
    assert!(
        cr.content.contains("Check naming."),
        "pulled dep should expand its own @include"
    );

    // ...and appears exactly once even though two skills import it (diamond dedup, FR-010).
    let copies = report
        .bundle
        .skills
        .iter()
        .filter(|s| s.id == "code-review")
        .count();
    assert_eq!(copies, 1, "code-review must be deduped to a single copy");

    // The marker rendered as a followable relative link, not a bare slug, so the reading
    // agent can navigate to the co-bundled skill; the raw marker text is gone (FR-007).
    let reviewer = find(&report, "reviewer").expect("reviewer present");
    assert!(
        reviewer
            .content
            .contains("[code-review](../code-review/SKILL.md)"),
        "marker should render a relative link to the resolved skill, got:\n{}",
        reviewer.content
    );
    assert!(
        !reviewer.content.contains("{{CodeReview}}"),
        "raw marker should be rendered away"
    );
}

#[test]
fn pulled_skill_brings_its_own_imports_references_and_rendered_markers() {
    // FR-008/FR-009: importing `jira` (cross-repo) must also pull what JIRA imports
    // (`bitbucket`), carry jira's references, and render jira's own {{BB}} marker —
    // a pulled skill is as fully compiled as a local one.
    let report =
        common::build("us2-transitive-pull", "web", "claude").expect("build should succeed");

    let jira = report
        .bundle
        .skills
        .iter()
        .find(|s| s.id == "jira")
        .expect("jira pulled");
    assert_eq!(jira.reason, "pulled-by:main");
    assert!(
        jira.content.contains("[bitbucket](../bitbucket/SKILL.md)"),
        "pulled skill's own marker must render as a link, got:\n{}",
        jira.content
    );
    assert!(
        !jira.content.contains("{{BB}}"),
        "no unresolved marker may ship in the bundle"
    );
    assert_eq!(
        jira.references.len(),
        1,
        "pulled skill's references must be carried"
    );
    assert_eq!(jira.references[0].stem, "fields");

    let bb = report
        .bundle
        .skills
        .iter()
        .find(|s| s.id == "bitbucket")
        .expect("transitive dependency of a pulled skill must be pulled too");
    assert_eq!(bb.reason, "pulled-by:jira");
}

#[test]
fn same_id_from_two_sources_is_hard_error_not_silent_shadowing() {
    // Supply-chain guard: source-a and source-b both provide skill id `dup`; importing
    // both must fail naming both sources, never first-one-wins.
    let work = common::out_dir("xsource-collision");
    write(
        &work.join("skillc.config.yaml"),
        "targets:\n  web:\n    entry:\n      mounts: [main]\nagents: [claude]\n",
    );
    write(
        &work.join("skills-lock.json"),
        r#"{"version":1,"sources":{"src-a":{"url":"u","commit":"c","path":"sources/a"},"src-b":{"url":"u","commit":"c","path":"sources/b"}}}"#,
    );
    write(
        &work.join("skills/main/SKILL.md"),
        "---\nid: main\nname: M\ndescription: m\nimports:\n  A: src-a:dup\n  B: src-b:dup\n---\n\nUse {{A}} and {{B}}.\n",
    );
    for src in ["a", "b"] {
        write(
            &work.join(format!("sources/{src}/skills/dup/SKILL.md")),
            &format!("---\nid: dup\nname: D\ndescription: from {src}\n---\n\nbody {src}\n"),
        );
    }

    let err = build_catalog(&work).expect_err("cross-source id collision must fail");
    match err {
        PipelineError::Hard(diags) => {
            let msg = diags
                .iter()
                .find(|d| d.code == Code::DepVersionConflict)
                .map(|d| d.message.clone())
                .expect("expected a dep conflict diagnostic");
            assert!(
                msg.contains("src-a") && msg.contains("src-b"),
                "must name both sources, got: {msg}"
            );
        }
        other => panic!("expected Hard, got {other:?}"),
    }
}

#[test]
fn runaway_import_chain_is_refused_not_stack_overflow() {
    // 200 unique-id skills chained via mirror-local imports: must fail with a named
    // diagnostic at the depth cap, not recurse unboundedly.
    let work = common::out_dir("deep-chain");
    write(
        &work.join("skillc.config.yaml"),
        "targets:\n  web:\n    entry:\n      mounts: [main]\nagents: [claude]\n",
    );
    write(
        &work.join("skills-lock.json"),
        r#"{"version":1,"sources":{"deep":{"url":"u","commit":"c","path":"sources/deep"}}}"#,
    );
    write(
        &work.join("skills/main/SKILL.md"),
        "---\nid: main\nname: M\ndescription: m\nimports:\n  Next: deep:c0\n---\n\nGo {{Next}}.\n",
    );
    for i in 0..200 {
        let next = format!("imports:\n  Next: ../c{}\n---\n\nGo {{{{Next}}}}.\n", i + 1);
        let body = if i < 199 {
            format!("---\nid: c{i}\nname: C\ndescription: d\n{next}")
        } else {
            format!("---\nid: c{i}\nname: C\ndescription: d\n---\n\nend\n")
        };
        write(
            &work.join(format!("sources/deep/skills/c{i}/SKILL.md")),
            &body,
        );
    }

    let err = build_catalog(&work).expect_err("chain past the cap must fail");
    match err {
        PipelineError::Hard(diags) => assert!(
            diags
                .iter()
                .any(|d| d.message.contains("dependency chain exceeds")),
            "expected the depth-cap diagnostic"
        ),
        other => panic!("expected Hard, got {other:?}"),
    }
}

#[test]
fn transitive_version_conflict_is_detected() {
    // main imports ext-a and ext-b; each pins a DIFFERENT version of shared dep `lib`.
    // The conflict happens one level below the catalog — must still hard-error (FR-011).
    let work = common::out_dir("transitive-version-conflict");
    write(
        &work.join("skillc.config.yaml"),
        "targets:\n  web:\n    entry:\n      mounts: [main]\nagents: [claude]\n",
    );
    write(
        &work.join("skills-lock.json"),
        r#"{"version":1,"sources":{"shared":{"url":"u","commit":"c","path":"sources/shared"}}}"#,
    );
    write(
        &work.join("skills/main/SKILL.md"),
        "---\nid: main\nname: M\ndescription: m\nimports:\n  A: shared:ext-a\n  B: shared:ext-b\n---\n\n{{A}} {{B}}\n",
    );
    write(
        &work.join("sources/shared/skills/ext-a/SKILL.md"),
        "---\nid: ext-a\nname: A\ndescription: a\nimports:\n  Lib: ../lib@1.0\n---\n\nuse {{Lib}}\n",
    );
    write(
        &work.join("sources/shared/skills/ext-b/SKILL.md"),
        "---\nid: ext-b\nname: B\ndescription: b\nimports:\n  Lib: ../lib@2.0\n---\n\nuse {{Lib}}\n",
    );
    write(
        &work.join("sources/shared/skills/lib/SKILL.md"),
        "---\nid: lib\nname: L\ndescription: l\n---\n\nlib body\n",
    );

    let err = build_catalog(&work).expect_err("transitive version conflict must fail");
    match err {
        PipelineError::Hard(diags) => assert!(
            diags.iter().any(|d| d.code == Code::DepVersionConflict),
            "expected error[dep/version-conflict]"
        ),
        other => panic!("expected Hard, got {other:?}"),
    }
}

fn write(path: &std::path::Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn build_catalog(catalog: &std::path::Path) -> Result<skillc_core::BuildReport, PipelineError> {
    skillc_core::run_build(&skillc_core::BuildOptions {
        catalog: catalog.to_path_buf(),
        config: None,
        out: catalog.join("dist"),
        target: "web".to_string(),
        agent: "claude".to_string(),
        locked: false,
        frozen: false,
    })
}

#[test]
fn locked_requires_content_hash_pin() {
    // The us2 fixture's lockfile pins no contentHash → --locked must refuse the build,
    // telling the user the hash to pin.
    let err = common::build_with(
        "us2-markers-deps",
        "scm",
        "claude",
        "locked-unpinned",
        true,
        false,
    )
    .expect_err("--locked must fail on an unpinned source");
    match err {
        PipelineError::Hard(diags) => {
            let msg = diags
                .iter()
                .find(|d| d.code == Code::SourceUnavailable)
                .map(|d| d.message.clone())
                .expect("expected error[source/unavailable]");
            assert!(
                msg.contains("no contentHash pin") && msg.contains("sha256:"),
                "message should name the missing pin and the current hash, got: {msg}"
            );
        }
        other => panic!("expected Hard failure, got {other:?}"),
    }
}

#[test]
fn content_hash_pin_is_verified() {
    // Copy the fixture, pin the REAL mirror hash → build passes (even under --locked).
    // Then corrupt the pin → build fails naming pinned vs actual.
    let src = common::fixture("us2-markers-deps");
    let work = common::out_dir("us2-hash-pin-catalog");
    copy_dir(&src, &work);

    let mirror = work.join("sources/shared-skills");
    let real = skillc_core::resolve::lock::hash_dir(&mirror).expect("hash_dir");

    let lock_path = work.join("skills-lock.json");
    let lock_text = std::fs::read_to_string(&lock_path).expect("read lock");
    let pinned = lock_text.replace(
        "\"url\"",
        &format!("\"contentHash\": \"{real}\",\n      \"url\""),
    );
    std::fs::write(&lock_path, &pinned).expect("write lock");

    let ok = build_at(&work, true);
    assert!(
        ok.is_ok(),
        "correctly-pinned --locked build should pass: {ok:?}"
    );

    // Corrupt the pin: same length, different bytes.
    let bad = pinned.replace(
        &real,
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    );
    std::fs::write(&lock_path, bad).expect("write corrupted lock");
    let err = build_at(&work, false).expect_err("drifted pin must fail even without --locked");
    match err {
        PipelineError::Hard(diags) => {
            let msg = diags
                .iter()
                .find(|d| d.code == Code::SourceUnavailable)
                .map(|d| d.message.clone())
                .expect("expected error[source/unavailable]");
            assert!(
                msg.contains("does not match") && msg.contains(&real),
                "message should name pinned vs actual, got: {msg}"
            );
        }
        other => panic!("expected Hard failure, got {other:?}"),
    }
}

fn build_at(
    catalog: &std::path::Path,
    locked: bool,
) -> Result<skillc_core::BuildReport, PipelineError> {
    skillc_core::run_build(&skillc_core::BuildOptions {
        catalog: catalog.to_path_buf(),
        config: None,
        out: catalog.join("dist"),
        target: "scm".to_string(),
        agent: "claude".to_string(),
        locked,
        frozen: false,
    })
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for entry in std::fs::read_dir(from).expect("read_dir") {
        let entry = entry.expect("entry");
        let dest = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), &dest).expect("copy");
        }
    }
}

#[test]
fn unused_import_warns() {
    let report = common::build("us2-markers-deps", "scm", "claude").expect("build should succeed");
    let warned = report
        .diagnostics
        .iter()
        .any(|d| d.code == Code::ImportUnused);
    assert!(warned, "expected an import/unused warning for LegacyHelper");
}

#[test]
fn undefined_marker_is_hard_error() {
    let err = common::build("us2-marker-undefined", "admin", "claude")
        .expect_err("undefined marker must fail the build");
    match err {
        PipelineError::Hard(diags) => {
            assert!(
                diags.iter().any(|d| d.code == Code::MarkerUndefined),
                "expected error[marker/undefined]"
            );
        }
        other => panic!("expected Hard failure, got {other:?}"),
    }
}

#[test]
fn version_conflict_is_hard_error() {
    let err = common::build("us2-version-conflict", "admin", "claude")
        .expect_err("conflicting versions must fail the build");
    match err {
        PipelineError::Hard(diags) => {
            assert!(
                diags.iter().any(|d| d.code == Code::DepVersionConflict),
                "expected error[dep/version-conflict]"
            );
        }
        other => panic!("expected Hard failure, got {other:?}"),
    }
}
