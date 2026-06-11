//! Install stage (T045): place a built artifact into an agent destination (separate step,
//! FR-023). Idempotent — re-installing identical artifact bytes yields an identical
//! destination (FR-025).
//!
//! **Pruning**: install records what it placed in a receipt (`.skillc-receipt.json` at the
//! destination root, keyed by `<target>/<agent>`). The next install for the same key
//! removes skills that the previous install placed but the new artifact no longer carries
//! — so unmounting a skill in the catalog actually retires it from the agent, instead of
//! silently lingering (the drift the framework exists to prevent). Skills the receipt has
//! never recorded (hand-authored, other tools) are never touched; a skill still owned by
//! another `<target>/<agent>` install into the same destination is kept.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::agent;
use crate::diagnostics::{Code, Diagnostic};
use crate::model::{is_safe_segment, is_valid_skill_name};

#[derive(Deserialize)]
struct InstallManifest {
    target: String,
    agent: String,
    skills: Vec<InstallSkill>,
}

#[derive(Deserialize)]
struct InstallSkill {
    id: String,
    #[serde(default)]
    references: Vec<String>,
}

pub const RECEIPT_FILENAME: &str = ".skillc-receipt.json";

/// What skillc has installed into a destination, keyed by `<target>/<agent>` so multiple
/// targets can share one destination without pruning each other's skills.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Receipt {
    #[serde(default)]
    installs: BTreeMap<String, Vec<String>>,
}

impl Receipt {
    fn read(dest: &Path) -> Result<Self, Diagnostic> {
        let path = dest.join(RECEIPT_FILENAME);
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| {
                Diagnostic::error(
                    Code::ConfigInvalid,
                    format!("malformed {}: {e}", path.display()),
                )
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(Diagnostic::error(
                Code::ConfigInvalid,
                format!("cannot read {}: {e}", path.display()),
            )),
        }
    }

    fn write(&self, dest: &Path) -> Result<(), Diagnostic> {
        let json = serde_json::to_string_pretty(self).expect("receipt serializes");
        write_to(&dest.join(RECEIPT_FILENAME), &format!("{json}\n"))
    }

    /// Skill ids owned by any install key other than `key`.
    fn owned_elsewhere(&self, key: &str) -> BTreeSet<&str> {
        self.installs
            .iter()
            .filter(|(k, _)| k.as_str() != key)
            .flat_map(|(_, ids)| ids.iter().map(String::as_str))
            .collect()
    }
}

/// Place the artifact at `artifact` into `dest` using the recorded agent's formatter (or
/// `agent_override`).
pub fn install(
    artifact: &Path,
    dest: &Path,
    agent_override: Option<&str>,
) -> Result<(), Diagnostic> {
    let manifest_text = read(&artifact.join("manifest.json"))?;
    let manifest: InstallManifest = serde_json::from_str(&manifest_text).map_err(|e| {
        Diagnostic::error(
            Code::ConfigInvalid,
            format!("artifact manifest is unreadable: {e}"),
        )
    })?;

    let agent = agent_override.unwrap_or(&manifest.agent);
    let fmt = agent::formatter_for(agent);

    // `target`/`agent` come from the (untrusted) artifact manifest and become the receipt
    // key. A slash-bearing target (`"a/b"`) would let a crafted artifact forge another
    // install's key and poison its prune list — reject anything that isn't a single safe
    // path segment, the same rule build enforces for these names.
    for (label, value) in [("target", manifest.target.as_str()), ("agent", agent)] {
        if !is_safe_segment(value) {
            return Err(Diagnostic::error(
                Code::ConfigInvalid,
                format!("artifact manifest {label} `{value}` is not a safe name"),
            ));
        }
    }

    let mut receipt = Receipt::read(dest)?;
    let install_key = format!("{}/{}", manifest.target, agent);

    for skill in &manifest.skills {
        // The artifact may come from an untrusted source (install is decoupled from build),
        // so re-validate every id/stem before joining it into a `dest` path — otherwise a
        // crafted manifest (`id: ../../.ssh/...`) writes outside `dest`. See the security
        // review (zip-slip-style).
        if !is_safe_segment(&skill.id) {
            return Err(Diagnostic::error(
                Code::ConfigInvalid,
                format!(
                    "artifact manifest skill id `{}` is not a safe path segment",
                    skill.id
                ),
            ));
        }
        // Install is decoupled from build, so a hand-crafted artifact could carry a
        // path-safe-but-non-slug id that the build gate would have rejected. Re-apply the
        // same name rule here so install never places an unloadable skill.
        if !is_valid_skill_name(&skill.id) {
            return Err(Diagnostic::error(
                Code::ConfigInvalid,
                format!(
                    "artifact manifest skill id `{}` is not a valid skill name (must be a \
                     1–64 char lowercase letters/digits/hyphens slug)",
                    skill.id
                ),
            ));
        }

        let src = artifact.join("skills").join(&skill.id).join("SKILL.md");
        let content = read(&src)?;
        write_to(&dest.join(fmt.skill_file(&skill.id)), &content)?;

        for stem in &skill.references {
            if !is_safe_segment(stem) {
                return Err(Diagnostic::error(
                    Code::ConfigInvalid,
                    format!("artifact manifest reference stem `{stem}` is not a safe path segment"),
                ));
            }
            let src_ref = artifact
                .join("skills")
                .join(&skill.id)
                .join("references")
                .join(format!("{stem}.md"));
            let ref_content = read(&src_ref)?;
            write_to(
                &dest.join(fmt.reference_file(&skill.id, stem)),
                &ref_content,
            )?;
        }
    }

    // Prune: skills this (target, agent) installed before but the new artifact dropped.
    // Only ever removes ids the receipt recorded as ours — never hand-authored skills —
    // and keeps ids still owned by another install into the same destination.
    let now: BTreeSet<String> = manifest.skills.iter().map(|s| s.id.clone()).collect();
    let before: Vec<String> = receipt
        .installs
        .get(&install_key)
        .cloned()
        .unwrap_or_default();
    let kept_by_others = receipt.owned_elsewhere(&install_key);
    for stale in before.iter().filter(|id| !now.contains(*id)) {
        if kept_by_others.contains(stale.as_str()) {
            continue;
        }
        // Receipt content is on-disk state: re-validate before joining into a removal path
        // (a tampered receipt must not become `rm -rf` outside the skills tree).
        if !is_safe_segment(stale) {
            continue;
        }
        let skill_md = dest.join(fmt.skill_file(stale));
        let Some(skill_dir) = skill_md.parent() else {
            continue;
        };
        // Trip-wire before an irreversible remove_dir_all: the computed dir must root
        // under `dest`. Holds for the shipped formatters by construction; a future
        // formatter returning an absolute/`..` path must not silently escape.
        if !skill_dir.starts_with(dest) {
            return Err(Diagnostic::error(
                Code::ConfigInvalid,
                format!(
                    "refusing to prune `{stale}`: computed path {} escapes the destination",
                    skill_dir.display()
                ),
            ));
        }
        if skill_dir.is_dir() {
            std::fs::remove_dir_all(skill_dir).map_err(|e| {
                Diagnostic::error(
                    Code::ConfigInvalid,
                    format!("cannot remove stale skill `{stale}`: {e}"),
                )
            })?;
            eprintln!("cite: removed stale skill `{stale}` (no longer in {install_key})");
        }
    }

    receipt
        .installs
        .insert(install_key, now.into_iter().collect());
    receipt.write(dest)?;

    Ok(())
}

fn read(p: &Path) -> Result<String, Diagnostic> {
    std::fs::read_to_string(p).map_err(|e| {
        Diagnostic::error(
            Code::ConfigInvalid,
            format!("cannot read {}: {e}", p.display()),
        )
    })
}

fn write_to(p: &Path, contents: &str) -> Result<(), Diagnostic> {
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            Diagnostic::error(
                Code::ConfigInvalid,
                format!("mkdir {}: {e}", parent.display()),
            )
        })?;
    }
    std::fs::write(p, contents)
        .map_err(|e| Diagnostic::error(Code::ConfigInvalid, format!("write {}: {e}", p.display())))
}
