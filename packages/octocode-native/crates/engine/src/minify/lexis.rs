//! A language's own rules for where its comments and strings are, from its
//! spec, as data on a grammar row (`grammars::Lexis`). A grammar's parse can
//! read these wrong where it recovers from an error or where its lexer prefers
//! another token, so for a row that has them, compaction removes only a
//! comment both agree on and keeps every string of the spec byte for byte
//! (`agree`), and `verify` proves the view against them byte by byte.

use super::grammars::Lexis;
use super::layout::{Region, RegionKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Piece {
    Comment,
    String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Span {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) piece: Piece,
}

/// The comments and strings of `source` by `lexis`, in order. An escape
/// takes the byte after it, a line break too. A string ends at its quote or
/// before a line break no escape takes; an unclosed comment or string runs to
/// the end. No comment starts in the unquoted argument of a raw call.
pub(super) fn spans(source: &str, lexis: &Lexis) -> Vec<Span> {
    let bytes = source.as_bytes();
    let (open, close) = lexis.block_comment;
    let mut spans = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let rest = &bytes[at..];
        let byte = bytes[at];
        let (end, piece) = if byte == lexis.escape {
            at = (at + 2).min(bytes.len());
            continue;
        } else if rest.starts_with(open.as_bytes()) {
            let body = at + open.len();
            let end = find(&bytes[body..], close.as_bytes())
                .map_or(bytes.len(), |found| body + found + close.len());
            (end, Piece::Comment)
        } else if lexis
            .line_comment
            .is_some_and(|opener| rest.starts_with(opener.as_bytes()))
        {
            let end = rest
                .iter()
                .position(|b| *b == b'\n')
                .map_or(bytes.len(), |found| at + found);
            (end, Piece::Comment)
        } else if lexis.quotes.contains(&byte) {
            (string_end(bytes, at, lexis.escape), Piece::String)
        } else if let Some(argument) = raw_argument(bytes, at, lexis) {
            at = raw_end(bytes, argument, lexis.escape);
            continue;
        } else {
            at += 1;
            continue;
        };
        spans.push(Span {
            start: at,
            end,
            piece,
        });
        at = end;
    }
    spans
}

/// `regions` (from a grammar's parse) where they agree with `spans` (from
/// the language's spec): a comment region goes only where the spec has the
/// same comment and following source whitespace separates its neighbors. A
/// comment between adjacent tokens is retained: inserting whitespace can
/// change a selector, while deleting it can merge tokens. A spec string,
/// and any region it overlaps, stays byte for byte.
pub(super) fn agree(
    source: &str,
    regions: Vec<Region>,
    spans: &[Span],
    lexis: &Lexis,
) -> Vec<Region> {
    let is_spec_comment = |region: &Region| {
        spans
            .binary_search_by_key(&region.start, |span| span.start)
            .is_ok_and(|found| {
                spans[found].piece == Piece::Comment && spans[found].end == region.end
            })
    };
    let strings = spans
        .iter()
        .filter(|span| span.piece == Piece::String)
        .map(|span| Region {
            start: span.start,
            end: span.end,
            kind: RegionKind::Literal,
        });
    let mut all: Vec<Region> = regions
        .into_iter()
        .map(|region| Region {
            kind: if region.kind == RegionKind::Comment
                && (!is_spec_comment(&region)
                    || separates_tokens(source, region.start, region.end, lexis))
            {
                RegionKind::Literal
            } else {
                region.kind
            },
            ..region
        })
        .chain(strings)
        .collect();
    all.sort_by_key(|region| region.start);
    let mut merged: Vec<Region> = Vec::with_capacity(all.len());
    for region in all {
        match merged.last_mut() {
            Some(last) if region.start < last.end => {
                last.end = last.end.max(region.end);
                last.kind = RegionKind::Literal;
            }
            _ => merged.push(region),
        }
    }
    merged
}

/// A following whitespace code point is outside this comment and the token
/// before it. Whitespace before the comment is not enough: it may belong to an
/// escape (`\61 ` is one CSS escape). Otherwise retain the lexical boundary,
/// even in malformed token sequences, rather than insert a space or join tokens.
pub(super) fn separates_tokens(source: &str, start: usize, end: usize, lexis: &Lexis) -> bool {
    let bytes = source.as_bytes();
    start > 0 && end < bytes.len() && !lexis.whitespace.contains(&bytes[end])
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn string_end(bytes: &[u8], start: usize, escape: u8) -> usize {
    let quote = bytes[start];
    let mut at = start + 1;
    while at < bytes.len() {
        match bytes[at] {
            byte if byte == escape => at += 2,
            byte if byte == quote => return at + 1,
            b'\n' => return at,
            _ => at += 1,
        }
    }
    bytes.len()
}

/// Where the unquoted argument of a raw call that starts at `at` begins.
fn raw_argument(bytes: &[u8], at: usize, lexis: &Lexis) -> Option<usize> {
    let in_name =
        |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') || byte >= 0x80;
    if at > 0 && (in_name(bytes[at - 1]) || bytes[at - 1] == lexis.escape) {
        return None;
    }
    let call = lexis.raw_calls.iter().find(|call| {
        bytes
            .get(at..at + call.len())
            .is_some_and(|name| name.eq_ignore_ascii_case(call.as_bytes()))
    })?;
    let argument = at + call.len();
    let first = bytes[argument..]
        .iter()
        .position(|byte| !lexis.whitespace.contains(byte))
        .map_or(bytes.len(), |found| argument + found);
    (!bytes
        .get(first)
        .is_some_and(|byte| lexis.quotes.contains(byte)))
    .then_some(argument)
}

fn raw_end(bytes: &[u8], mut at: usize, escape: u8) -> usize {
    while at < bytes.len() {
        match bytes[at] {
            byte if byte == escape => at += 2,
            b')' => return at + 1,
            _ => at += 1,
        }
    }
    bytes.len()
}

#[cfg(test)]
mod tests;
