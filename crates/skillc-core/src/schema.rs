//! Skill schema conformance rules (T036).
//!
//! Required frontmatter fields, `referenceMode` enum (validated at parse), and import-form
//! well-formedness. Any violation is a hard failure naming the skill + rule (FR-016).

use crate::diagnostics::{Code, Diagnostics};
use crate::model::{is_safe_segment, is_valid_skill_name, Skill};

/// Validate one skill's frontmatter + import forms. Emits `error[schema/invalid]` per
/// violation (does not short-circuit, so all are reported).
pub fn check_skill(skill: &Skill, diags: &mut Diagnostics) {
    if skill.id.trim().is_empty() {
        diags.error(
            Code::SchemaInvalid,
            format!(
                "skill at {} is missing required `id`",
                skill.source_path.display()
            ),
        );
    } else if !is_safe_segment(&skill.id) {
        // The id becomes a directory name in the artifact (skills/<id>/); a traversal id
        // would escape the output tree on emit. See the security review.
        diags.error(
            Code::SchemaInvalid,
            format!("skill id `{}` is not a safe path segment", skill.id),
        );
    } else if !is_valid_skill_name(&skill.id) {
        // The id is emitted verbatim as the `SKILL.md` `name` (and the skill directory). The
        // agent loader (Agent Skills spec) requires that field to be a 1–64 char lowercase
        // letters/digits/hyphens slug; a non-slug id would compile into an UNLOADABLE skill,
        // so reject it here (compiler-as-validator, FR-016).
        diags.error(
            Code::SchemaInvalid,
            format!(
                "skill id `{}` is not a valid skill name: use 1–64 chars of lowercase \
                 letters, digits, and hyphens (no leading/trailing or consecutive hyphen) — \
                 the id becomes the SKILL.md `name`, which the agent loader validates",
                skill.id
            ),
        );
    }
    if skill.name.trim().is_empty() {
        diags.error(
            Code::SchemaInvalid,
            format!(
                "skill `{}` is missing required frontmatter field `name`",
                skill.id
            ),
        );
    }
    if skill.description.trim().is_empty() {
        diags.error(
            Code::SchemaInvalid,
            format!(
                "skill `{}` is missing required frontmatter field `description`",
                skill.id
            ),
        );
    }
    for alias in skill.imports.keys() {
        if !is_ident(alias) {
            diags.error(
                Code::SchemaInvalid,
                format!(
                    "skill `{}` import alias `{alias}` is not a valid `{{{{}}}}` identifier",
                    skill.id
                ),
            );
        }
    }
}

/// A valid `{{}}` identifier: non-empty, ASCII alphanumeric plus `_` and `-`.
fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
