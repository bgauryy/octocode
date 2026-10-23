//! Cheap per-file signals behind `sort: "relevance"` (see
//! `text_search::result::compare_recs`). For a bare-identifier search, a source
//! file with a [`DECLARATION_WEIGHT`] hit ranks before the match count; the
//! rest order files after the count and before the path:
//!
//! 1. [`is_demoted_path`] — test, fixture, generated, bundled, and vendored
//!    paths rank after source paths.
//! 2. [`line_weight`] summed over a file's matched lines — a hit on the name a
//!    declaration introduces outranks the same token in ordinary code, which
//!    outranks it in a comment or a string literal.
//!
//! Both are lexical: no parser, no index. The line weights allocate nothing;
//! the path checks run once per file.

mod declaration;
mod demotion;

pub use declaration::is_declaration_keyword;
pub(crate) use demotion::{
    GENERATED_HEADER_BYTES, GitAttributes, has_generated_header, is_demoted_path, is_generated_path,
};

use declaration::declares_at;

/// Weight of a matched line whose first match starts at byte `first_match`.
pub(crate) const DECLARATION_WEIGHT: u32 = 2;
const CODE_WEIGHT: u32 = 1;
const PROSE_WEIGHT: u32 = 0;

/// Relevance weight of one matched line: [`DECLARATION_WEIGHT`] when the first
/// match is the name a declaration introduces, `0` inside a comment or a
/// string literal, `1` for any other code.
pub(crate) fn line_weight(line: &[u8], first_match: usize) -> u32 {
    let first_match = first_match.min(line.len());
    let indent = line
        .iter()
        .take_while(|byte| byte.is_ascii_whitespace())
        .count();
    if starts_comment(&line[indent..]) || in_string_or_comment(line, indent, first_match) {
        return PROSE_WEIGHT;
    }
    if declares_at(line, indent, first_match) {
        DECLARATION_WEIGHT
    } else {
        CODE_WEIGHT
    }
}

/// Rank of one matched line among a file's hits, used to choose which rows a
/// clipped file shows first: 3 when the match is the name a declaration
/// introduces, 2 on a deciding statement (an assignment, a branch, or a
/// `return`/`raise`/`throw`), 1 on other code, 0 in a comment or string.
pub(crate) fn line_rank(line: &[u8], first_match: usize) -> u32 {
    match line_weight(line, first_match) {
        PROSE_WEIGHT => 0,
        DECLARATION_WEIGHT => 3,
        _ => {
            let indent = line
                .iter()
                .take_while(|byte| byte.is_ascii_whitespace())
                .count();
            if opens_with_decision(&line[indent..]) || assigns(&line[indent..]) {
                2
            } else {
                1
            }
        }
    }
}

/// Words that open a statement deciding control flow or a result.
const DECISION_KEYWORDS: &[&[u8]] = &[
    b"if", b"elif", b"else", b"case", b"when", b"switch", b"match", b"while", b"for", b"return",
    b"raise", b"throw", b"yield", b"guard", b"unless", b"except", b"catch",
];

/// First word after any closing braces (`} else if`) is a decision keyword.
fn opens_with_decision(text: &[u8]) -> bool {
    let start = text
        .iter()
        .take_while(|byte| matches!(byte, b'}' | b')') || byte.is_ascii_whitespace())
        .count();
    let word = text[start..]
        .iter()
        .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'_')
        .count();
    DECISION_KEYWORDS.contains(&&text[start..start + word])
}

/// An assignment operator outside brackets, strings, and a trailing `//`
/// comment: `x = y`, `x := y`, `x += y`. Comparisons (`==`, `!=`, `<=`, `>=`),
/// arrows (`=>`), and keyword arguments inside a call (`f(a=1)`) do not count.
fn assigns(text: &[u8]) -> bool {
    let mut depth = 0usize;
    let mut quote = None;
    let mut at = 0;
    while at < text.len() {
        let byte = text[at];
        match quote {
            Some(_) if byte == b'\\' => at += 1,
            Some(open) if byte == open => quote = None,
            Some(_) => {}
            None => match byte {
                b'"' | b'\'' | b'`' => quote = Some(byte),
                b'/' if text.get(at + 1) == Some(&b'/') => return false,
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth = depth.saturating_sub(1),
                b'=' if depth == 0 => {
                    let before = at.checked_sub(1).map(|i| text[i]);
                    let after = text.get(at + 1).copied();
                    let comparison = matches!(after, Some(b'=' | b'>'))
                        || matches!(before, Some(b'=' | b'!' | b'<' | b'>'));
                    if !comparison {
                        return true;
                    }
                }
                _ => {}
            },
        }
        at += 1;
    }
    false
}

fn starts_comment(text: &[u8]) -> bool {
    let next_is_blank = |at: usize| text.get(at).is_none_or(u8::is_ascii_whitespace);
    text.starts_with(b"//")
        || text.starts_with(b"/*")
        || text.starts_with(b"<!--")
        || (text.starts_with(b"*") && (next_is_blank(1) || text.get(1) == Some(&b'/')))
        || (text.starts_with(b"#") && (next_is_blank(1) || text.get(1) == Some(&b'#')))
        || (text.starts_with(b"--") && next_is_blank(2))
}

/// Whether byte `end` sits inside a `"`/`` ` `` string or after a `//` or
/// `/*` comment opener on this line. Single quotes are not tracked: they are
/// Rust lifetimes and char literals as often as strings.
fn in_string_or_comment(line: &[u8], start: usize, end: usize) -> bool {
    let mut quote = None;
    let mut at = start;
    while at < end {
        let byte = line[at];
        match quote {
            Some(_) if byte == b'\\' => at += 1,
            Some(open) if byte == open => quote = None,
            Some(_) => {}
            None if byte == b'"' || byte == b'`' => quote = Some(byte),
            None if byte == b'/' && matches!(line.get(at + 1), Some(b'/' | b'*')) => return true,
            None => {}
        }
        at += 1;
    }
    quote.is_some()
}

#[cfg(test)]
mod tests;
