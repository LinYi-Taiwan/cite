//! US4 — Conformance enforced by the compiler (T035).
//!
//! Each violation fails the build naming the rule; the per-target reference assertion does
//! NOT fire for non-applicable targets; stray + collision are reported.

#[path = "common.rs"]
mod common;

use skillc_core::diagnostics::Code;
use skillc_core::PipelineError;

fn hard_has(fixture: &str, target: &str, code: Code) {
    match common::build(fixture, target, "claude") {
        Err(PipelineError::Hard(diags)) => {
            assert!(
                diags.iter().any(|d| d.code == code),
                "fixture `{fixture}` should emit {:?}; got: {:?}",
                code,
                diags.iter().map(|d| d.code).collect::<Vec<_>>()
            );
        }
        other => panic!("fixture `{fixture}` expected Hard failure, got {other:?}"),
    }
}

#[test]
fn missing_required_frontmatter_fails() {
    hard_has("us4-missing-frontmatter", "admin", Code::SchemaInvalid);
}

#[test]
fn non_slug_id_fails_because_it_cannot_be_a_loadable_name() {
    // `id: bad_name` is a safe path segment but not a valid Agent Skills `name` (underscore).
    // Emitting it would produce an unloadable skill, so the compiler must reject it.
    hard_has("us4-bad-name-id", "admin", Code::SchemaInvalid);
}

#[test]
fn non_slug_pulled_id_fails_so_third_party_cannot_inject_unloadable_skill() {
    // A cross-repo pulled skill bypasses the local `validate` stage, but its id still becomes
    // a directory + the emitted `name`. The resolve stage must apply the same name rule so a
    // third-party source can't smuggle in an unloadable/path-unsafe skill.
    hard_has("us4-bad-pulled-id", "scm", Code::SchemaInvalid);
}

#[test]
fn per_target_reference_missing_fails_for_applicable_target() {
    hard_has("us4-missing-pertarget-ref", "admin", Code::ReferenceMissing);
}

#[test]
fn per_target_assertion_does_not_fire_for_non_applicable_target() {
    // `r` is mounted in admin, not in shop. Building shop must not assert references/shop.md.
    let report = common::build("us4-missing-pertarget-ref", "shop", "claude")
        .expect("shop build should succeed (r is not a member)");
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code == Code::ReferenceMissing),
        "per-target reference assertion must not fire for a non-applicable target"
    );
}

#[test]
fn stray_reference_filename_is_reported() {
    hard_has("us4-stray-ref", "admin", Code::ReferenceStray);
}

#[test]
fn shared_vs_target_collision_warns() {
    let report =
        common::build("us4-collision", "admin", "claude").expect("collision build succeeds");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == Code::ReferenceSharedCollision),
        "expected a shared-vs-target collision warning"
    );
}
