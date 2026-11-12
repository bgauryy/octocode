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
/// place ([`holes`]).
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
    if content.contains("${") {
        holes(content, &mut found, &mut patterns);
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
        // A template's holes are the line pass's ([`holes`]).
        chain.push(piece.start() + 1..piece.end() - 1);
        chain_end = piece.end();
    }
    judge_chain(content, &chain, found, patterns);
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
        let shape_only = matches!(VALUE_TESTS[index], ValueTest::Every | ValueTest::Placed)
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
        if !joined
            .get(raw.secret.clone())
            .is_some_and(|secret| VALUE_TESTS[index].redacts("", secret))
        {
            continue;
        }
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
/// `content`, in order) ([`chain_finds`]): each find adds, to `found`, the
/// part of every piece it covers with the placeholder of the pattern that
/// found it.
fn judge_chain(
    content: &str,
    chain: &[Range<usize>],
    found: &mut Vec<(Range<usize>, &'static str)>,
    patterns: &mut Vec<&'static str>,
) {
    let Some((index, finds)) = chain_finds(content, chain) else {
        return;
    };
    found.extend(finds.into_iter().flatten());
    if !patterns.contains(&PATTERNS[index].name) {
        patterns.push(PATTERNS[index].name);
    }
}

/// One secret a chain spells: the part of each piece it covers, with its
/// placeholder.
type ChainFind = Vec<(Range<usize>, &'static str)>;

/// The secrets the pieces of one literal spell (`chain`: each piece's body in
/// `content`, in order), and the pattern that found them: their text joined
/// is read as a text of its own, and a secret is the join's when it crosses a
/// seam, or lies in one piece that alone holds none there (the name that
/// makes it a credential is another piece's: `"password=" + "…"`). `None`
/// when they spell none.
fn chain_finds(content: &str, chain: &[Range<usize>]) -> Option<(usize, Vec<ChainFind>)> {
    if chain.len() < 2 {
        return None;
    }
    let mut joined = String::new();
    // Where each piece starts in the joined text.
    let mut starts = Vec::with_capacity(chain.len());
    for piece in chain {
        starts.push(joined.len());
        joined.push_str(content.get(piece.clone()).unwrap_or_default());
    }
    let result = super::redact(&joined, Scope::Pathless)?;
    let index = result
        .secrets_detected
        .first()
        .and_then(|name| PATTERNS.iter().position(|pattern| pattern.name == name))?;
    let piece_of = |at: usize| {
        starts
            .partition_point(|start| *start <= at)
            .saturating_sub(1)
    };
    let mut finds = Vec::new();
    for secret in result.edits.redacted() {
        let (first, last) = (
            piece_of(secret.start),
            piece_of(secret.end.saturating_sub(1)),
        );
        if first == last {
            // The piece's own text: the patterns read it where it is written.
            let piece = content.get(chain[first].clone()).unwrap_or_default();
            let own = secret.start - starts[first]..secret.end - starts[first];
            let alone = super::redact(piece, Scope::Pathless);
            if alone.is_some_and(|alone| alone.edits.overlaps(&own)) {
                continue;
            }
        }
        let mut find = Vec::new();
        for (piece, start) in chain.iter().zip(&starts).take(last + 1).skip(first) {
            let from = secret.start.max(*start) - start;
            let to = secret.end.min(start + piece.len()) - start;
            if from < to {
                find.push((piece.start + from..piece.start + to, replacement_for(index)));
            }
        }
        finds.push(find);
    }
    (!finds.is_empty()).then_some((index, finds))
}

/// One interpolation a template or an interpolated string writes on one
/// line (`${expr}`, `${"…"}`), no braces nested in it.
#[expect(clippy::expect_used, reason = "static pattern, valid by construction")]
static ANY_HOLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\{[^{}\r\n]*\}").expect("any hole"));

/// Judges the text each line writes around its interpolations ([`ANY_HOLE`],
/// in a template, an interpolated string, a template's line inside a
/// multi-line one): the text between them, from the quote before the first
/// (or the line's start) to the quote after the last (or the line's end),
/// joined is what the program holds there. The value after an
/// interpolation is the value a credential's name before it holds
/// (`` `password=${x}…` ``), and the interpolation stays as written. A
/// literal a hole writes is joined too only where the text around it alone
/// spells nothing (`` `ghp_${"…"}` ``): then it is part of the secret.
fn holes(
    content: &str,
    found: &mut Vec<(Range<usize>, &'static str)>,
    patterns: &mut Vec<&'static str>,
) {
    let mut line_start = 0;
    for line in content.split_inclusive('\n') {
        let start = line_start;
        line_start += line.len();
        if !line.contains("${") {
            continue;
        }
        let holes: Vec<_> = ANY_HOLE.captures_iter(line).filter_map(|hole| hole.get(0)).collect();
        let (Some(first), Some(last)) = (holes.first(), holes.last()) else {
            continue;
        };
        let quote = |c: char| matches!(c, '"' | '\'' | '`');
        let open = line
            .get(..first.start())
            .and_then(|head| head.rfind(quote))
            .map_or(0, |at| at + 1);
        let body = line.trim_end_matches(['\n', '\r']);
        let close = body
            .get(last.end()..)
            .and_then(|rest| rest.find(quote))
            .map_or(body.len(), |at| last.end() + at);
        // The text around the holes, and the literals the holes write.
        let mut around = Vec::with_capacity(holes.len() + 1);
        let mut with_literals = Vec::with_capacity(2 * holes.len() + 1);
        let mut from = open;
        for hole in &holes {
            around.push(start + from..start + hole.start());
            with_literals.push(start + from..start + hole.start());
            if let Some(literal) = HOLE.captures(hole.as_str()).and_then(|literal| {
                literal.name("dq").or_else(|| literal.name("sq"))
            }) {
                let at = start + hole.start();
                with_literals.push(at + literal.start()..at + literal.end());
            }
            from = hole.end();
        }
        around.push(start + from..start + close.max(from));
        with_literals.push(start + from..start + close.max(from));
        let plain = chain_finds(content, &around);
        let covered: Vec<Range<usize>> = plain
            .iter()
            .flat_map(|(_, finds)| finds.iter().flatten().map(|(range, _)| range.clone()))
            .collect();
        let literal = (with_literals.len() > around.len())
            .then(|| chain_finds(content, &with_literals))
            .flatten();
        for (index, finds) in plain.into_iter().chain(literal) {
            let mut any = false;
            for find in finds {
                // A find the text around the holes already makes leaves the
                // hole's literal shown.
                let overlaps = |range: &Range<usize>| {
                    covered
                        .iter()
                        .any(|seen| seen.start < range.end && range.start < seen.end)
                };
                let own = find.iter().all(|(range, _)| covered.contains(range));
                if !own && find.iter().any(|(range, _)| overlaps(range)) {
                    continue;
                }
                any = true;
                found.extend(find);
            }
            if any && !patterns.contains(&PATTERNS[index].name) {
                patterns.push(PATTERNS[index].name);
            }
        }
    }
}
