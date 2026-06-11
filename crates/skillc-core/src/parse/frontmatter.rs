//! YAML frontmatter parsing (T007), via `serde_norway`.
//!
//! Splits the leading `---`-fenced YAML block from the markdown body and deserializes
//! the typed fields (`id`/`name`/`description`/`referenceMode`/`appliesTo`/`imports`).
//! Missing-required-field enforcement is the schema stage's job (T036); here we only
//! surface a parse error if the YAML itself is malformed.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::diagnostics::{Code, Diagnostic};

/// Raw, optional frontmatter as authored. Required-field validation happens later
/// (schema stage) so we can name each missing field rather than failing deserialization.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawFrontmatter {
    pub id: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "referenceMode")]
    pub reference_mode: Option<String>,
    #[serde(rename = "appliesTo")]
    pub applies_to: Option<String>,
    #[serde(default)]
    pub imports: Option<BTreeMap<String, String>>,
}

/// The parsed parts of a `SKILL.md`: typed frontmatter + the body after the closing fence.
pub struct Parsed {
    pub frontmatter: RawFrontmatter,
    pub body: String,
}

/// Split `text` into its `---`-fenced YAML frontmatter and the remaining body, then
/// deserialize the frontmatter. A file with no frontmatter fence yields an empty
/// `RawFrontmatter` and the whole text as body.
pub fn parse(text: &str, source: &Path) -> Result<Parsed, Diagnostic> {
    let (yaml, body) = split(text);
    let frontmatter = match yaml {
        Some(y) => serde_norway::from_str::<RawFrontmatter>(y).map_err(|e| {
            Diagnostic::error(
                Code::SchemaInvalid,
                format!("{}: malformed frontmatter: {e}", source.display()),
            )
        })?,
        None => RawFrontmatter::default(),
    };
    Ok(Parsed {
        frontmatter,
        body: body.to_string(),
    })
}

/// Returns `(Some(yaml), body)` when the text opens with a `---` fence and has a closing
/// `---` line; otherwise `(None, whole_text)`.
fn split(text: &str) -> (Option<&str>, &str) {
    // Tolerate a leading BOM; the open fence must be the first line.
    let stripped = text.strip_prefix('\u{feff}').unwrap_or(text);
    let after_open = match stripped
        .strip_prefix("---\n")
        .or_else(|| stripped.strip_prefix("---\r\n"))
    {
        Some(rest) => rest,
        // Return the BOM-stripped text so a leading BOM never leaks into a body-only file.
        None => return (None, stripped),
    };
    // Find the closing fence: a line that is exactly `---`.
    let Some(fence_at) = find_fence_line(after_open) else {
        return (None, stripped);
    };
    let yaml = &after_open[..fence_at];
    let after_fence = &after_open[fence_at..];
    let body = after_fence
        .strip_prefix("---\n")
        .or_else(|| after_fence.strip_prefix("---\r\n"))
        .unwrap_or("");
    (Some(yaml), body)
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
    fn parses_frontmatter_and_body() {
        let text = "---\nid: foo\nname: Foo\ndescription: a foo\n---\nBody here\n";
        let parsed = parse(text, &p()).unwrap();
        assert_eq!(parsed.frontmatter.id.as_deref(), Some("foo"));
        assert_eq!(parsed.frontmatter.name.as_deref(), Some("Foo"));
        assert_eq!(parsed.body, "Body here\n");
    }

    #[test]
    fn no_frontmatter_is_all_body() {
        let text = "Just a body, no fence\n";
        let parsed = parse(text, &p()).unwrap();
        assert!(parsed.frontmatter.id.is_none());
        assert_eq!(parsed.body, text);
    }

    #[test]
    fn imports_map_parses() {
        let text =
            "---\nid: s\nname: S\ndescription: d\nimports:\n  CodeReview: ../code-review\n---\nx";
        let parsed = parse(text, &p()).unwrap();
        let imports = parsed.frontmatter.imports.unwrap();
        assert_eq!(
            imports.get("CodeReview").map(String::as_str),
            Some("../code-review")
        );
    }

    #[test]
    fn malformed_yaml_errors() {
        let text = "---\nid: [unclosed\n---\nbody";
        assert!(parse(text, &p()).is_err());
    }
}
