//! US2 (T018): the similarity cluster (code-review family), the duplicate-identity cluster
//! (the two qa-playwright), no false grouping of qa-playwright into the code-review cluster,
//! the empty-case `clusters_empty_reason`, and that scanning mutates nothing (SC-005).

use std::path::{Path, PathBuf};

use skill_inspector::model::ClusterKind;
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
fn clusters_capture_similarity_and_identity_without_false_grouping() {
    let fx = fixtures().join("multi-source");
    let ctx = scan_context(fx.join("project"), fx.join("home"));
    let export = build_export(
        &["claude-code".to_string()],
        &ctx,
        &InspectorState::default(),
    );

    // A similarity cluster groups the three code-review-family skills.
    let sim = export
        .clusters
        .iter()
        .find(|c| matches!(c.kind, ClusterKind::Similarity))
        .expect("a similarity cluster exists");
    assert!(sim.score.is_some(), "similarity carries a score");
    assert!(sim.reason.starts_with("shared terms:"), "human reason");
    assert!(sim.members.iter().any(|m| m.ends_with("/code-review")));
    assert!(sim
        .members
        .iter()
        .any(|m| m.ends_with("/react-code-review")));
    assert!(sim
        .members
        .iter()
        .any(|m| m.ends_with("/devkit.typescript.code-review")));
    // No false grouping: qa-playwright is NOT in the similarity cluster (FR-010).
    assert!(sim.members.iter().all(|m| !m.contains("qa-playwright")));

    // A duplicate-identity cluster for the two qa-playwright instances (FR-008).
    let dup = export
        .clusters
        .iter()
        .find(|c| matches!(c.kind, ClusterKind::DuplicateIdentity))
        .expect("a duplicate-identity cluster exists");
    assert_eq!(dup.members.len(), 2);
    assert!(dup.members.iter().all(|m| m.contains("qa-playwright")));
    assert_eq!(dup.reason, "identical content hash");
    assert!(dup.score.is_none());

    assert!(export.clusters_empty_reason.is_none());

    settings_for(&fx).bind(|| {
        insta::assert_json_snapshot!("clusters", export.clusters);
    });
}

#[test]
fn empty_overlap_reports_a_reason() {
    let fx = fixtures().join("no-overlap");
    let ctx = scan_context(fx.join("project"), fx.join("home"));
    let export = build_export(
        &["claude-code".to_string()],
        &ctx,
        &InspectorState::default(),
    );

    assert!(export.clusters.is_empty(), "no clusters for one lone skill");
    let reason = export
        .clusters_empty_reason
        .expect("empty reason is non-null when clusters == []");
    assert!(!reason.is_empty());
}

#[test]
fn scanning_mutates_nothing() {
    // SC-005: re-scan → identical fixture tree (no on-disk change from analysis).
    let fx = fixtures().join("multi-source");
    let before = tree_digest(&fx);
    let ctx = scan_context(fx.join("project"), fx.join("home"));
    let _ = build_export(
        &["claude-code".to_string()],
        &ctx,
        &InspectorState::default(),
    );
    let _ = build_export(
        &["claude-code".to_string()],
        &ctx,
        &InspectorState::default(),
    );
    let after = tree_digest(&fx);
    assert_eq!(before, after, "scan must not mutate the fixture tree");
}

/// A stable digest of every file path + length under `dir` (cheap mutation detector).
fn tree_digest(dir: &Path) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    collect(dir, dir, &mut out);
    out.sort();
    out
}

fn collect(base: &Path, dir: &Path, out: &mut Vec<(String, u64)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect(base, &path, out);
        } else if let Ok(meta) = path.metadata() {
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            out.push((rel, meta.len()));
        }
    }
}
