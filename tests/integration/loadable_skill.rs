//! Output-quality conformance: every `SKILL.md` the compiler emits must be a skill that an
//! agent can actually load and use — otherwise the framework is pointless. These tests are
//! the executable definition of "usable output":
//!
//!   1. Loadable — valid YAML frontmatter; `name` is a spec-legal slug equal to the skill
//!      directory; `description` is non-empty and within the length cap; no authoring-layer
//!      fields leak through (https://agentskills.io/specification).
//!   2. Self-contained — no unresolved `@include` directive and no unrendered `{{Alias}}`
//!      marker survives into the artifact (FR-009 / SC-002); the body is non-empty.
//!
//! The frontmatter rules below are re-implemented here (not imported from the library) on
//! purpose: the test is an INDEPENDENT oracle over the emitted bytes, so a bug in the
//! compiler's own validator cannot mask itself.

#[path = "common.rs"]
mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_norway::Value;
use skillc_core::{run_build, BuildOptions, BuildReport};

/// Agent Skills `name` rules: 1–64 chars, ASCII lowercase letters/digits/hyphens, no
/// leading/trailing or consecutive hyphen, and equal to the skill's directory name.
fn assert_valid_name(name: &str, dir: &str) {
    assert!(
        !name.is_empty() && name.len() <= 64,
        "name must be 1–64 chars; got {name:?}"
    );
    assert!(
        !name.starts_with('-') && !name.ends_with('-') && !name.contains("--"),
        "name must not start/end with or repeat a hyphen; got {name:?}"
    );
    assert!(
        name.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
        "name must be a lowercase letters/digits/hyphens slug; got {name:?}"
    );
    assert_eq!(
        name, dir,
        "name must equal the skill directory (loader requirement)"
    );
}

/// Assert one emitted `SKILL.md` is a loadable, self-contained skill for directory `dir`.
fn assert_loadable_skill(skill_md: &str, dir: &str) {
    // --- frontmatter fence ---
    assert!(
        skill_md.starts_with("---\n"),
        "[{dir}] SKILL.md must open with a YAML frontmatter fence; got:\n{skill_md}"
    );
    let after_open = &skill_md[4..];
    let close = after_open
        .find("\n---\n")
        .unwrap_or_else(|| panic!("[{dir}] frontmatter has no closing fence:\n{skill_md}"));
    let yaml = &after_open[..close];
    let body = &after_open[close + 5..];

    // --- frontmatter is valid YAML and a mapping ---
    let fm: BTreeMap<String, Value> = serde_norway::from_str(yaml)
        .unwrap_or_else(|e| panic!("[{dir}] frontmatter is not valid YAML: {e}\n{yaml}"));

    // --- name ---
    let name = fm
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("[{dir}] frontmatter missing string `name`:\n{yaml}"));
    assert_valid_name(name, dir);

    // --- description ---
    let desc = fm
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("[{dir}] frontmatter missing string `description`:\n{yaml}"));
    assert!(
        !desc.trim().is_empty(),
        "[{dir}] description must be non-empty"
    );
    assert!(
        desc.chars().count() <= 1024,
        "[{dir}] description must be ≤1024 chars"
    );

    // --- no authoring-layer fields leak into the agent artifact ---
    for leaked in ["id", "referenceMode", "appliesTo", "imports"] {
        assert!(
            !fm.contains_key(leaked),
            "[{dir}] authoring field `{leaked}` must not appear in emitted frontmatter:\n{yaml}"
        );
    }

    // --- self-contained body: no unresolved @include directives ---
    for line in body.lines() {
        let t = line.trim();
        assert!(
            !(t == "@include" || t.starts_with("@include ")),
            "[{dir}] unresolved `@include` survived into the artifact: {line:?}"
        );
    }

    // --- self-contained body: no unrendered {{Alias}} markers (ignoring escaped \{{) ---
    let unescaped = body.replace("\\{{", "");
    assert!(
        !unescaped.contains("{{"),
        "[{dir}] unrendered `{{{{Alias}}}}` marker survived into the artifact:\n{body}"
    );

    // --- body must carry real content (a skill with no instructions is useless) ---
    assert!(
        !body.trim().is_empty(),
        "[{dir}] SKILL.md body is empty after frontmatter"
    );
}

/// Read every emitted `skills/<dir>/SKILL.md` under an artifact dir as `(dir, contents)`.
fn emitted_skills(artifact_dir: &Path) -> Vec<(String, String)> {
    let skills_root = artifact_dir.join("skills");
    let mut out = Vec::new();
    let mut entries: Vec<PathBuf> = fs::read_dir(&skills_root)
        .unwrap_or_else(|e| panic!("read {}: {e}", skills_root.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    entries.sort();
    for dir in entries {
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let md = fs::read_to_string(dir.join("SKILL.md"))
            .unwrap_or_else(|e| panic!("read {}/SKILL.md: {e}", dir.display()));
        out.push((name, md));
    }
    out
}

/// Build a catalog that lives outside `tests/fixtures/` (e.g. the shipped `examples/`).
fn build_external(catalog: PathBuf, target: &str, agent: &str, tag: &str) -> BuildReport {
    let opts = BuildOptions {
        catalog,
        config: None,
        out: common::out_dir(tag),
        target: target.to_string(),
        agent: agent.to_string(),
        locked: false,
        frozen: false,
    };
    run_build(&opts).unwrap_or_else(|e| panic!("build {tag} failed: {e:?}"))
}

/// Every skill emitted by a real (non-error) build must be loadable + self-contained.
#[test]
fn happy_builds_emit_loadable_self_contained_skills() {
    let cases = [
        ("us1-reuse", "admin"),
        ("us2-markers-deps", "scm"),
        ("us3-entry-pull", "scm"),
        ("us3-entry-pull", "admin"),
    ];
    for (fixture, target) in cases {
        let r = common::build(fixture, target, "claude")
            .unwrap_or_else(|e| panic!("{fixture}/{target} should build: {e:?}"));
        let skills = emitted_skills(&r.artifact_dir);
        assert!(
            !skills.is_empty(),
            "{fixture}/{target} produced no skills to validate"
        );
        for (dir, md) in skills {
            assert_loadable_skill(&md, &dir);
        }
    }
}

/// `@include` content reuse — the framework's defining feature — must be fully inlined:
/// the block's text is present and the directive itself is gone.
#[test]
fn include_blocks_are_fully_inlined() {
    let r = common::build("us1-reuse", "admin", "claude").expect("build");
    let skills = emitted_skills(&r.artifact_dir);
    let (_, fc) = skills
        .iter()
        .find(|(d, _)| d == "frontend-coding")
        .expect("frontend-coding emitted");
    assert!(
        !fc.contains("@include"),
        "the `@include pr-rules` directive must be inlined away:\n{fc}"
    );
    assert!(
        fc.contains("## PR Rules") && fc.contains("Keep PRs small."),
        "the included block's content must appear inline:\n{fc}"
    );
}

/// `{{Alias}}` markers must resolve to a concrete pointer (the dep's id), never survive raw.
#[test]
fn markers_are_resolved_to_pointers() {
    let r = common::build("us2-markers-deps", "scm", "claude").expect("build");
    let skills = emitted_skills(&r.artifact_dir);
    let (_, reviewer) = skills
        .iter()
        .find(|(d, _)| d == "reviewer")
        .expect("reviewer emitted");
    assert!(
        !reviewer.contains("{{"),
        "no raw `{{{{Alias}}}}` may remain:\n{reviewer}"
    );
    assert!(
        reviewer.contains("code-review"),
        "the marker must render to the resolved dep pointer `code-review`:\n{reviewer}"
    );
}

/// Real-world shape: a skill authored with a human-readable CJK title and CJK description
/// (as people actually write skills) must still compile to a loadable artifact — the spec
/// `name` becomes the slug `id`, the CJK title does NOT leak into `name`, and it survives as
/// the body heading. Exercises the shipped `examples/team` catalog end-to-end.
#[test]
fn real_world_cjk_skill_compiles_to_loadable_artifact() {
    let catalog = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
        .join("team");

    for (target, dir, want_desc) in [
        ("web", "frontend-coding", "React/TS 的撰寫與 review 慣例。"),
        ("api", "backend-coding", "NestJS 的撰寫慣例。"),
    ] {
        let r = build_external(
            catalog.clone(),
            target,
            "claude",
            &format!("example-{target}"),
        );
        let skills = emitted_skills(&r.artifact_dir);
        let (d, md) = skills
            .iter()
            .find(|(d, _)| d == dir)
            .unwrap_or_else(|| panic!("{target}: expected skill `{dir}`"));

        // Loadable + self-contained, same bar as every other build.
        assert_loadable_skill(md, d);

        // The CJK human title must NOT have been used as the spec name…
        assert!(
            !md.contains("name: 前端開發規範") && !md.contains("name: 後端開發規範"),
            "{target}: CJK human title leaked into the spec `name`:\n{md}"
        );
        // …but the CJK description (legal: no charset limit, only length) must be carried…
        assert!(
            md.contains(&format!("description: {want_desc}")),
            "{target}: CJK description must be emitted verbatim:\n{md}"
        );
        // …and the human title survives as the body heading.
        assert!(
            md.contains("# 前端開發規範") || md.contains("# 後端開發規範"),
            "{target}: human title should survive as the body H1:\n{md}"
        );
    }
}
