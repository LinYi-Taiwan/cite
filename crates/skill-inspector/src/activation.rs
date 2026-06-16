//! Activation computation (FR-016): purely from on-disk state, no vendor dependency.
//!
//! - `eligible`: the skill sits in an active root applicable to this context (triggerable).
//!   A Tier-2 quarantined skill is moved out of its root ⇒ not eligible. A Tier-1
//!   folder-disabled skill is still present in its root ⇒ eligible, just not active.
//! - `active`: currently active here — false if quarantined-global OR set `off` via this
//!   folder's `skillOverrides`.

use crate::model::{ActivationState, Skill, SkillState};
use crate::scan::Inventory;

pub fn compute(inventory: &Inventory, context: &str) -> Vec<ActivationState> {
    let mut out: Vec<ActivationState> = inventory
        .skills
        .iter()
        .map(|s| ActivationState {
            skill_key: s.key(),
            context: context.to_string(),
            eligible: eligible(s),
            active: matches!(s.state, SkillState::Active),
        })
        .collect();
    out.sort_by(|a, b| a.skill_key.cmp(&b.skill_key));
    out
}

fn eligible(skill: &Skill) -> bool {
    // Present in an active root (active or folder-disabled) ⇒ eligible; quarantined OR owned by
    // a globally-disabled plugin ⇒ not (the latter is on disk but cannot trigger anywhere).
    !matches!(
        skill.state,
        SkillState::DisabledGlobal | SkillState::DisabledPlugin
    )
}
