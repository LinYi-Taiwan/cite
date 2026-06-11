//! Assemble stage (T017): inline `@include` block content into each includer, producing
//! the in-memory [`Bundle`]. Diamond reuse inlines a block's content *per includer* as
//! authored — blocks are content, not deduped nodes (Edge Cases).
//!
//! US2 extends this with dependency landing/dedup (T026); US3 with reference pruning
//! (T032). In this US1 cut every catalog skill is a member with reason `mounted`.

use std::collections::BTreeMap;

use crate::config::FrameworkConfig;
use crate::model::{Catalog, ReferenceKind};
use crate::parse::{include, marker};
use crate::resolve::Resolution;
use crate::shake::Membership;

/// The assembled, self-contained output for one target (pre-emit).
#[derive(Debug, Clone)]
pub struct Bundle {
    pub target: String,
    pub skills: Vec<BundledSkill>,
}

#[derive(Debug, Clone)]
pub struct BundledSkill {
    pub id: String,
    /// Carried for the emitted `SKILL.md` frontmatter so the artifact is a conformant,
    /// loadable skill (Agent Skills `description`). The spec `name` is the slug `id` (above),
    /// not a human title, so only `description` needs carrying here.
    pub description: String,
    /// `"mounted"` or `"pulled-by:<skill-id>"` (SC-008). Refined in US2/US3.
    pub reason: String,
    /// The assembled `SKILL.md` body: `@include`s inlined, `{{Alias}}` resolved (US2).
    pub content: String,
    /// Blocks inlined into this skill (for the manifest).
    pub includes: Vec<String>,
    /// Declared imports, alias → raw spec (for the manifest).
    pub imports: BTreeMap<String, String>,
    pub references: Vec<BundledReference>,
}

#[derive(Debug, Clone)]
pub struct BundledReference {
    pub stem: String,
    pub content: String,
}

/// Assemble the bundle for `target`: member catalog skills (with `@include`s inlined and
/// `{{Alias}}` markers rendered) plus the cross-repo deps they pulled, with per-target
/// reference pruning (FR-015). Membership comes from shake.
pub fn assemble(
    catalog: &Catalog,
    cfg: &FrameworkConfig,
    resolution: &Resolution,
    membership: &Membership,
    target: &str,
) -> Bundle {
    let mut skills: Vec<BundledSkill> = Vec::new();

    // BTreeMap iteration → deterministic skill order (FR-022).
    for skill in catalog.skills.values() {
        if !membership.members.contains(&skill.id) {
            continue;
        }
        let inlined = expand_body(&skill.body, catalog);
        // Render `{{Alias}}` as a followable relative markdown link, not a bare slug: the
        // emitted SKILL.md lives at `skills/<this>/SKILL.md`, so `../<id>/SKILL.md` resolves
        // to the referenced skill in both the artifact and any installed destination. The
        // reading agent gets a path it can open, which is the point of a resolved pointer.
        let resolver = |alias: &str| {
            resolution
                .marker_targets
                .get(&(skill.id.clone(), alias.to_string()))
                .map(|id| format!("[{id}](../{id}/SKILL.md)"))
        };
        let content = marker::render(&inlined, &resolver);
        let imports = skill
            .imports
            .iter()
            .map(|(k, v)| (k.clone(), v.raw.clone()))
            .collect();
        let references = prune_references(skill, cfg, target);
        let reason = membership
            .reasons
            .get(&skill.id)
            .cloned()
            .unwrap_or_else(|| "mounted".to_string());
        skills.push(BundledSkill {
            id: skill.id.clone(),
            description: skill.description.clone(),
            reason,
            content,
            includes: skill.includes.clone(),
            imports,
            references,
        });
    }

    // Pulled cross-repo dependencies land as top-level skills, deduped (FR-008/010),
    // carrying their own references through the same per-target pruning as catalog skills.
    for pulled in resolution.pulled.values() {
        if !membership.pulled.contains(&pulled.id) {
            continue;
        }
        if skills.iter().any(|s| s.id == pulled.id) {
            continue; // a catalog skill of the same id already covers it
        }
        let references = pulled
            .references
            .iter()
            .filter(|r| match cfg.classify_reference(&r.stem) {
                ReferenceKind::PerTarget => r.stem == target,
                ReferenceKind::Shared => true,
                ReferenceKind::Stray => true,
            })
            .map(|r| BundledReference {
                stem: r.stem.clone(),
                content: r.content.clone(),
            })
            .collect();
        skills.push(BundledSkill {
            id: pulled.id.clone(),
            description: pulled.description.clone(),
            reason: format!("pulled-by:{}", pulled.pulled_by),
            content: pulled.content.clone(),
            includes: pulled.includes.clone(),
            imports: pulled.imports.clone(),
            references,
        });
    }

    // Keep deterministic order even after appending pulled skills.
    skills.sort_by(|a, b| a.id.cmp(&b.id));

    Bundle {
        target: target.to_string(),
        skills,
    }
}

/// Retain `references/<target>.md` (this target only) + shared references; drop other
/// targets' reference files (FR-015). Stray references are kept and reported in validate.
fn prune_references(
    skill: &crate::model::Skill,
    cfg: &FrameworkConfig,
    target: &str,
) -> Vec<BundledReference> {
    skill
        .references
        .iter()
        .filter(|r| match cfg.classify_reference(&r.stem) {
            ReferenceKind::PerTarget => r.stem == target,
            ReferenceKind::Shared => true,
            ReferenceKind::Stray => true,
        })
        .map(|r| BundledReference {
            stem: r.stem.clone(),
            content: r.content.clone(),
        })
        .collect()
}

/// Replace each `@include <block>` line with the block's (recursively expanded) content,
/// against `catalog`'s blocks. Public so pulled cross-repo skills can be expanded against
/// their own source-mirror catalog (US2).
pub fn expand_includes(body: &str, catalog: &Catalog) -> String {
    expand_body(body, catalog)
}

/// Replace each `@include <block>` line with the block's (recursively expanded) content.
/// Cycles are rejected upstream (graph stage), so this recursion terminates.
fn expand_body(body: &str, catalog: &Catalog) -> String {
    let mut out = String::new();
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        match include::parse_line(trimmed) {
            Some(id) => {
                if let Some(block) = catalog.blocks.get(id) {
                    let expanded = expand_body(&block.content, catalog);
                    out.push_str(&expanded);
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                }
                // Missing blocks are already a hard error; nothing to inline.
            }
            None => out.push_str(line),
        }
    }
    out
}
