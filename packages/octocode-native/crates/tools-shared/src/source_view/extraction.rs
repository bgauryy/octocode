use super::types::{ExtractionError, LineRange, NextCalls};
use crate::source_view::local_fetch_query;
use octocode_contract_types::{ErrorCode, LocalFetchQuery};

mod lines;
pub(crate) use lines::LineOffsets;
pub mod matches;
pub use matches::{FITS_ANY_BYTE_PAGE, MATCH_MARGIN};
pub mod search_lead;
pub mod spans;

/// Byte spans of `q`'s patterns in `content`, a text no redaction wrote
/// into a known range: a streamed window's.
pub fn match_spans(
    q: &LocalFetchQuery,
    content: &str,
) -> Result<super::types::RegexSpans, ExtractionError> {
    spans::match_spans(q, content, &[], &local_fetch_query::match_strings(q))
}

/// The 1-based lines of `content` a match of `q`'s patterns touches, as a
/// whole-file match read selects them, and whether a regex scan stopped at
/// its match limit. The whole-file answer a streamed scan must equal.
#[cfg(test)]
pub(crate) fn match_lines(
    q: &LocalFetchQuery,
    content: &str,
) -> Result<(Vec<usize>, bool), ExtractionError> {
    let found = match_spans(q, content)?;
    let lines = LineOffsets::new(content);
    Ok((spans::hit_lines(&lines, &found.spans), found.truncated))
}

#[cfg(test)]
mod canonical_tests;

use matches::match_extract;
pub use search_lead::{search_lead, text_search_query};
#[cfg(any(test, feature = "test-support"))]
pub use spans::{RUST_REGEX_RUNS, SpanBudgetOverride, literal_ranges};

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
    /// The bytes of `text` around each match that sits in a line too long
    /// for any byte page: sorted, disjoint, each inside its line. The byte
    /// walk of such a line shows them first.
    pub match_windows: Vec<(usize, usize)>,
    /// A list `matchString` read with hits: the entries that selected none
    /// of them, as written, in order.
    pub unmatched: Vec<String>,
}

/// The warning that names the entries of a list `matchString` that selected
/// nothing while the others did: each as written, in order.
pub(crate) fn unmatched_warning(q: &LocalFetchQuery, unmatched: &[String]) -> Option<String> {
    if unmatched.is_empty() {
        return None;
    }
    let total = local_fetch_query::match_strings(q).len();
    let terms = unmatched
        .iter()
        .map(|term| serde_json::to_string(term).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(", ");
    let scope = if local_fetch_query::has_ranges(q) {
        " in the requested ranges"
    } else {
        ""
    };
    Some(format!(
        "{} of {total} matchString entries matched nothing{scope}: {terms}.",
        unmatched.len()
    ))
}

/// View-line sentinel for an omission marker (source lines are 1-based).
pub const OMISSION_LINE: usize = 0;

/// Separates non-adjacent requested windows (ranges or match windows), so a
/// gap never reads as lines cut from a span the read asked for: a requested
/// span is never elided.
pub(crate) fn omission_marker(start: usize, end: usize) -> String {
    octocode_runtime_base::response::numbered::omission_marker(
        start,
        end,
        octocode_runtime_base::response::numbered::Omitted::NotRequested,
    )
}
pub fn line_count(s: &str) -> usize {
    octocode_engine::text::LineCount::of(s.as_bytes())
}
/// The view `q` selects of `content`; `written` are the byte ranges of it
/// its redactions wrote ([`super::written`]): no pattern matches their own
/// spelling.
pub fn extract(
    q: &LocalFetchQuery,
    content: &str,
    written: &[std::ops::Range<usize>],
) -> Result<Extraction, ExtractionError> {
    extract_indexed(q, content, written, &LineOffsets::new(content))
}

pub(super) fn extract_indexed(
    q: &LocalFetchQuery,
    content: &str,
    written: &[std::ops::Range<usize>],
    lines: &LineOffsets,
) -> Result<Extraction, ExtractionError> {
    if selects_whole(q) {
        return Ok(Extraction::whole(content.into()));
    }
    let patterns = local_fetch_query::match_strings(q);
    let requested = local_fetch_query::line_ranges(q);
    if !patterns.is_empty() {
        return match_extract(q, (content, written), lines, &patterns);
    }
    ranges_extract(q, content, lines, &requested)
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
            match_windows: vec![],
            unmatched: vec![],
        }
    }
}

/// Line ranges joined into one view: sorted, overlapping or adjacent ranges
/// merged, clamped to the file end; with `block`, each widened to its
/// enclosing declaration.
fn ranges_extract(
    q: &LocalFetchQuery,
    content: &str,
    lines: &LineOffsets,
    requested: &[LineRange],
) -> Result<Extraction, ExtractionError> {
    let total = lines.len();
    let mut warnings = vec![];
    let mut kept: Vec<LineRange> = vec![];
    for range in requested {
        if range.start > total {
            warnings.push(format!(
                "Range {}-{} starts past the file end ({}); skipped.",
                range.start,
                range.end,
                octocode_engine::text::counted(total, "line", "lines")
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
            format!(
                "Requested lines start past the file end ({})",
                octocode_engine::text::counted(total, "line", "lines")
            ),
        ));
    }
    let mut next = NextCalls::default();
    if local_fetch_query::block(q) {
        kept = super::block::widen_ranges(
            content,
            local_fetch_query::path(q),
            kept,
            &mut warnings,
            &mut next,
        );
    }
    let windows = merge_ranges(kept);
    let (text, selected, _) = placed_windows_text(content, lines, &windows);
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
        match_windows: vec![],
        unmatched: vec![],
    })
}

/// Sorted ranges with overlapping or adjacent ones merged.
fn merge_ranges(ranges: Vec<LineRange>) -> Vec<LineRange> {
    crate::line_spans::merge_spans(ranges.into_iter().map(|range| (range.start, range.end)))
        .into_iter()
        .map(|(start, end)| LineRange { start, end })
        .collect()
}

/// The text of `windows` (sorted, disjoint), with one omission marker
/// between non-adjacent windows; `selected` maps each view line to its
/// source line (markers map to [`OMISSION_LINE`]).
/// Third: the byte of the text each window starts at.
fn placed_windows_text(
    content: &str,
    lines: &LineOffsets,
    windows: &[LineRange],
) -> (String, Vec<usize>, Vec<usize>) {
    let mut selected = vec![];
    let mut text = String::new();
    let mut placed = Vec::with_capacity(windows.len());
    let mut prev_end: Option<usize> = None;
    for r in windows {
        if let Some(prev) = prev_end.filter(|prev| r.start > prev + 1) {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&omission_marker(prev + 1, r.start - 1));
            selected.push(OMISSION_LINE);
        }
        placed.push(text.len());
        text.push_str(lines.range(content, r.start, r.end));
        selected.extend(r.start..=r.end);
        prev_end = Some(r.end);
    }
    (text, selected, placed)
}

#[cfg(test)]
mod line_count_tests;
