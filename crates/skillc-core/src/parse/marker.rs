//! `{{Alias}}` inline-marker extraction and rendering (T021).
//!
//! A marker is `{{` + an identifier (letters / digits / `_` / `-`) + `}}`. The literal
//! escape `\{{` emits a literal `{{` and is **not** a marker. Markers resolve to an
//! `imports` alias (FR-007); rendering replaces each with the resolved unit's id.

/// Extract the aliases referenced by `{{Alias}}` markers in `body`, in document order
/// (may repeat). `\{{` escapes are skipped.
pub fn extract(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    scan(body, |alias| out.push(alias.to_string()), |_| {});
    out
}

/// Render markers: replace each `{{Alias}}` with `resolve(alias)`; if `resolve` returns
/// `None` the original marker text is preserved (validation should have rejected it
/// already). `\{{` becomes a literal `{{`.
pub fn render(body: &str, resolve: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(body.len());
    render_into(body, &mut out, resolve);
    out
}

/// Core scanner. Calls `on_marker(alias)` for each real marker and `on_literal_escape`
/// when a `\{{` escape is seen (offset of the `\`).
fn scan(body: &str, mut on_marker: impl FnMut(&str), mut on_escape: impl FnMut(usize)) {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if i > 0 && bytes[i - 1] == b'\\' {
                on_escape(i - 1);
                i += 2;
                continue;
            }
            if let Some(rel) = body[i + 2..].find("}}") {
                let inner = body[i + 2..i + 2 + rel].trim();
                if is_ident(inner) {
                    on_marker(inner);
                    i = i + 2 + rel + 2;
                    continue;
                }
            }
        }
        i += 1;
    }
}

fn render_into(body: &str, out: &mut String, resolve: &dyn Fn(&str) -> Option<String>) {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Literal escape: `\{{` → `{{`.
        if bytes[i] == b'\\' && i + 2 < bytes.len() && bytes[i + 1] == b'{' && bytes[i + 2] == b'{'
        {
            out.push_str("{{");
            i += 3;
            continue;
        }
        if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            if let Some(rel) = body[i + 2..].find("}}") {
                let inner = body[i + 2..i + 2 + rel].trim();
                if is_ident(inner) {
                    match resolve(inner) {
                        Some(target) => out.push_str(&target),
                        None => out.push_str(&body[i..i + 2 + rel + 2]),
                    }
                    i = i + 2 + rel + 2;
                    continue;
                }
            }
        }
        // Push one UTF-8 char worth of bytes.
        let ch_len = utf8_len(bytes[i]);
        out.push_str(&body[i..i + ch_len]);
        i += ch_len;
    }
}

/// A valid `{{}}` identifier: non-empty, ASCII alphanumeric plus `_` and `-`.
fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Byte length of the UTF-8 character starting with `first`.
fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_markers_in_order() {
        assert_eq!(
            extract("use {{CodeReview}} and {{Git}} and {{CodeReview}}"),
            vec!["CodeReview", "Git", "CodeReview"]
        );
    }

    #[test]
    fn escaped_marker_is_not_extracted() {
        assert_eq!(
            extract(r"literal \{{NotAMarker}} here"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn render_resolves_and_escapes() {
        let map = |a: &str| match a {
            "CodeReview" => Some("code-review".to_string()),
            _ => None,
        };
        assert_eq!(render("see {{CodeReview}}", &map), "see code-review");
        assert_eq!(render(r"literal \{{X}}", &map), "literal {{X}}");
    }

    #[test]
    fn render_preserves_unresolved() {
        let map = |_: &str| None;
        assert_eq!(render("{{Unknown}}", &map), "{{Unknown}}");
    }

    #[test]
    fn handles_utf8_body() {
        let map = |_: &str| Some("x".to_string());
        assert_eq!(render("中文 {{A}} 結尾", &map), "中文 x 結尾");
    }
}
