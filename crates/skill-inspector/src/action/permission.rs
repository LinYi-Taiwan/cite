//! Plugin per-repo disable: write `permissions.deny += "Skill(<plugin>:<id>)"` into the
//! CURRENT repo's `.claude/settings.local.json` (preserving every other key). Plugin skills are
//! not affected by `skillOverrides` (docs/en/skills §"Override skill visibility from settings"),
//! so a permission deny rule is the per-repo, file-only, reversible mechanism — it never moves
//! or deletes the skill's files, and it only ever touches the one folder it is handed.
//!
//! Unlike the `skillOverrides` path there is nothing to remember for undo (a deny rule is binary:
//! present or absent), and `scan` derives the toggle state straight from the file — so this layer
//! keeps no `InspectorState` record. `enable` simply removes the rule again.

use std::path::Path;

use crate::settings;

/// Add the `Skill(<name>)` deny rule for this plugin skill in `folder`. Idempotent.
pub fn disable(folder: &Path, name: &str) -> std::io::Result<bool> {
    settings::add_skill_deny(folder, name)
}

/// Remove the `Skill(<name>)` deny rule for this plugin skill in `folder`. Idempotent.
pub fn enable(folder: &Path, name: &str) -> std::io::Result<bool> {
    settings::remove_skill_deny(folder, name)
}
