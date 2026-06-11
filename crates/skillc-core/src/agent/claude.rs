//! Claude agent formatter (T043).
//!
//! Claude Code loads skills from `skills/<id>/SKILL.md` (with a sibling `references/`),
//! which is exactly the canonical artifact layout — so the Claude formatter uses the
//! trait defaults.

use super::AgentFormatter;

pub struct ClaudeFormatter;

impl AgentFormatter for ClaudeFormatter {
    fn name(&self) -> &str {
        "claude"
    }
}
