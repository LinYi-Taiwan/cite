//! Context-load assembly (FR-017/018, SC-007). Splits the honest boundary: where an agent
//! exposes a recognizable record we report it `present`; where it exposes nothing we report
//! exactly one `unavailable` entry with empty keys — never fabricated.

pub mod claude_transcript;

use std::path::Path;

use crate::model::{ContextAvailability, ContextLoadRecord};
use crate::scan::claude::ClaudeProvider;

/// One context-load view per requested agent.
pub fn collect(agents: &[String], home: &Path) -> Vec<ContextLoadRecord> {
    let mut out = Vec::new();
    for agent in agents {
        let records = match agent.as_str() {
            ClaudeProvider::AGENT => claude_transcript::read(home),
            // Other agents expose no transcript the tool can read.
            _ => Vec::new(),
        };
        if records.is_empty() {
            out.push(ContextLoadRecord {
                turn_ref: format!("{agent}:unavailable"),
                availability: ContextAvailability::Unavailable,
                loaded_skill_keys: Vec::new(),
            });
        } else {
            out.extend(records);
        }
    }
    out.sort_by(|a, b| a.turn_ref.cmp(&b.turn_ref));
    out
}
