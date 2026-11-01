use super::types::{ExtractionError, LineRange, NextCalls, line_read, uncovered};
use crate::tools::source_view::local_fetch_query;
use octocode_contract_types::{ErrorCode, LocalFetchQuery};

mod matches;
mod search_lead;
mod spans;

pub(crate) fn match_spans(
    q: &LocalFetchQuery,
    content: &str,
) -> Result<super::types::RegexSpans, ExtractionError> {
    spans::match_spans(q, content, &local_fetch_query::match_strings(q))
}

/// The 1-based lines of `content` a match of `q`'s patterns touches, as a
/// whole-file match read selects them, and whether a regex scan stopped at
/// its match limit.
pub(crate) fn match_lines(
    q: &LocalFetchQuery,
    content: &str,
) -> Result<(Vec<usize>, bool), ExtractionError> {
    let found = match_spans(q, content)?;
    let lines = content.split_inclusive('\n').collect::<Vec<_>>();
    Ok((spans::hit_lines(&lines, &found.spans), found.truncated))
}

#[cfg(test)]
mod canonical_tests;

use matches::match_extract;
pub(crate) use search_lead::search_lead;
#[cfg(test)]
pub(crate) use spans::{MAX_REGEX_MATCHES, RUST_REGEX_RUNS, literal_ranges};

pub struct Extraction {
    pub text: String,
    pub source_lines: Option<Vec<usize>>,
    pub start: Option<usize>,
    pub end: Option<usize>,
    pub match_ranges: Vec<LineRange>,
    pub matched_lines: Vec<usize>,
    pub count: Option<usize>,
    pub warnings: Vec<String>,
    /// Continuations to the data this view selected but did not return.
    pub next: NextCalls,
    /// The declarations a `block:true` match read widened hits to.
    pub blocks: Vec<super::types::DeclaredBlock>,
    /// A regex scan stopped at its match limit: the selection is a prefix
    /// of the matches (warned, with a `findText` lead that pages them all;
    /// the response delivers it as `hints.findText`).
    pub match_limited: bool,
}

fn follow_up(query: LocalFetchQuery) -> Option<super::types::FetchCall> {
    Some(super::types::FetchCall::continued(query))
}

/// View-line sentinel for an omission marker (source lines are 1-based).
pub const OMISSION_LINE: usize = 0;

/// Separates non-adjacent requested windows (ranges or match windows), so a
/// gap never reads as lines cut from a span the read asked for: a requested
/// span is never elided.
pub(crate) fn omission_marker(start: usize, end: usize) -> String {
    crate::response::numbered::omission_marker(
        start,
        end,
        crate::response::numbered::Omitted::NotRequested,
    )
}
pub fn line_count(s: &str) -> usize {
    s.split_inclusive('\n').count()
}
pub fn extract(q: &LocalFetchQuery, content: &str) -> Result<Extraction, ExtractionError> {
    if selects_whole(q) {
        return Ok(Extraction::whole(content.into()));
    }
    let patterns = local_fetch_query::match_strings(q);
    let requested = local_fetch_query::line_ranges(q);
    let lines = content.split_inclusive('\n').collect::<Vec<_>>();
    if !patterns.is_empty() {
        return match_extract(q, content, &lines, &patterns);
    }
    ranges_extract(q, content, &lines, &requested)
}

/// A read with no pattern and no range selects the whole text.
pub(super) fn selects_whole(q: &LocalFetchQuery) -> bool {
    local_fetch_query::match_strings(q).is_empty() && local_fetch_query::line_ranges(q).is_empty()
}

impl Extraction {
    /// A read with no pattern and no range: the whole text, as given.
    pub(super) fn whole(text: String) -> Self {
        Self {
            text,
            source_lines: None,
            start: None,
            end: None,
            match_ranges: vec![],
            matched_lines: vec![],
            count: None,
            warnings: vec![],
            next: NextCalls::default(),
            blocks: vec![],
            match_limited: false,
        }
    }
}

/// Line ranges joined into one view: sorted, overlapping or adjacent ranges
/// merged, clamped to the file end; with `block`, each widened to its
/// enclosing declaration.
fn ranges_extract(
    q: &LocalFetchQuery,
    content: &str,
    lines: &[&str],
    requested: &[LineRange],
) -> Result<Extraction, ExtractionError> {
    let total = lines.len();
    let mut warnings = vec![];
    let mut kept: Vec<LineRange> = vec![];
    for range in requested {
        if range.start > total {
            warnings.push(format!(
                "Range {}-{} starts past the file end ({total} lines); skipped.",
                range.start, range.end
            ));
            continue;
        }
        if range.end > total {
            warnings.push(format!(
                "Range {}-{} adjusted to end at {total} (file end).",
                range.start, range.end
            ));
        }
        kept.push(LineRange {
            start: range.start.max(1),
            end: range.end.min(total),
        });
    }
    if kept.is_empty() {
        return Err(ExtractionError::new(
            ErrorCode::InvalidInput,
            format!("Requested lines start past the file end ({total} lines)"),
        ));
    }
    let mut rest = vec![];
    if local_fetch_query::block(q) {
        kept = super::block::widen_ranges(
            content,
            local_fetch_query::path(q),
            kept,
            &mut warnings,
            &mut rest,
        );
    }
    let windows = merge_ranges(kept);
    let (text, selected) = windows_text(lines, &windows);
    // A plain read that stops inside a declaration offers no reread: the
    // caller chose these lines, and `block` or `ranges` reads the rest.
    let next = NextCalls {
        continue_block: line_read(q, &uncovered(&merge_ranges(rest), &windows)).and_then(follow_up),
        ..NextCalls::default()
    };
    Ok(Extraction {
        text,
        source_lines: Some(selected),
        start: windows.first().map(|r| r.start),
        end: windows.last().map(|r| r.end),
        match_ranges: vec![],
        matched_lines: vec![],
        count: None,
        warnings,
        next,
        blocks: vec![],
        match_limited: false,
    })
}

/// The unseen lines of the first declaration in `blocks` (in hit order)
/// that `shown` does not cover whole; empty when every one is shown.
fn first_unseen(blocks: Vec<LineRange>, shown: &[LineRange]) -> Vec<LineRange> {
    blocks
        .into_iter()
        .map(|block| uncovered(&[block], shown))
        .find(|rest| !rest.is_empty())
        .unwrap_or_default()
}

/// Sorted ranges with overlapping or adjacent ones merged.
fn merge_ranges(ranges: Vec<LineRange>) -> Vec<LineRange> {
    crate::tools::line_spans::merge_spans(ranges.into_iter().map(|range| (range.start, range.end)))
        .into_iter()
        .map(|(start, end)| LineRange { start, end })
        .collect()
}

/// The text of `windows` (sorted, disjoint), with one omission marker
/// between non-adjacent windows; `selected` maps each view line to its
/// source line (markers map to [`OMISSION_LINE`]).
fn windows_text(lines: &[&str], windows: &[LineRange]) -> (String, Vec<usize>) {
    let mut selected = vec![];
    let mut text = String::new();
    let mut prev_end: Option<usize> = None;
    for r in windows {
        if let Some(prev) = prev_end.filter(|prev| r.start > prev + 1) {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&omission_marker(prev + 1, r.start - 1));
            selected.push(OMISSION_LINE);
        }
        for line in r.start..=r.end {
            text.push_str(lines[line - 1]);
            selected.push(line);
        }
        prev_end = Some(r.end);
    }
    (text, selected)
}
