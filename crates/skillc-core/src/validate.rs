//! Validate stage (T037/T038/T039).
//!
//! Catalog-wide conformance: schema rules (via [`schema`]), stray reference files
//! (FR-018, a hard fail per SC-005), and the shared-reference-vs-target-name collision
//! warning (Edge Cases). The per-target "missing required reference" check (FR-017) is
//! membership-dependent and runs after shake ([`check_per_target_references`]).

use crate::config::FrameworkConfig;
use crate::diagnostics::{Code, Diagnostics};
use crate::model::{Catalog, ReferenceKind, ReferenceMode};
use crate::schema;
use crate::shake::Membership;

/// Run catalog-wide validation (schema, stray refs, collisions).
pub fn validate(catalog: &Catalog, cfg: &FrameworkConfig, diags: &mut Diagnostics) {
    for skill in catalog.skills.values() {
        schema::check_skill(skill, diags);

        // Stray reference files cannot hide (FR-018; SC-005 → hard fail).
        for r in &skill.references {
            if cfg.classify_reference(&r.stem) == ReferenceKind::Stray {
                diags.error(
                    Code::ReferenceStray,
                    format!(
                        "skill `{}` has stray reference `references/{}.md` — stem is neither a \
                         registered target nor a declared shared reference",
                        skill.id, r.stem
                    ),
                );
            }
        }
    }

    // A declared shared reference whose stem is also a target name collides (Edge Cases):
    // it stops being shared once the target is registered.
    for stem in cfg.shared_references() {
        if cfg.is_target(stem) {
            diags.warning(
                Code::ReferenceSharedCollision,
                format!(
                    "shared reference `{stem}` collides with a registered target name (it is \
                     treated as per-target, not shared)"
                ),
            );
        }
    }
}

/// FR-017: a `referenceMode: per-target` skill that is a member of `target`'s bundle must
/// have `references/<target>.md`. The assertion fires only for applicable (member) skills —
/// a per-target skill not in this target's bundle is not asserted against.
pub fn check_per_target_references(
    catalog: &Catalog,
    membership: &Membership,
    target: &str,
    diags: &mut Diagnostics,
) {
    for id in &membership.members {
        let Some(skill) = catalog.skills.get(id) else {
            continue;
        };
        if skill.reference_mode != ReferenceMode::PerTarget {
            continue;
        }
        let has_target_ref = skill.references.iter().any(|r| r.stem == target);
        if !has_target_ref {
            diags.error(
                Code::ReferenceMissing,
                format!(
                    "skill `{}` (referenceMode per-target) is missing required \
                     `references/{target}.md` for target `{target}`",
                    skill.id
                ),
            );
        }
    }
}
