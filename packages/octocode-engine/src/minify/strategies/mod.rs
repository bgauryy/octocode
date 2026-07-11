pub mod code;
pub mod core;
pub mod json;
pub mod markdown;
pub mod web;

pub use code::{minify_javascript_core, minify_js_oxc};
pub use core::{minify_aggressive, minify_code_core, minify_conservative, minify_general_core};
pub use json::{minify_json_core_inner, minify_json_readable_inner};
pub use markdown::minify_markdown_core;
pub use web::{
    minify_css_core, minify_css_quality, minify_embedded_web, minify_html_core, minify_html_quality,
};

// ── Shared byte-level helpers (used across submodules) ────────────────────────

/// Byte length of the UTF-8 sequence starting with `b`. Callers only invoke
/// this on sequence-lead bytes (valid `&str` + always advancing by full
/// sequences), so continuation bytes never reach it.
#[inline]
pub(crate) fn utf8_seq_len(b: u8) -> usize {
    match b {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// Copy the full UTF-8 sequence at byte `i` of `s` into `result`; returns the
/// index of the next sequence. Keeps byte-level scanners multibyte-safe: all
/// branch decisions compare ASCII bytes (which never occur inside a multibyte
/// sequence), and passthrough copies whole sequences instead of `u8 as char`.
#[inline]
pub(super) fn copy_seq(s: &str, i: usize, result: &mut String) -> usize {
    let len = utf8_seq_len(s.as_bytes()[i]).min(s.len() - i);
    result.push_str(&s[i..i + len]);
    i + len
}

/// Advance `ri` (a cursor into a sorted, non-overlapping literal-range list)
/// past any range that already ends at or before `pos`, then report whether
/// `pos` falls inside the range `ri` now points at.
#[inline]
fn in_literal_at(ranges: &[(usize, usize)], ri: &mut usize, pos: usize) -> Option<(usize, usize)> {
    while *ri < ranges.len() && ranges[*ri].1 <= pos {
        *ri += 1;
    }
    if *ri < ranges.len() && ranges[*ri].0 <= pos && pos < ranges[*ri].1 {
        Some(ranges[*ri])
    } else {
        None
    }
}

/// Collapse runs of whitespace to a single space, treating any byte ranges
/// reported by `crate::comment_remover::literal_ranges` (when `rules` is
/// given) as opaque — copied verbatim, never collapsed, so string/regex/
/// template-literal content is never mutated by the aggressive strategy.
///
/// A run containing a newline collapses to a single `\n` rather than `' '`:
/// several aggressive-strategy languages (Elixir, Perl, Erlang, Clojure, ...)
/// use a bare newline as a statement separator, so flattening it to a space
/// silently changes program meaning. Collapsing runs of horizontal whitespace
/// (spaces/tabs) is safe everywhere and still happens unconditionally.
pub(super) fn collapse_whitespace(
    s: &str,
    rules: Option<&crate::comment_remover::CommentRules>,
) -> String {
    let ranges = rules
        .map(|r| crate::comment_remover::literal_ranges(s, r))
        .unwrap_or_default();
    let mut ri = 0usize;
    let bytes = s.as_bytes();
    let mut result = String::with_capacity(s.len());
    let mut ws_has_newline: Option<bool> = None;
    let mut i = 0usize;
    while i < bytes.len() {
        if let Some((_, end)) = in_literal_at(&ranges, &mut ri, i) {
            if let Some(has_nl) = ws_has_newline.take() {
                result.push(if has_nl { '\n' } else { ' ' });
            }
            result.push_str(&s[i..end]);
            i = end;
            continue;
        }
        let ch_len = utf8_seq_len(bytes[i]);
        let ch_str = &s[i..i + ch_len];
        let is_ws = ch_str.chars().next().is_some_and(char::is_whitespace);
        if is_ws {
            let is_nl = ch_str == "\n" || ch_str == "\r";
            let cur = ws_has_newline.get_or_insert(false);
            *cur = *cur || is_nl;
        } else {
            if let Some(has_nl) = ws_has_newline.take() {
                result.push(if has_nl { '\n' } else { ' ' });
            }
            result.push_str(ch_str);
        }
        i += ch_len;
    }
    if let Some(has_nl) = ws_has_newline.take() {
        result.push(if has_nl { '\n' } else { ' ' });
    }
    result
}

/// Remove spaces around `{}:;, ><`, treating literal ranges (see
/// `collapse_whitespace`) as opaque so punctuation-looking bytes inside a
/// string/regex/template literal are never touched.
pub(super) fn re_tighten_punct(
    s: &str,
    rules: Option<&crate::comment_remover::CommentRules>,
) -> String {
    let ranges = rules
        .map(|r| crate::comment_remover::literal_ranges(s, r))
        .unwrap_or_default();
    let mut ri = 0usize;
    let bytes = s.as_bytes();
    let mut result = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if let Some((_, end)) = in_literal_at(&ranges, &mut ri, i) {
            result.push_str(&s[i..end]);
            i = end;
            continue;
        }
        let b = bytes[i];
        if b == b' '
            && matches!(
                bytes.get(i + 1).copied(),
                Some(b'{' | b'}' | b':' | b';' | b',' | b'<' | b'>')
            )
        {
            i += 1;
            continue;
        }
        if matches!(b, b'{' | b'}' | b':' | b';' | b',') && bytes.get(i + 1) == Some(&b' ') {
            result.push(b as char);
            i += 2;
            continue;
        }
        if b == b'>' && bytes.get(i + 1) == Some(&b' ') && bytes.get(i + 2) == Some(&b'<') {
            result.push('>');
            i += 2;
            continue;
        }
        i = copy_seq(s, i, &mut result);
    }
    result
}

// ── Tests ─────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    // ── JSON ──────────────────────────────────────────────────────────────────
    #[test]
    fn json_core_compacts_valid_json() {
        let (out, failed) = minify_json_core_inner("{\"a\": 1,  \"b\": 2 }");
        assert_eq!(out, r#"{"a":1,"b":2}"#);
        assert!(!failed);
    }

    #[test]
    fn json_core_strips_jsonc_comments_and_trailing_commas() {
        let src = "{ // comment\n  \"key\": \"value\", // trailing comma\n}";
        let (out, failed) = minify_json_core_inner(src);
        assert!(!failed);
        assert!(out.contains("key"));
        assert!(!out.contains("comment"));
    }

    #[test]
    fn json_core_preserves_non_ascii_through_jsonc_strip() {
        let (out, failed) = minify_json_core_inner("{\n  // comment\n  \"k\": \"café\",\n}");
        assert!(!failed);
        assert!(out.contains("café"), "JSONC strip corrupted UTF-8: '{out}'");
        assert!(!out.contains('Ã'), "Latin-1 mojibake detected: '{out}'");
    }

    #[test]
    fn json_core_marks_unparseable_input_failed() {
        let (out, failed) = minify_json_core_inner("{ invalid json");
        assert!(failed);
        assert_eq!(out, "{ invalid json");
    }

    #[test]
    fn json_readable_marks_unparseable_input_failed() {
        let (out, failed) = minify_json_readable_inner("{ invalid json");
        assert!(failed);
        assert_eq!(out, "{ invalid json");
    }

    #[test]
    fn json_readable_accepts_jsonc_after_noise_strip() {
        let src = "{\n  // comment\n  \"key\": \"value\",\n}\n";
        let (out, failed) = minify_json_readable_inner(src);
        assert!(!failed);
        assert!(out.contains("\"key\""));
        assert!(!out.contains("comment"));
    }

    #[test]
    fn json_core_preserves_bignum_precision() {
        let (out, failed) = minify_json_core_inner(r#"{"n": 123456789012345678901234567890}"#);
        assert!(!failed);
        assert!(
            out.contains("123456789012345678901234567890"),
            "bignum lost precision: '{out}'"
        );
    }

    #[test]
    fn json_core_preserves_decimal_precision() {
        let (out, failed) = minify_json_core_inner(r#"{"pi": 3.14159265365358979}"#);
        assert!(!failed);
        assert!(
            out.contains("3.14159265365358979"),
            "high-precision decimal was rounded: '{out}'"
        );
    }

    #[test]
    fn json_core_preserves_key_order() {
        let (out, failed) = minify_json_core_inner(r#"{"z": 1, "a": 2, "m": 3}"#);
        assert!(!failed);
        assert_eq!(
            out, r#"{"z":1,"a":2,"m":3}"#,
            "keys were reordered: '{out}'"
        );
    }

    // ── conservative ──────────────────────────────────────────────────────────
    #[test]
    fn conservative_strips_c_style_comments() {
        let out = minify_conservative("int x; // comment\nint y;", Some(&["c-style"]));
        assert!(!out.contains("comment"));
        assert!(out.contains("int x"));
    }

    #[test]
    fn conservative_strips_hash_comments_preserving_code() {
        let out = minify_conservative("x = 1 # comment\n# full line\ny = 2", Some(&["hash"]));
        assert!(!out.contains("comment"));
        assert!(!out.contains("full line"));
        assert!(out.contains("x = 1") && out.contains("y = 2"));
    }

    // ── code core ─────────────────────────────────────────────────────────────
    #[test]
    fn code_core_collapses_blank_runs_to_one() {
        // Mirrors the TS contract exactly: "a\n\n\n\nb" → "a\n\nb"
        assert_eq!(minify_code_core("a\n\n\n\nb"), "a\n\nb");
    }

    // ── aggressive: UTF-8 safety through collapse + punct tightening ──────────
    #[test]
    fn aggressive_preserves_non_ascii() {
        let out = minify_aggressive("local s = \"café → naïve\" { x = 1 }", None);
        assert!(
            out.contains("café → naïve"),
            "aggressive path corrupted UTF-8: '{out}'"
        );
        assert!(!out.contains('Ã'), "Latin-1 mojibake detected: '{out}'");
    }

    // ── javascript core ───────────────────────────────────────────────────────
    #[test]
    fn javascript_core_preserves_non_ascii_and_strips_comments() {
        let out = minify_javascript_core("const s = \"café\"; // strip me");
        assert!(
            out.contains("café"),
            "punct tightening corrupted UTF-8: '{out}'"
        );
        assert!(!out.contains("strip me"));
    }

    // ── regression: aggressive strategy must not mutate string content ────────
    #[test]
    fn aggressive_lua_preserves_internal_string_whitespace_and_punctuation() {
        let out = minify_aggressive(
            "local s = \"hello    world\"\nlocal t = \"a: b, c\"",
            Some(&["lua"]),
        );
        assert!(
            out.contains("hello    world"),
            "aggressive collapsed whitespace inside a string literal: '{out}'"
        );
        assert!(
            out.contains("a: b, c"),
            "aggressive tightened punctuation inside a string literal: '{out}'"
        );
    }

    #[test]
    fn aggressive_preserves_newline_as_statement_separator() {
        // Elixir-shaped input: a bare newline ends a statement. Flattening it
        // to a space (the pre-fix behavior) produces invalid Elixir.
        let out = minify_aggressive("x = 1\ny = 2", Some(&["hash"]));
        assert_eq!(out, "x = 1\ny = 2");
    }

    #[test]
    fn javascript_core_preserves_template_literal_content() {
        let out = minify_javascript_core("const s = `key: ${v}, end`;");
        assert!(
            out.contains("`key: ${v}, end`"),
            "JS heuristic fallback mutated template literal content: '{out}'"
        );
    }

    #[test]
    fn aggressive_clojure_quote_prefix_does_not_swallow_following_string() {
        // Regression: `'` is Clojure's quote-prefix, not a string delimiter.
        // Using the shared DEFAULT_QUOTE_DELIMITERS (which include `'`) would
        // treat `'(a b))` as an unterminated string, swallow the real string
        // literal that follows, and collapse its internal whitespace.
        let out = minify_aggressive(
            "(def x '(a b))  (def y \"hello   world\")",
            Some(&["clojure"]),
        );
        assert!(
            out.contains("\"hello   world\""),
            "quote-prefix apostrophe corrupted a later real string literal: '{out}'"
        );
    }

    #[test]
    fn javascript_core_preserves_internal_string_whitespace() {
        let out = minify_javascript_core("const s = \"a    b\";");
        assert!(
            out.contains("\"a    b\""),
            "JS heuristic fallback collapsed whitespace inside a string literal: '{out}'"
        );
    }

    // ── markdown ──────────────────────────────────────────────────────────────
    #[test]
    fn markdown_normalizes_crlf_without_carriage_returns() {
        let out = minify_markdown_core("# Title\r\n\r\nSome text  \r\n");
        assert!(out.contains("# Title"));
        assert!(out.contains("Some text"));
        assert!(!out.contains('\r'));
    }

    #[test]
    fn markdown_strips_emoji_noise_and_joins_paragraph_wraps() {
        let src = "# Guide 🚀\n\nThis is a soft\nwrapped paragraph 😊 with :sparkles: punctuation .\n\nSecond paragraph.\n\n```js\nconsole.log(\"😀 keep code literal\");\n```\n";
        let out = minify_markdown_core(src);
        assert!(out.contains("# Guide"), "heading kept: '{out}'");
        assert!(
            out.contains("This is a soft wrapped paragraph with punctuation."),
            "soft wrap should join and punctuation should tighten: '{out}'"
        );
        assert!(out.contains("Second paragraph."));
        assert!(
            out.contains("console.log(\"😀 keep code literal\");"),
            "fenced code is literal content: '{out}'"
        );
        assert!(!out.contains('🚀'));
        assert!(!out.contains('😊'));
        assert!(!out.contains(":sparkles:"));
        assert!(
            !out.contains("\n\n"),
            "blank padding should be removed: '{out}'"
        );
    }

    #[test]
    fn markdown_drops_image_anchor_and_break_noise() {
        let src = "# Assets\n\n<a id=\"top\"></a>\n<br />\n![Build](https://img.shields.io/badge/build-passing-green)\n![Screenshot](./screen.png)\nText <br> after break.\n";
        let out = minify_markdown_core(src);
        assert!(out.contains("# Assets"));
        assert!(out.contains("Text after break."));
        assert!(!out.contains("img.shields.io"));
        assert!(!out.contains("Screenshot"));
        assert!(!out.contains("<a id"));
        assert!(!out.contains("<br"));
    }
}
