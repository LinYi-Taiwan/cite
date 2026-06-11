//! Emit stage (T041/T042): write the deterministic, inspectable artifact tree.
//!
//! Layout (artifact-layout.md):
//! ```text
//! <out>/<target>/<agent>/
//! ├── manifest.json
//! └── skills/<id>/SKILL.md
//!                 references/<stem>.md
//! ```
//! Determinism (FR-022): sorted keys (`BTreeMap`), sorted skills/references/warnings, no
//! timestamps / wall-clock / filesystem-order in the bytes.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::assemble::Bundle;
use crate::diagnostics::{Code, Diagnostic};

#[derive(Serialize)]
struct Manifest {
    target: String,
    agent: String,
    skills: Vec<ManifestSkill>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct ManifestSkill {
    id: String,
    reason: String,
    includes: Vec<String>,
    imports: BTreeMap<String, String>,
    references: Vec<String>,
}

/// Write `bundle` for `agent` into `artifact_dir`, with `warnings` recorded in the
/// manifest. The directory is recreated from scratch so stale files cannot linger.
pub fn emit(
    bundle: &Bundle,
    agent: &str,
    warnings: &[String],
    artifact_dir: &Path,
) -> Result<(), Diagnostic> {
    // Fresh tree (idempotent re-emit; no leftovers affecting determinism).
    let _ = std::fs::remove_dir_all(artifact_dir);
    mkdir(artifact_dir)?;

    let skills_root = artifact_dir.join("skills");
    mkdir(&skills_root)?;

    // The agent formatter re-renders the frontmatter the parser stripped, so the artifact is
    // a conformant, loadable skill (not just a bare body).
    let formatter = crate::agent::formatter_for(agent);

    for skill in &bundle.skills {
        let skill_dir = skills_root.join(&skill.id);
        mkdir(&skill_dir)?;
        // The Agent Skills spec requires `name` to be the slug matching the skill directory
        // (lowercase/hyphen, == dir name), so we pass the `id`, not the human title.
        let skill_md = formatter.render_skill_md(&skill.id, &skill.description, &skill.content);
        write_file(&skill_dir.join("SKILL.md"), &skill_md)?;

        if !skill.references.is_empty() {
            let refs_dir = skill_dir.join("references");
            mkdir(&refs_dir)?;
            // Sorted reference order for determinism.
            let mut refs: Vec<_> = skill.references.iter().collect();
            refs.sort_by(|a, b| a.stem.cmp(&b.stem));
            for r in refs {
                write_file(&refs_dir.join(format!("{}.md", r.stem)), &r.content)?;
            }
        }
    }

    let manifest = build_manifest(bundle, agent, warnings);
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| Diagnostic::error(Code::ConfigInvalid, format!("manifest serialize: {e}")))?;
    write_file(&artifact_dir.join("manifest.json"), &format!("{json}\n"))?;

    Ok(())
}

fn build_manifest(bundle: &Bundle, agent: &str, warnings: &[String]) -> Manifest {
    let skills = bundle
        .skills
        .iter()
        .map(|s| {
            let mut references: Vec<String> = s.references.iter().map(|r| r.stem.clone()).collect();
            references.sort();
            ManifestSkill {
                id: s.id.clone(),
                reason: s.reason.clone(),
                includes: s.includes.clone(),
                imports: s.imports.clone(),
                references,
            }
        })
        .collect();

    let mut warnings = warnings.to_vec();
    warnings.sort();

    Manifest {
        target: bundle.target.clone(),
        agent: agent.to_string(),
        skills,
        warnings,
    }
}

fn mkdir(p: &Path) -> Result<(), Diagnostic> {
    std::fs::create_dir_all(p)
        .map_err(|e| Diagnostic::error(Code::ConfigInvalid, format!("mkdir {}: {e}", p.display())))
}

fn write_file(p: &Path, contents: &str) -> Result<(), Diagnostic> {
    std::fs::write(p, contents)
        .map_err(|e| Diagnostic::error(Code::ConfigInvalid, format!("write {}: {e}", p.display())))
}
