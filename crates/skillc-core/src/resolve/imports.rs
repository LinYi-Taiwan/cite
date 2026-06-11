//! Marker → import resolution checks (T022).
//!
//! Every `{{Alias}}` MUST resolve to a declared import (FR-007); an import declared but
//! never referenced by a marker is an unused-import warning (FR-012). `@include` uses block
//! ids, not aliases, so it does not count as "referencing" an import.

use std::collections::BTreeSet;

use crate::diagnostics::{Code, Diagnostics};
use crate::model::Skill;

/// Validate a skill's markers against its imports. Returns the set of aliases actually
/// referenced by a marker (the caller resolves only those).
///
/// `warn_unused`: emit `import/unused` for declared-but-unreferenced imports. Pass `false`
/// for pulled external skills — the local user cannot edit a third-party skill, so the
/// warning would be unactionable noise that buries real local warnings.
pub fn check_markers(
    skill: &Skill,
    warn_unused: bool,
    diags: &mut Diagnostics,
) -> BTreeSet<String> {
    let mut used = BTreeSet::new();

    for alias in &skill.markers {
        if skill.imports.contains_key(alias) {
            used.insert(alias.clone());
        } else {
            let marker = ["{{", alias, "}}"].concat();
            diags.error(
                Code::MarkerUndefined,
                format!(
                    "skill `{}` references {marker} but no import declares `{alias}`",
                    skill.id
                ),
            );
        }
    }

    if warn_unused {
        for alias in skill.imports.keys() {
            if !used.contains(alias) {
                diags.warning(
                    Code::ImportUnused,
                    format!(
                        "skill `{}` declares import `{alias}` but never references it",
                        skill.id
                    ),
                );
            }
        }
    }

    used
}
