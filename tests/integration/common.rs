//! Shared helpers for integration tests. Included via `#[path] mod common;` because each
//! integration test is its own crate root (declared with an explicit `path` in
//! `crates/skillc-core/Cargo.toml`).

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use skillc_core::{BuildOptions, BuildReport, PipelineError};

/// Per-process counter so concurrent `build()` calls never share an output dir (emit
/// recreates the tree, so a shared dir would race under parallel tests).
static BUILD_SEQ: AtomicUsize = AtomicUsize::new(0);

/// Absolute path to `tests/fixtures/<name>` from the skillc-core crate dir.
pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// A throwaway output dir under `target/test-out/<name>` (deterministic path, no temp rng).
pub fn out_dir(name: &str) -> PathBuf {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("test-out")
        .join(name);
    let _ = std::fs::remove_dir_all(&p);
    p
}

/// Build a single (target, agent) from a fixture catalog with default flags. Each call gets
/// a unique output dir so parallel tests don't race on emit.
pub fn build(fixture_name: &str, target: &str, agent: &str) -> Result<BuildReport, PipelineError> {
    let seq = BUILD_SEQ.fetch_add(1, Ordering::Relaxed);
    let opts = BuildOptions {
        catalog: fixture(fixture_name),
        config: None,
        out: out_dir(&format!("{fixture_name}-{target}-{agent}-{seq}")),
        target: target.to_string(),
        agent: agent.to_string(),
        locked: false,
        frozen: false,
    };
    skillc_core::run_build(&opts)
}

/// Build with explicit flags (for --locked/--frozen scenarios).
pub fn build_with(
    fixture_name: &str,
    target: &str,
    agent: &str,
    out_suffix: &str,
    locked: bool,
    frozen: bool,
) -> Result<BuildReport, PipelineError> {
    let opts = BuildOptions {
        catalog: fixture(fixture_name),
        config: None,
        out: out_dir(&format!("{fixture_name}-{out_suffix}")),
        target: target.to_string(),
        agent: agent.to_string(),
        locked,
        frozen,
    };
    skillc_core::run_build(&opts)
}
