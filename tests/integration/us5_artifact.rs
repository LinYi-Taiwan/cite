//! US5 — Inspectable, deterministic build artifact (T040).
//!
//! Asserts byte-identical double `build --locked --frozen` (with an `insta` golden of the
//! manifest), the manifest shape, an empty-but-valid artifact, and install idempotency.

#[path = "common.rs"]
mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use skillc_core::diagnostics::Code;
use skillc_core::InstallOptions;

fn read(p: &Path) -> String {
    fs::read_to_string(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// Map of dest-relative path → file bytes, for directory comparison.
fn snapshot_dir(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let mut entries: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                let rel = p.strip_prefix(base).unwrap().to_string_lossy().into_owned();
                out.insert(rel, fs::read(&p).unwrap());
            }
        }
    }
    walk(root, root, &mut out);
    out
}

#[test]
fn double_build_is_byte_identical() {
    let a =
        common::build_with("us1-reuse", "admin", "claude", "det-a", true, true).expect("build a");
    let b =
        common::build_with("us1-reuse", "admin", "claude", "det-b", true, true).expect("build b");

    assert_eq!(
        snapshot_dir(&a.artifact_dir),
        snapshot_dir(&b.artifact_dir),
        "identical inputs must produce a byte-identical artifact tree (FR-022)"
    );
}

#[test]
fn manifest_has_expected_shape_and_golden() {
    let r =
        common::build_with("us1-reuse", "admin", "claude", "manifest", true, true).expect("build");
    let manifest = read(&r.artifact_dir.join("manifest.json"));

    let v: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(v["target"], "admin");
    assert_eq!(v["agent"], "claude");

    let skills = v["skills"].as_array().unwrap();
    let fc = skills
        .iter()
        .find(|s| s["id"] == "frontend-coding")
        .expect("frontend-coding");
    assert_eq!(fc["reason"], "mounted");
    assert!(
        fc["includes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x == "pr-rules"),
        "manifest should record the inlined block"
    );
    assert!(
        v["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap().contains("block/unused")),
        "manifest should record the unused-block warning"
    );

    // Golden snapshot of the full manifest (determinism anchor).
    insta::assert_snapshot!("us1_reuse_manifest", manifest);
}

#[test]
fn emitted_skill_md_carries_loadable_frontmatter() {
    // Regression: parsing strips the authoring frontmatter; emit must re-render the agent's
    // required frontmatter (Claude: name + description) or the artifact is not a loadable
    // skill. Earlier the body was emitted bare, with no `---` block at all.
    let r = common::build("us1-reuse", "admin", "claude").expect("build");
    let skill_md = read(
        &r.artifact_dir
            .join("skills")
            .join("frontend-coding")
            .join("SKILL.md"),
    );

    assert!(
        skill_md.starts_with("---\n"),
        "SKILL.md must open with a YAML frontmatter fence; got:\n{skill_md}"
    );
    let fm_end = skill_md[4..]
        .find("\n---\n")
        .expect("frontmatter must have a closing fence")
        + 4;
    let frontmatter = &skill_md[4..fm_end];
    // Agent Skills spec: `name` is the slug == the skill directory name (lowercase + hyphen),
    // NOT the human title. `frontend-coding` is the dir under skills/; the human title
    // ("Frontend Coding Standards") must NOT be in the name field or it fails validation.
    assert!(
        frontmatter.contains("name: frontend-coding"),
        "name must be the slug matching the skill directory; got:\n{frontmatter}"
    );
    assert!(
        !frontmatter.contains("Frontend Coding Standards"),
        "the human title must not be emitted as the spec name; got:\n{frontmatter}"
    );
    assert!(
        frontmatter.contains("description: FE coding conventions and review checklist."),
        "frontmatter must carry the skill description; got:\n{frontmatter}"
    );
    // Authoring-only fields must NOT leak into the agent output.
    assert!(
        !frontmatter.contains("id:") && !frontmatter.contains("referenceMode"),
        "authoring-layer fields must not be emitted; got:\n{frontmatter}"
    );
}

#[test]
fn empty_entry_yields_empty_but_valid_artifact() {
    let r = common::build("us5-empty", "admin", "claude").expect("empty build still valid");
    assert!(r.bundle.skills.is_empty(), "empty entry → no skills");

    let v: serde_json::Value =
        serde_json::from_str(&read(&r.artifact_dir.join("manifest.json"))).unwrap();
    assert!(v["skills"].as_array().unwrap().is_empty());
    assert!(
        r.diagnostics.iter().any(|d| d.code == Code::EntryEmpty),
        "empty entry should warn"
    );
}

#[test]
fn install_is_idempotent() {
    let r = common::build_with("us1-reuse", "admin", "claude", "install-src", false, false)
        .expect("build");
    let dest = common::out_dir("us5-install-dest");

    let opts = InstallOptions {
        artifact: r.artifact_dir.clone(),
        dest: dest.clone(),
        agent: None,
    };
    skillc_core::run_install(&opts).expect("first install");
    let first = snapshot_dir(&dest);
    skillc_core::run_install(&opts).expect("second install");
    let second = snapshot_dir(&dest);

    assert_eq!(
        first, second,
        "re-installing identical bytes must be idempotent (FR-025)"
    );
    assert!(
        dest.join("skills")
            .join("frontend-coding")
            .join("SKILL.md")
            .exists(),
        "install should place the skill in the agent's format"
    );
}
