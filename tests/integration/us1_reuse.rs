//! US1 — Reusable blocks: change once, sync everywhere (T013).
//!
//! Asserts both includers inline the current block content, missing `@include` →
//! `error[include/missing]`, `@include` cycle → `error[cycle]`, and an unincluded block →
//! `warning[block/unused]`.

#[path = "common.rs"]
mod common;

use skillc_core::diagnostics::{Code, Severity};
use skillc_core::PipelineError;

fn skill_content<'a>(report: &'a skillc_core::BuildReport, id: &str) -> &'a str {
    report
        .bundle
        .skills
        .iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("skill `{id}` not in bundle"))
        .content
        .as_str()
}

#[test]
fn block_inlined_into_every_includer() {
    let report = common::build("us1-reuse", "admin", "claude").expect("build should succeed");

    // Both skills inline the *block's* content (FR-001), without the includer files being
    // hand-edited — the @include directive is replaced by the block body.
    for id in ["frontend-coding", "git"] {
        let content = skill_content(&report, id);
        assert!(
            content.contains("Keep PRs small."),
            "skill `{id}` should inline pr-rules content, got:\n{content}"
        );
        assert!(
            !content.contains("@include pr-rules"),
            "skill `{id}` should not retain the raw @include directive"
        );
    }
}

#[test]
fn unused_block_warns() {
    let report = common::build("us1-reuse", "admin", "claude").expect("build should succeed");
    let warned = report
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Warning && d.code == Code::BlockUnused);
    assert!(warned, "expected a block/unused warning for `experimental`");
}

#[test]
fn missing_block_is_hard_error() {
    let err = common::build("us1-missing-block", "admin", "claude")
        .expect_err("missing @include must fail the build");
    match err {
        PipelineError::Hard(diags) => {
            assert!(
                diags.iter().any(|d| d.code == Code::IncludeMissing),
                "expected error[include/missing]"
            );
        }
        other => panic!("expected Hard failure, got {other:?}"),
    }
}

#[test]
fn include_cycle_is_hard_error() {
    let err = common::build("us1-cycle", "admin", "claude")
        .expect_err("@include cycle must fail the build");
    match err {
        PipelineError::Hard(diags) => {
            assert!(
                diags.iter().any(|d| d.code == Code::Cycle),
                "expected error[cycle]"
            );
        }
        other => panic!("expected Hard failure, got {other:?}"),
    }
}
