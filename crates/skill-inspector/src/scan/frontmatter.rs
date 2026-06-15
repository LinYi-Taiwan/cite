//! Minimal YAML frontmatter parsing for `SKILL.md`, vendored from the (now-removed)
//! `skillc-core` compiler. The inspector only needs `name` + `description`; parse errors
//! are surfaced as a plain `String` (callers only branch on Ok/Err and flag the skill
//! `metadata_complete=false` — they never read the message).
//!
//! Unknown frontmatter fields are tolerated (not `deny_unknown_fields`): a `SKILL.md`
//! authored for the compiler (with `imports`, `referenceMode`, …) must still parse here.

use std::path::Path;

use serde::Deserialize;

/// The subset of frontmatter the inspector reads. Extra authored fields are ignored.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawFrontmatter {
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Parsed parts of a `SKILL.md`: the typed frontmatter (body is not retained — unused here).
pub struct Parsed {
    pub frontmatter: RawFrontmatter,
}

/// Split `text` into its `---`-fenced YAML frontmatter and deserialize it. A file with no
/// frontmatter fence yields an empty `RawFrontmatter`. Returns `Err` only when a present
/// YAML block is malformed.
pub fn parse(text: &str, source: &Path) -> Result<Parsed, String> {
    let yaml = split(text);
    let frontmatter = match yaml {
        Some(y) => serde_norway::from_str::<RawFrontmatter>(y)
            .map_err(|e| format!("{}: malformed frontmatter: {e}", source.display()))?,
        None => RawFrontmatter::default(),
    };
    Ok(Parsed { frontmatter })
}

/// Returns `Some(yaml)` when the text opens with a `---` fence and has a closing `---` line;
/// otherwise `None` (no frontmatter).
fn split(text: &str) -> Option<&str> {
    // Tolerate a leading BOM; the open fence must be the first line.
    let stripped = text.strip_prefix('\u{feff}').unwrap_or(text);
    let after_open = stripped
        .strip_prefix("---\n")
        .or_else(|| stripped.strip_prefix("---\r\n"))?;
    // Find the closing fence: a line that is exactly `---`.
    let fence_at = find_fence_line(after_open)?;
    Some(&after_open[..fence_at])
}

/// Find the byte offset (within `s`) of a line consisting solely of `---`.
fn find_fence_line(s: &str) -> Option<usize> {
    let mut offset = 0usize;
    for line in s.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p() -> PathBuf {
        PathBuf::from("SKILL.md")
    }

    #[test]
    fn parses_frontmatter() {
        let text = "---\nid: foo\nname: Foo\ndescription: a foo\n---\nBody here\n";
        let parsed = parse(text, &p()).unwrap();
        assert_eq!(parsed.frontmatter.name.as_deref(), Some("Foo"));
        assert_eq!(parsed.frontmatter.description.as_deref(), Some("a foo"));
    }

    #[test]
    fn no_frontmatter_is_empty() {
        let parsed = parse("Just a body, no fence\n", &p()).unwrap();
        assert!(parsed.frontmatter.name.is_none());
        assert!(parsed.frontmatter.description.is_none());
    }

    #[test]
    fn unknown_fields_tolerated() {
        let text =
            "---\nid: s\nname: S\ndescription: d\nimports:\n  CodeReview: ../code-review\n---\nx";
        let parsed = parse(text, &p()).unwrap();
        assert_eq!(parsed.frontmatter.name.as_deref(), Some("S"));
    }

    #[test]
    fn malformed_yaml_errors() {
        let text = "---\nid: [unclosed\n---\nbody";
        assert!(parse(text, &p()).is_err());
    }
}
