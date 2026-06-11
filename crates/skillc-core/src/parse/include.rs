//! `@include <block>` line-directive extraction (T014).
//!
//! An `@include` is a line directive: optional leading whitespace, the literal `@include`,
//! at least one space, then a single block-id token. Anything after the id on the line is
//! ignored. `@includes` (no space) is *not* a directive.

/// Extract the block ids referenced by `@include <block-id>` line directives in `body`,
/// in document order.
pub fn extract(body: &str) -> Vec<String> {
    body.lines()
        .filter_map(|line| parse_line(line).map(str::to_string))
        .collect()
}

/// Parse a single line; returns the block id if it is an `@include` directive.
pub fn parse_line(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix("@include")?;
    // Require whitespace immediately after the directive keyword.
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let id = rest.split_whitespace().next()?;
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

/// Is this line an `@include` directive (for any block)?
pub fn is_include_line(line: &str) -> bool {
    parse_line(line).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_in_order() {
        let body = "intro\n@include a\nmiddle\n  @include b  \n@include a\n";
        assert_eq!(extract(body), vec!["a", "b", "a"]);
    }

    #[test]
    fn includes_keyword_without_space_is_not_a_directive() {
        assert!(parse_line("@includes pr-rules").is_none());
        assert!(parse_line("text @include not-at-line-start").is_none());
    }

    #[test]
    fn leading_whitespace_allowed() {
        assert_eq!(parse_line("    @include nested"), Some("nested"));
    }
}
