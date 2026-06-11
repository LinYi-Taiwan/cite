//! Per-agent output formatters (T043/T044).
//!
//! The canonical build artifact is agent-neutral-but-agent-tagged (artifact-layout.md); a
//! formatter decides the on-disk shape at the *install* destination. All five registered
//! agents have adopted the Agent Skills standard (`skills/<id>/SKILL.md` with
//! `name`/`description` frontmatter), so the shared default layout is the *verified*
//! convention for each — what differs per agent is only the destination root the user
//! passes as `--dest` (see the README's install table). Verified 2026-06-11 against:
//! Codex <https://developers.openai.com/codex/skills>, Cursor
//! <https://cursor.com/docs/context/skills>, Gemini CLI
//! <https://geminicli.com/docs/cli/skills/>, Copilot custom-instructions docs
//! (`.github/skills/<name>/SKILL.md`). An agent that later diverges plugs a formatter in
//! here without changing the rest of the pipeline.

pub mod claude;

use std::path::PathBuf;

use serde::Serialize;

/// Render a `---`-fenced YAML frontmatter block (`name` + `description`) via `serde_norway`
/// so values are correctly quoted/escaped and the bytes stay deterministic (FR-022). Ends
/// with a blank line separating frontmatter from the body.
fn render_frontmatter(name: &str, description: &str) -> String {
    #[derive(Serialize)]
    struct Frontmatter<'a> {
        name: &'a str,
        description: &'a str,
    }
    // serde_norway emits trailing-`\n` `key: value` lines; field order is preserved.
    // A two-`&str` struct cannot fail to serialize; a panic here means a real bug, not a
    // silently-stripped frontmatter (which would emit a nameless, non-loadable skill).
    let yaml = serde_norway::to_string(&Frontmatter { name, description })
        .expect("frontmatter (two string fields) is always serializable");
    format!("---\n{yaml}---\n\n")
}

/// How an agent lays a bundle out at the install destination.
pub trait AgentFormatter {
    fn name(&self) -> &str;

    /// Render the final `SKILL.md` bytes for this agent: the agent's required frontmatter
    /// prepended to the assembled body. Parsing drops the authoring frontmatter
    /// (`id`/`referenceMode`/`appliesTo`/`imports`); without re-rendering, the emitted file
    /// has no frontmatter and is not a loadable skill. The default emits the cross-agent
    /// minimum — `name` + `description` — which is exactly Claude's / the Agent Skills
    /// contract; an agent with a different on-disk skill format overrides this.
    ///
    /// `name` MUST be the skill's slug `id` (lowercase alphanumeric + hyphens, == the skill
    /// directory name) per the Agent Skills spec — NOT a human-readable title, which would
    /// fail validation. The human title lives on as the body's first heading.
    fn render_skill_md(&self, name: &str, description: &str, body: &str) -> String {
        // `render_frontmatter` already ends with one blank line. Drop exactly the body's own
        // single leading line break (the artifact of the source's post-frontmatter newline)
        // so the separator is one blank line — without swallowing any further author-intended
        // blank lines.
        let body = body
            .strip_prefix("\r\n")
            .or_else(|| body.strip_prefix('\n'))
            .unwrap_or(body);
        format!("{}{body}", render_frontmatter(name, description))
    }

    /// Destination-relative path for a skill's `SKILL.md`.
    fn skill_file(&self, skill_id: &str) -> PathBuf {
        PathBuf::from("skills").join(skill_id).join("SKILL.md")
    }

    /// Destination-relative path for one of a skill's references.
    fn reference_file(&self, skill_id: &str, stem: &str) -> PathBuf {
        PathBuf::from("skills")
            .join(skill_id)
            .join("references")
            .join(format!("{stem}.md"))
    }
}

/// The default layout (mirrors the canonical artifact): `skills/<id>/SKILL.md`.
pub struct DefaultFormatter {
    name: String,
}

impl AgentFormatter for DefaultFormatter {
    fn name(&self) -> &str {
        &self.name
    }
}

/// The agents the framework knows how to format for (FR-024).
pub const KNOWN_AGENTS: &[&str] = &["claude", "codex", "cursor", "gemini", "copilot"];

/// Resolve a formatter by agent name. Unknown agents fall back to the default layout under
/// their own name (selection is validated against the config registry upstream).
pub fn formatter_for(agent: &str) -> Box<dyn AgentFormatter> {
    match agent {
        "claude" => Box::new(claude::ClaudeFormatter),
        other => Box::new(DefaultFormatter {
            name: other.to_string(),
        }),
    }
}
