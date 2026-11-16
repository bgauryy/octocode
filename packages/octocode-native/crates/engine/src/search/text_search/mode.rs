//! What a search computes per matched line: the output mode and the per-line work.

use crate::types::{SearchOutput, TextSearchOptions};

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
    /// Show a matched line too long for a row as windows around its hits.
    pub(super) row_window: Option<crate::types::RowWindow>,
}

pub(super) fn match_work(mode: Mode, only_matching: bool) -> MatchWork {
    MatchWork {
        materialize_line: mode == Mode::Normal,
        deferred_line: false,
        enumerate_submatches: mode != Mode::CountLines,
        collect_spans: mode == Mode::Normal && only_matching,
        row_window: None,
    }
}

/// One matcher per searched term, built like the search's own: only a
/// search of two or more terms counts them for each file it keeps (empty
/// otherwise): a count view's per-term counts, and for any other view which
/// terms the files that matched hold.
pub(super) fn term_matchers<M>(
    opts: &TextSearchOptions,
    mode: Mode,
    build: impl Fn(&str) -> crate::error::Result<M>,
) -> crate::error::Result<Vec<M>> {
    match opts.terms.as_deref() {
        Some(terms) if terms.len() > 1 && mode != Mode::FilesWithoutMatch => {
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
