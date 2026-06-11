//! Shake stage (T029/T030/T033): per-target membership by entry-point pull.
//!
//! A target's bundle members are the closure of its `entry.mounts` (plus any
//! `appliesTo: all` skills, FR-014c) over import edges. A non-existent mount is a hard
//! error (FR-014a); a catalog skill reached by no entry and no import anywhere is an
//! orphan warning (FR-014b); an empty entry yields an empty-but-valid bundle (Edge Cases).

use std::collections::{BTreeMap, BTreeSet};

use crate::config::FrameworkConfig;
use crate::diagnostics::{Code, Diagnostics};
use crate::model::Catalog;
use crate::resolve::Resolution;

/// The set of units that make up one target's bundle.
#[derive(Debug, Clone, Default)]
pub struct Membership {
    /// Catalog skills in the bundle (mounted or imported).
    pub members: BTreeSet<String>,
    /// External (pulled) skill ids imported by some member.
    pub pulled: BTreeSet<String>,
    /// Per-member `reason` for the manifest: `mounted` or `pulled-by:<skill>`.
    pub reasons: BTreeMap<String, String>,
}

/// Compute membership for `target_name`. Emits `entry/missing` (hard), `entry/empty` and
/// `skill/orphan` (warnings).
pub fn shake(
    catalog: &Catalog,
    cfg: &FrameworkConfig,
    resolution: &Resolution,
    target_name: &str,
    diags: &mut Diagnostics,
) -> Membership {
    let target = cfg.target(target_name).expect("target validated upstream");

    // Entry validation (FR-014a): every mount must be a catalog skill.
    for mount in &target.mounts {
        if !catalog.skills.contains_key(mount) {
            diags.error(
                Code::EntryMissing,
                format!("target `{target_name}` entry mounts skill `{mount}` which does not exist"),
            );
        }
    }

    let auto: Vec<String> = catalog
        .skills
        .values()
        .filter(|s| s.applies_to_all)
        .map(|s| s.id.clone())
        .collect();

    if target.mounts.is_empty() && auto.is_empty() {
        diags.warning(
            Code::EntryEmpty,
            format!("target `{target_name}` has an empty entry (empty-but-valid bundle)"),
        );
    }

    // Roots = existing mounts + appliesTo:all skills.
    let mut roots: Vec<String> = target
        .mounts
        .iter()
        .filter(|m| catalog.skills.contains_key(*m))
        .cloned()
        .collect();
    roots.extend(auto.iter().cloned());

    let members = closure(catalog, resolution, roots.clone());

    // Pulled external deps imported by any member, plus everything THEY pull in turn —
    // the transitive closure over `pulled_deps` (FR-008: pull it and its dependencies).
    let mut pulled = BTreeSet::new();
    let mut stack: Vec<String> = resolution
        .marker_targets
        .iter()
        .filter(|((sk, _), tid)| members.contains(sk) && resolution.pulled.contains_key(*tid))
        .map(|(_, tid)| tid.clone())
        .collect();
    while let Some(id) = stack.pop() {
        if pulled.insert(id.clone()) {
            if let Some(deps) = resolution.pulled_deps.get(&id) {
                stack.extend(deps.iter().cloned());
            }
        }
    }

    // Per-member reasons.
    let mounted: BTreeSet<&String> = target.mounts.iter().chain(auto.iter()).collect();
    let mut reasons = BTreeMap::new();
    for member in &members {
        let reason = if mounted.contains(member) {
            "mounted".to_string()
        } else {
            // Reached via import — name the first member that imports it.
            importer_of(resolution, &members, member)
                .map(|imp| format!("pulled-by:{imp}"))
                .unwrap_or_else(|| "mounted".to_string())
        };
        reasons.insert(member.clone(), reason);
    }

    emit_orphans(catalog, cfg, resolution, diags);

    Membership {
        members,
        pulled,
        reasons,
    }
}

/// Transitive closure of `roots` over local import edges (catalog skill → catalog skill).
fn closure(catalog: &Catalog, resolution: &Resolution, roots: Vec<String>) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut stack = roots;
    while let Some(id) = stack.pop() {
        if !catalog.skills.contains_key(&id) {
            continue;
        }
        if seen.insert(id.clone()) {
            for ((sk, _alias), tid) in &resolution.marker_targets {
                if *sk == id && catalog.skills.contains_key(tid) {
                    stack.push(tid.clone());
                }
            }
        }
    }
    seen
}

/// The first member that imports `target` (deterministic via `BTreeMap` ordering).
fn importer_of(
    resolution: &Resolution,
    members: &BTreeSet<String>,
    target: &str,
) -> Option<String> {
    resolution
        .marker_targets
        .iter()
        .find(|((sk, _), tid)| tid.as_str() == target && members.contains(sk))
        .map(|((sk, _), _)| sk.clone())
}

/// Warn on catalog skills reachable from no entry (any target) and no import (FR-014b).
fn emit_orphans(
    catalog: &Catalog,
    cfg: &FrameworkConfig,
    resolution: &Resolution,
    diags: &mut Diagnostics,
) {
    let mut roots: Vec<String> = Vec::new();
    for t in cfg.targets() {
        roots.extend(
            t.mounts
                .iter()
                .filter(|m| catalog.skills.contains_key(*m))
                .cloned(),
        );
    }
    roots.extend(
        catalog
            .skills
            .values()
            .filter(|s| s.applies_to_all)
            .map(|s| s.id.clone()),
    );
    let reachable = closure(catalog, resolution, roots);
    for id in catalog.skills.keys() {
        if !reachable.contains(id) {
            diags.warning(
                Code::SkillOrphan,
                format!("skill `{id}` is reached by no entry or import"),
            );
        }
    }
}
