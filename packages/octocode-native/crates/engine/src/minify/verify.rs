//! Proof that a compact view changes no code, for the unit tests' fixtures:
//! parse the source and the view with the grammar compaction used, and
//! compare their tokens without comments. Where the grammar's row has its
//! language's lexis, also byte by byte against the spec (`holds_to_lexis`),
//! which no parse can read wrong the same way on both sides.

use tree_sitter::Language;

mod refinement;
use refinement::{Token, refines};

use super::apply::{KeptLine, WhitespaceOnly, apply_content_view_minification};
use super::grammars::{Lexis, Spacing};
use super::layout::RegionKind;
use super::lexis::{Piece, spans};
use crate::signatures::extractor::{parse_with_deadline, whole_file_deadline};

/// A verified compact view.
#[derive(Clone, Copy, Debug)]
pub struct Verified {
    pub whitespace_only: Option<WhitespaceOnly>,
    pub view_bytes: usize,
}

/// `path`'s compact view keeps `source`'s tokens, and every kept line's text
/// comes from the line it cites, in order. Where the grammar reads all of
/// both, token for token; else (what it cannot read is kept as it is, and
/// recovery may draw its bounds elsewhere in the view) the source less the
/// comments compaction removed, without whitespace. `Err` says what changed.
pub fn verify(path: &str, source: &str) -> Result<Verified, String> {
    let view = apply_content_view_minification(source, path);
    citations(source, &view.lines)?;
    let verified = Verified {
        whitespace_only: view.whitespace_only,
        view_bytes: view.lines.iter().map(|line| line.text.len()).sum(),
    };
    let extension = crate::text::extension_of(path, true, "txt");
    let (true, Ok(lexed)) = (
        view.whitespace_only.is_none(),
        super::syntax::lex(source, &extension, &|| false),
    ) else {
        return Ok(verified);
    };
    let text: String = view.lines.iter().map(|line| line.text.as_str()).collect();
    if let Some(lexis) = lexed.grammar.lexis {
        holds_to_lexis(source, &text, lexis, lexed.spacing)?;
    }
    let (before, clean_before) = tokens(source, lexed.grammar.language)?;
    let (after, clean_after) = tokens(&text, lexed.grammar.language)?;
    if clean_before && !clean_after {
        return Err("the view does not parse".to_owned());
    }
    if clean_before {
        return refines(source, &before, &text, &after).map(|()| verified);
    }
    let mut expected = String::with_capacity(source.len());
    let mut pos = 0;
    for region in lexed
        .regions
        .iter()
        .filter(|r| r.kind == RegionKind::Comment)
    {
        expected.push_str(&source[pos..region.start]);
        pos = region.end;
    }
    expected.push_str(&source[pos..]);
    let squash = |text: &str| -> String { text.chars().filter(|c| !c.is_whitespace()).collect() };
    let (after, before) = (squash(&text), squash(&expected));
    if after == before {
        return Ok(verified);
    }
    let at = after
        .chars()
        .zip(before.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let window =
        |text: &str| -> String { text.chars().skip(at.saturating_sub(60)).take(160).collect() };
    Err(format!(
        "text changed at char {at}\nview:   {}\nsource: {}",
        window(&after),
        window(&before)
    ))
}

/// The leaf tokens of `source`, without comments, an `ERROR` node as one
/// token of its text; whether the grammar read all of it.
fn tokens<'a>(source: &'a str, language: &Language) -> Result<(Vec<Token<'a>>, bool), String> {
    let tree = parse_with_deadline(source, language, whole_file_deadline())
        .map_err(|failure| format!("{failure:?}"))?;
    let mut out = Vec::new();
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if !(node.is_named() && node.kind().contains("comment")) {
            if !node.is_error() && cursor.goto_first_child() {
                continue;
            }
            if node.end_byte() > node.start_byte() {
                let token = &source[node.start_byte()..node.end_byte()];
                // A line-break token (C's directive ends) compares as one
                // break however many blank lines it spans (`refines`).
                out.push(Token {
                    text: token,
                    aggregate: node.kind() == "preproc_arg",
                });
            }
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return Ok((out, !tree.root_node().has_error()));
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Unit {
    /// A byte that is not whitespace, or an escape with the byte it takes.
    Code,
    Comment,
    String,
}

/// `text` as the units `lexis` reads, each with whether whitespace or a
/// comment the view dropped may stand before it (`true` for whitespace).
fn units<'a>(text: &'a str, lexis: &Lexis) -> Vec<(Unit, &'a str, bool, usize)> {
    let bytes = text.as_bytes();
    let pieces = spans(text, lexis);
    let mut out = Vec::new();
    let (mut at, mut next, mut spaced) = (0, 0, false);
    while at < bytes.len() {
        if let Some(span) = pieces.get(next).filter(|span| span.start == at) {
            let unit = match span.piece {
                Piece::Comment => Unit::Comment,
                Piece::String => Unit::String,
            };
            out.push((unit, &text[span.start..span.end], spaced, at));
            (at, next, spaced) = (span.end, next + 1, false);
        } else if lexis.whitespace.contains(&bytes[at]) {
            (at, spaced) = (at + 1, true);
        } else {
            let mut end = at + if bytes[at] == lexis.escape { 2 } else { 1 };
            end = end.min(bytes.len());
            while !text.is_char_boundary(end) {
                end += 1;
            }
            out.push((Unit::Code, &text[at..end], spaced, at));
            (at, spaced) = (end, false);
        }
    }
    out
}

/// `view` is `source` less whitespace and whole comments, by the language's
/// own lexis rather than by a grammar's parse: every string byte for byte,
/// every other unit in order, no unit added, and no two units joined (where
/// the source separated them) unless `spacing` says a space there separates
/// nothing, nor one split.
fn holds_to_lexis(source: &str, view: &str, lexis: &Lexis, spacing: Spacing) -> Result<(), String> {
    let line = |at: usize| source[..at].matches('\n').count() + 1;
    let after = units(view, lexis);
    let mut next = 0;
    let mut previous: Option<u8> = None;
    let mut dropped = false;
    for (unit, text, spaced, at) in units(source, lexis) {
        let kept = after.get(next);
        if unit == Unit::Comment && kept.is_none_or(|(_, kept, _, _)| *kept != text) {
            if super::lexis::separates_tokens(source, at, at + text.len(), lexis) {
                return Err(format!(
                    "line {}: the view removes a comment separating adjacent tokens",
                    line(at)
                ));
            }
            // CSS comments emit no whitespace. Only actual source whitespace
            // before or after a removed comment can separate its neighbors.
            dropped |= spaced;
            continue;
        }
        let Some(&(kept_unit, kept_text, kept_spaced, _)) = kept else {
            return Err(format!(
                "line {}: {text:?} is missing from the view",
                line(at)
            ));
        };
        if kept_unit != unit || kept_text != text {
            return Err(format!(
                "line {}: the view has {kept_text:?} where the source has {text:?}",
                line(at)
            ));
        }
        let separated = spaced || dropped;
        if let Some(previous) = previous {
            let first = text.as_bytes()[0];
            let tight = spacing.after.contains(&previous) || spacing.before.contains(&first);
            if separated && !kept_spaced && !tight {
                return Err(format!(
                    "line {}: {text:?} joined to the unit before it",
                    line(at)
                ));
            }
            if !separated && kept_spaced {
                return Err(format!(
                    "line {}: {text:?} split from the unit before it",
                    line(at)
                ));
            }
        }
        previous = text.as_bytes().last().copied();
        (next, dropped) = (next + 1, false);
    }
    match after.get(next) {
        Some((_, extra, _, _)) => Err(format!("the view adds {extra:?}")),
        None => Ok(()),
    }
}

/// Every kept line's text is its cited line's text in order, with gaps where
/// comments went, and numbers ascend.
fn citations(source: &str, kept: &[KeptLine]) -> Result<(), String> {
    let lines: Vec<&str> = source.split_inclusive('\n').collect();
    let squash = |text: &str| -> String { text.chars().filter(|c| !c.is_whitespace()).collect() };
    for row in kept {
        let cited = squash(lines.get(row.line - 1).copied().unwrap_or(""));
        let mut rest = cited.chars();
        if !squash(&row.text).chars().all(|c| rest.any(|d| d == c)) {
            return Err(format!(
                "line {}: {:?} is not from {cited:?}",
                row.line, row.text
            ));
        }
    }
    if kept.windows(2).any(|pair| pair[0].line >= pair[1].line) {
        return Err("numbers out of order".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
