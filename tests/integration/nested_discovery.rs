//! Nested catalog layout — skills and blocks may live at any depth under `skills/`
//! and `blocks/`; intermediate directories are free-form grouping (layer, team,
//! domain) that the compiler imposes no taxonomy on.

#[path = "common.rs"]
mod common;

#[test]
fn skills_and_blocks_are_discovered_at_any_depth() {
    let r = common::build("nested-discovery", "demo", "claude").expect("build");

    let noise: Vec<_> = r.diagnostics.iter().collect();
    assert!(
        noise.is_empty(),
        "nested layout must produce zero diagnostics, got: {noise:?}"
    );

    let has = |id: &str| r.bundle.skills.iter().any(|s| s.id == id);
    assert!(has("composer"), "mounted skill under skills/organisms/");
    assert!(
        has("leaf"),
        "imported skill under skills/molecules/ pulled via ../../ local import"
    );

    let read = |id: &str| {
        std::fs::read_to_string(r.artifact_dir.join("skills").join(id).join("SKILL.md"))
            .unwrap_or_else(|e| panic!("read emitted {id}: {e}"))
    };
    assert!(
        read("leaf").contains("Shared credential note"),
        "block under blocks/shared/ inlined by id regardless of depth"
    );
    assert!(
        read("composer").contains("[leaf](../leaf/SKILL.md)"),
        "marker resolves to the flat dist link, independent of source grouping"
    );
}
