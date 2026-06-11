//! Security regression: untrusted path components (skill id, etc.) must be rejected before
//! any filesystem write escapes the output tree (see the code-review security findings).

#[path = "common.rs"]
mod common;

use skillc_core::diagnostics::Code;
use skillc_core::PipelineError;

#[test]
fn traversal_skill_id_is_rejected_and_writes_nothing() {
    // Sentinel path the malicious id tries to escape to; must never be created.
    let sentinel = std::path::Path::new("/tmp/skillc-pwned");
    let _ = std::fs::remove_dir_all(sentinel);

    let err = common::build("sec-traversal-id", "admin", "claude")
        .expect_err("a traversal skill id must fail the build");
    match err {
        PipelineError::Hard(diags) => {
            assert!(
                diags.iter().any(|d| d.code == Code::SchemaInvalid),
                "expected schema/invalid for the unsafe id; got {:?}",
                diags.iter().map(|d| d.code).collect::<Vec<_>>()
            );
        }
        other => panic!("expected Hard failure, got {other:?}"),
    }

    assert!(
        !sentinel.exists(),
        "the malicious id must NOT have written outside the output tree"
    );
}
