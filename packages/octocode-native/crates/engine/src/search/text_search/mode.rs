//! What a search computes per matched line: the output mode, the per-line work, and the relevance
//! signals a search ranks by.

use crate::types::{CaseRule, PatternSyntax, SearchOutput, SearchSort, TextSearchOptions};
use aho_corasick::AhoCorasick;

/// Internal projection of the mutually exclusive search output views.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Mode {
    FilesOnly,
    FilesWithoutMatch,
    CountMatches,
    CountLines,
    Normal,
}

#[derive(Clone, Copy)]
pub(super) struct MatchWork {
    pub(super) materialize_line: bool,
    pub(super) deferred_line: bool,
    pub(super) enumerate_submatches: bool,
    pub(super) collect_spans: bool,
    /// Sum [`relevance::line_weight`](crate::search::relevance::line_weight) over matched lines.
    pub(super) weigh_lines: bool,
}

pub(super) fn match_work(mode: Mode, only_matching: bool) -> MatchWork {
    MatchWork {
        materialize_line: mode == Mode::Normal,
        deferred_line: false,
        enumerate_submatches: mode != Mode::CountLines,
        collect_spans: mode == Mode::Normal && only_matching,
        weigh_lines: false,
    }
}

/// Views that report per-file matches; path-list views carry no density.
pub(super) fn lists_match_density(mode: Mode) -> bool {
    !matches!(mode, Mode::FilesOnly | Mode::FilesWithoutMatch)
}

pub(super) fn ranks_by_relevance(opts: &TextSearchOptions) -> bool {
    opts.sort == SearchSort::Relevance
}

/// A search for one bare identifier (`spawn_blocking`, literal or regex):
/// the question is where that name lives, so a declaring file leads.
pub(super) fn identifier_search(opts: &TextSearchOptions) -> bool {
    let pattern = opts.pattern.as_bytes();
    !opts.invert_match.unwrap_or(false)
        && pattern
            .first()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$'))
        && pattern
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
}

/// Most words a coverage rank tracks (one bit each).
const MAX_ALTERNATION_WORDS: usize = 64;

/// The words of a plain word alternation (`a|b|c`, optionally wrapped in one
/// group and `\b` anchors), matched with the search's case rule: the
/// `relevance` rank puts files holding more of them first, so one common
/// word repeated cannot outrank files that hold the rarer ones.
pub(super) fn alternation_words(opts: &TextSearchOptions) -> Option<AhoCorasick> {
    if opts.syntax == PatternSyntax::Literal || opts.invert_match.unwrap_or(false) {
        return None;
    }
    let mut pattern = opts.pattern.as_str();
    if let Some(inner) = pattern
        .strip_prefix(r"\b")
        .and_then(|p| p.strip_suffix(r"\b"))
    {
        pattern = inner;
    }
    if let Some(inner) = pattern.strip_prefix('(').and_then(|p| p.strip_suffix(')')) {
        pattern = inner.strip_prefix("?:").unwrap_or(inner);
    }
    let words: Vec<&str> = pattern.split('|').collect();
    let plain = |word: &&str| {
        !word.is_empty()
            && word
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$' | b'-'))
    };
    if words.len() < 2 || words.len() > MAX_ALTERNATION_WORDS || !words.iter().all(plain) {
        return None;
    }
    let insensitive = match opts.case {
        CaseRule::Insensitive => true,
        CaseRule::Sensitive => false,
        CaseRule::Smart => !pattern.bytes().any(|byte| byte.is_ascii_uppercase()),
    };
    AhoCorasick::builder()
        .ascii_case_insensitive(insensitive)
        .build(&words)
        .ok()
}

/// One matcher per searched term, built like the search's own: only a
/// count view of two or more terms counts them (empty otherwise).
pub(super) fn term_matchers<M>(
    opts: &TextSearchOptions,
    mode: Mode,
    build: impl Fn(&str) -> crate::error::Result<M>,
) -> crate::error::Result<Vec<M>> {
    match opts.terms.as_deref() {
        Some(terms) if terms.len() > 1 && matches!(mode, Mode::CountMatches | Mode::CountLines) => {
            terms.iter().map(|term| build(term)).collect()
        }
        _ => Ok(Vec::new()),
    }
}

pub(super) fn resolve_mode(opts: &TextSearchOptions) -> Mode {
    match opts.output {
        SearchOutput::Lines | SearchOutput::Spans(_) => Mode::Normal,
        SearchOutput::FilesWithMatch => Mode::FilesOnly,
        SearchOutput::FilesWithoutMatch => Mode::FilesWithoutMatch,
        SearchOutput::CountLines => Mode::CountLines,
        SearchOutput::CountMatches => Mode::CountMatches,
    }
}

/// Whether the search reports submatch spans (`-o`).
pub(super) fn only_matching(opts: &TextSearchOptions) -> bool {
    matches!(opts.output, SearchOutput::Spans(_))
}
