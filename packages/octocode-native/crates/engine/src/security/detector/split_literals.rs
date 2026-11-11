//! Secrets written as the pieces of one literal.
//!
//! A program joins `"ghp_" + "…"` (or `"ghp_" "…"`, side by side) into one
//! string, so the pieces spell a secret no piece spells alone and no pattern
//! finds in the text as written. Each chain of pieces is judged as the value
//! the program holds: the pieces' text joined, read by every pattern that
//! needs no file or key around it. What a pattern finds across a seam is
//! hidden piece by piece, each part where it is written, so the quotes, the
//! operator and a comment between the pieces stay.
//!
//! Only a find that crosses a seam is this pass's: one inside a piece is the
//! text's own, which the patterns already read there. A name that says
//! "credential" before the first piece is the assignment rules' business; this
//! pass needs none.
//!
//! A `,` separates two values, so the items of a list are no chain. Code
//! still joins some lists into one string (`concat!("ghp_", "…")`,
//! `["xoxb-", "…"].join("")`), and such a list is told from any other by
//! what it spells: from an item that opens with a pattern's own literal, its
//! items joined are exactly one secret of that pattern, ending where an item
//! ends ([`lists`]).

use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

use super::credential_values::{VALUE_TESTS, ValueTest};
use super::regex_forms::anchored_pattern_regex;
use super::{
    PATTERNS, Scope, anchored_match, document, gates, prescan, replacement_for, secret_groups,
};
use crate::security::patterns::CODE_GAP;

/// One piece: a literal of any quote type on one line, escapes read.
#[expect(clippy::expect_used, reason = "static pattern, valid by construction")]
static PIECE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#""(?:\\[^\r\n]|[^"\\\r\n])*"|'(?:\\[^\r\n]|[^'\\\r\n])*'|`(?:\\[^\r\n]|[^`\\\r\n])*`"#,
    )
    .expect("piece")
});

/// All the text between two pieces of one literal: a `+` or nothing, with
/// the blanks, line breaks and comments source code writes there
/// ([`CODE_GAP`]). A `,` separates two values and is none.
#[expect(clippy::expect_used, reason = "static pattern, valid by construction")]
static SEAM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"\A{CODE_GAP}(?:\+{CODE_GAP})?\z")).expect("seam"));

/// A literal at the start of a text ([`PIECE`], anchored).
#[expect(clippy::expect_used, reason = "static pattern, valid by construction")]
static PIECE_AT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"\A(?:{})", PIECE.as_str())).expect("piece at"));

/// What a text starts with between two items of one list: a `,` with what
/// source code writes around it ([`CODE_GAP`]).
#[expect(clippy::expect_used, reason = "static pattern, valid by construction")]
static COMMA_AT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"\A{CODE_GAP},{CODE_GAP}")).expect("comma at"));

/// A literal a template writes in a hole of its own (`` `ghp_${"…"}` ``):
/// the program holds the template's text with the literal's in the hole's
/// place.
#[expect(clippy::expect_used, reason = "static pattern, valid by construction")]
static HOLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\$\{[ \t]*(?:"(?P<dq>[^"\\\r\n]*)"|'(?P<sq>[^'\\\r\n]*)')[ \t]*\}"#)
        .expect("hole")
});

/// A closing quote, a seam and an opening quote, or a literal in a
/// template's hole: the text may hold a chain.
#[expect(clippy::expect_used, reason = "static pattern, valid by construction")]
static HINT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r#"["'`]{CODE_GAP}(?:\+{CODE_GAP})?["'`]|\$\{{[ \t]*["']"#
    ))
    .expect("hint")
});

/// What the pass replaced: the text it left, each replaced range of the
/// content with the bytes it became, and the patterns that found a secret.
pub(super) struct Pieces {
    pub(super) text: String,
    pub(super) replaced: Vec<(Range<usize>, usize)>,
    pub(super) patterns: Vec<&'static str>,
}

/// `content` with every secret its literals spell in pieces hidden; `None`
/// when no chain of pieces and no list spells one.
pub(super) fn redact_pieces(content: &str) -> Option<Pieces> {
    let mut found = Vec::new();
    let mut patterns = Vec::new();
    if HINT.is_match(content) {
        chains(content, &mut found, &mut patterns);
    }
    lists(content, &mut found, &mut patterns);
    if found.is_empty() {
        return None;
    }
    // A chain's finds and a list's come apart: in order, none inside another.
    found.sort_by_key(|(range, _)| (range.start, std::cmp::Reverse(range.end)));
    let mut covered = 0;
    found.retain(|(range, _)| {
        let apart = range.start >= covered;
        covered = covered.max(range.end);
        apart
    });
    let mut text = String::with_capacity(content.len());
    let mut replaced = Vec::with_capacity(found.len());
    let mut from = 0;
    for (range, replacement) in found {
        text.push_str(content.get(from..range.start)?);
        text.push_str(replacement);
        from = range.end;
        replaced.push((range, replacement.len()));
    }
    text.push_str(content.get(from..)?);
    Some(Pieces {
        text,
        replaced,
        patterns,
    })
}

/// Judges every chain of pieces in `content` ([`judge_chain`]).
fn chains(
    content: &str,
    found: &mut Vec<(Range<usize>, &'static str)>,
    patterns: &mut Vec<&'static str>,
) {
    let mut chain: Vec<Range<usize>> = Vec::new();
    let mut chain_end = 0;
    for piece in PIECE.find_iter(content) {
        let joins = !chain.is_empty()
            && content
                .get(chain_end..piece.start())
                .is_some_and(|between| SEAM.is_match(between));
        if !joins {
            judge_chain(content, &chain, found, patterns);
            chain.clear();
        }
        let template = piece.as_str().starts_with('`');
        push_parts(
            content,
            piece.start() + 1..piece.end() - 1,
            template,
            &mut chain,
        );
        chain_end = piece.end();
    }
    judge_chain(content, &chain, found, patterns);
}

/// Adds to `chain` the parts of the piece whose body is `body`: all of it, or
/// for a template (`template`) with literals in holes of its own ([`HOLE`])
/// the text around each hole and the literal in it, in order.
fn push_parts(content: &str, body: Range<usize>, template: bool, chain: &mut Vec<Range<usize>>) {
    let text = content.get(body.clone()).filter(|_| template);
    let mut from = body.start;
    for hole in text.into_iter().flat_map(|text| HOLE.captures_iter(text)) {
        let (Some(whole), Some(literal)) =
            (hole.get(0), hole.name("dq").or_else(|| hole.name("sq")))
        else {
            continue;
        };
        chain.push(from..body.start + whole.start());
        chain.push(body.start + literal.start()..body.start + literal.end());
        from = body.start + whole.end();
    }
    chain.push(from..body.end);
}

/// Judges every list in `content` whose items, from one that opens with a
/// literal a pattern requires, spell exactly one secret of that pattern:
/// the pattern matches the items joined from their first byte, and its
/// secret ends where an item ends, two items or more in. Each such item
/// goes with the pattern's placeholder. Only a pattern that knows its secret
/// by shape alone reads a list: a name or a file is no part of a joined text.
fn lists(
    content: &str,
    found: &mut Vec<(Range<usize>, &'static str)>,
    patterns: &mut Vec<&'static str>,
) {
    let bytes = content.as_bytes();
    let mut done = 0;
    let mut list: Option<(usize, Vec<Range<usize>>, String)> = None;
    for (start, index) in prescan::gate_hits(content) {
        let opens = start
            .checked_sub(1)
            .filter(|open| *open >= done)
            .filter(|open| matches!(bytes.get(*open), Some(b'"' | b'\'' | b'`')));
        let shape_only = VALUE_TESTS[index] == ValueTest::Every
            && PATTERNS[index].file_context.is_none()
            && document::Form::of(index).is_none();
        let (Some(open), true) = (opens, shape_only) else {
            continue;
        };
        if list.as_ref().is_none_or(|(at, ..)| *at != open) {
            let items = items_at(content, open);
            let joined = items
                .iter()
                .filter_map(|item| content.get(item.clone()))
                .collect();
            list = Some((open, items, joined));
        }
        let Some((_, items, joined)) = list.as_ref() else {
            continue;
        };
        let regex = anchored_pattern_regex(index, joined.is_ascii());
        let Some((raw, true)) = anchored_match(regex, &secret_groups(regex), joined) else {
            continue;
        };
        // The items the secret covers: all of each, two or more.
        let mut end = 0;
        let covered = items
            .iter()
            .position(|item| {
                end += item.len();
                end >= raw.secret.end
            })
            .filter(|last| raw.secret.start == 0 && end == raw.secret.end && *last >= 1);
        let Some(last) = covered else {
            continue;
        };
        let name = PATTERNS[index].name;
        for item in items.iter().take(last + 1) {
            found.push((item.clone(), replacement_for(index)));
            done = item.end;
        }
        if !patterns.contains(&name) {
            patterns.push(name);
        }
    }
}

/// The bodies of the items of the list whose first item's quote opens at
/// `open`: each literal a `,` joins to the one before, as far as the items
/// could be one secret. An empty item or one with a blank ends them, and so
/// do more bytes than a pattern's longest match.
fn items_at(content: &str, open: usize) -> Vec<Range<usize>> {
    let mut items = Vec::new();
    let mut at = open;
    let mut bytes = 0;
    while let Some(piece) = content.get(at..).and_then(|rest| PIECE_AT.find(rest)) {
        let body = at + 1..at + piece.end() - 1;
        let text = content.get(body.clone()).unwrap_or_default();
        bytes += text.len();
        if text.is_empty()
            || text.contains(char::is_whitespace)
            || bytes > gates::LONGEST_BOUNDED_MATCH
        {
            break;
        }
        items.push(body);
        let after = at + piece.end();
        let Some(comma) = content.get(after..).and_then(|rest| COMMA_AT.find(rest)) else {
            break;
        };
        at = after + comma.end();
    }
    items
}

/// Judges the pieces of one literal (`chain`: each piece's body in
/// `content`, in order): their text joined is read as a text of its own, and
/// each secret found across a seam adds, to `found`, the part of every piece
/// it covers with the placeholder of the pattern that found it.
fn judge_chain(
    content: &str,
    chain: &[Range<usize>],
    found: &mut Vec<(Range<usize>, &'static str)>,
    patterns: &mut Vec<&'static str>,
) {
    if chain.len() < 2 {
        return;
    }
    let mut joined = String::new();
    // Where each piece starts in the joined text.
    let mut starts = Vec::with_capacity(chain.len());
    for piece in chain {
        starts.push(joined.len());
        joined.push_str(content.get(piece.clone()).unwrap_or_default());
    }
    let Some(result) = super::redact(&joined, Scope::Pathless) else {
        return;
    };
    let Some(index) = result
        .secrets_detected
        .first()
        .and_then(|name| PATTERNS.iter().position(|pattern| pattern.name == name))
    else {
        return;
    };
    let piece_of = |at: usize| {
        starts
            .partition_point(|start| *start <= at)
            .saturating_sub(1)
    };
    let mut crossed = false;
    for secret in result.edits.redacted() {
        let (first, last) = (
            piece_of(secret.start),
            piece_of(secret.end.saturating_sub(1)),
        );
        if first == last {
            continue;
        }
        crossed = true;
        for (piece, start) in chain.iter().zip(&starts).take(last + 1).skip(first) {
            let from = secret.start.max(*start) - start;
            let to = secret.end.min(start + piece.len()) - start;
            if from < to {
                found.push((piece.start + from..piece.start + to, replacement_for(index)));
            }
        }
    }
    if crossed && !patterns.contains(&PATTERNS[index].name) {
        patterns.push(PATTERNS[index].name);
    }
}
