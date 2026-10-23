use crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS;
use octocode_contract_types::ErrorCode;
use serde::{Deserialize, Serialize};

pub use octocode_contract_types::{
    LocalFetchQuery, MatchString, MinifyMode, ReadCaseMode, ReadRegex, WindowUnit,
};

pub(crate) mod local_fetch_query;
mod next_calls;
mod path_access;
mod result;

pub use next_calls::{FetchCall, NextCalls, ReadSource};
pub use path_access::{NotAFile, PathAccess, PathFailure, ValidatedRead};
pub use result::{LocalFetchResult, ReadStatus};

/// Default line-page safety ceiling. The byte budget bounds ordinary pages;
/// short-line files can fill it without an additional fixed row limit.
pub const DEFAULT_LINE_CHUNK: usize = crate::contracts::query_limits::local_fetch::LENGTH_MAXIMUM;

/// Lines an unanchored read of a view longer than one line page returns
/// before paging on (contract `localFetch.semanticPolicy.headLines`).
pub const HEAD_LINES: usize = crate::id::local_fetch_policy::HEAD_LINES;

/// Encodes an engine `usize` as a positive wire integer.
pub(crate) fn wire_positive(value: usize) -> Option<std::num::NonZeroU64> {
    std::num::NonZeroU64::new(u64::try_from(value).unwrap_or(u64::MAX))
}

/// Encodes an engine `usize` as a non-negative wire integer.
pub(crate) fn wire_count(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineRange {
    /// First line (1-based), named like every span's start.
    #[serde(rename = "line")]
    pub start: usize,
    /// Last line (1-based, inclusive).
    #[serde(rename = "endLine")]
    pub end: usize,
}
/// One window of the selected view; `result::PaginationWire` writes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Pagination {
    pub unit: WindowUnit,
    pub offset: usize,
    pub length: usize,
    /// The requested window, in `unit`; never sent (the caller set it).
    pub window: usize,
    pub total_lines: usize,
    pub total_bytes: usize,
    pub has_more: bool,
    pub next_offset: Option<usize>,
    /// A byte page that finished an oversized line: the 0-based line the
    /// next page resumes line paging at, so only that line is byte-chunked.
    pub resume_line: Option<usize>,
}

/// A source-line read of `ranges` derived from `q`: the extraction selectors,
/// paging cursor, and `block` are cleared so the read returns exactly those
/// lines, spelled as the published `ranges`. More ranges than one read
/// holds read as one span from the first start to the last end.
pub fn line_read(q: &LocalFetchQuery, ranges: &[LineRange]) -> Option<LocalFetchQuery> {
    let (first, last) = (ranges.first()?, ranges.last()?);
    let mut query = q.clone();
    local_fetch_query::clear_block_selectors(&mut query);
    query.match_string = None;
    query.regex = None;
    query.case_mode = None;
    query.whole_word = None;
    query.context_lines = None;
    query.full_content = None;
    query.offset = None;
    query.unit = None;
    query.length = None;
    query.snapshot = None;
    let span = [LineRange {
        start: first.start,
        end: last.end,
    }];
    let ranges = if ranges.len() > RANGES_MAX_ITEMS {
        &span[..]
    } else {
        ranges
    };
    query.ranges = ranges
        .iter()
        .filter_map(|range| format!("{}-{}", range.start, range.end).parse().ok())
        .collect();
    Some(query)
}

/// `wanted` minus every line in `shown`: the sorted, disjoint ranges of
/// `wanted` lines a view did not return.
pub fn uncovered(wanted: &[LineRange], shown: &[LineRange]) -> Vec<LineRange> {
    let mut rest = vec![];
    for range in wanted {
        let mut start = range.start;
        for seen in shown {
            if seen.end < start || seen.start > range.end {
                continue;
            }
            if seen.start > start {
                rest.push(LineRange {
                    start,
                    end: seen.start - 1,
                });
            }
            start = start.max(seen.end + 1);
        }
        if start <= range.end {
            rest.push(LineRange {
                start,
                end: range.end,
            });
        }
    }
    rest.sort_by_key(|range| range.start);
    rest.dedup();
    rest
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MinifyFallback {
    pub requested: MinifyMode,
    pub applied: MinifyMode,
    pub reason: String,
}
/// Why a row does not cover its requested scope beyond what pagination
/// states (a share-cut page is `hasMore` with `next.continueWindow`, no reason).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PartialReason {
    #[serde(rename = "fullContentSourceSizeLimit")]
    FullContentSource,
    #[serde(rename = "securitySelectedViewSizeLimit")]
    SecuritySelectedView,
    /// A regex scan stopped at its in-process match limit: the selection is a
    /// prefix of the matches.
    RegexMatchLimit,
}

/// A declaration a `block:true` read widened a hit to: its name, name line
/// and last line, an lspSearch anchor without parsing the text.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclaredBlock {
    pub symbol_name: String,
    pub line: usize,
    pub end_line: usize,
}

/// What the caller knows about a read besides its bytes.
#[derive(Clone, Debug, Default)]
pub struct SourceFacts {
    /// The file's modification time (ISO 8601), when the caller has one.
    pub modified: Option<String>,
    /// The row's share of the response window (`response::read_share`):
    /// the read fills it and its row fits it; a whole-file view larger than
    /// it returns its first page and `next.continueWindow`.
    pub window: Option<usize>,
    /// Chars the caller's row adds around this reader's row (a GitHub
    /// read's repository identity and its calls' owner, repo and ref): the
    /// page fits `window` less them.
    pub beside: usize,
    /// The bytes begin inside the file, past its first line (a streamed
    /// window): a U+FEFF leading them is text, not the file's byte-order
    /// mark.
    pub interior: bool,
    /// The bytes are a window of a larger byte view (a streamed source's
    /// range): where they start in it and its whole size, so a byte page's
    /// offsets, totals and continuation are the view's.
    pub byte_origin: Option<ByteOrigin>,
}

/// A byte window's place in the byte view it was cut from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ByteOrigin {
    /// The view byte the window's first byte is.
    pub base: usize,
    /// The view's size in bytes.
    pub total: usize,
}

impl ByteOrigin {
    /// `q`, a byte read of the view, as the same read of the window.
    pub(super) fn within(self, q: &LocalFetchQuery) -> LocalFetchQuery {
        let mut window = q.clone();
        if window.unit == Some(WindowUnit::Bytes) {
            let offset = local_fetch_query::offset(q)
                .unwrap_or(0)
                .saturating_sub(self.base);
            window.offset = Some(wire_count(offset));
        }
        window
    }

    /// A byte page cut from the window, restated in the view's coordinates:
    /// its offset, the view's size, and whether (and where) more follows.
    /// A page that stopped at an oversized line's end resumes by lines, so
    /// its remainder is the view's whenever the line is not the last.
    pub(super) fn place(self, page: &mut Pagination) {
        page.offset += self.base;
        page.total_bytes = self.total;
        let end = page.offset + page.length;
        if page.resume_line.is_none() {
            page.has_more = end < self.total;
        }
        page.next_offset = page.has_more.then_some(end);
    }
}

/// Byte spans one regex selected, in source order. `truncated` says the scan
/// stopped at its match limit with further matches unseen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegexSpans {
    pub spans: Vec<(usize, usize)>,
    pub truncated: bool,
}

/// A failed view selection: the row's `errorCode` and the reason the
/// message carries after the code's lead (`Invalid regex pattern: <detail>`).
/// The codes are `invalidPattern` (a `matchString` pattern does not
/// compile), `executionFailed` (the regex engine the pattern needs cannot
/// run it) and `invalidInput` (the request selects nothing the file has).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractionError {
    pub code: ErrorCode,
    pub detail: String,
}

impl ExtractionError {
    pub fn new(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    /// The row's error message.
    pub fn message(&self) -> String {
        match self.code {
            ErrorCode::InvalidPattern => format!("Invalid regex pattern: {}", self.detail),
            ErrorCode::ExecutionFailed => {
                format!("Regex execution unavailable: {}", self.detail)
            }
            _ => self.detail.clone(),
        }
    }
}

#[cfg(test)]
/// Minimal valid wire query; tests override the fields they exercise.
pub(crate) fn test_local_fetch_query() -> LocalFetchQuery {
    serde_json::from_value(
        serde_json::json!({"path": "_", "mainGoal": "test", "reasoning": "test"}),
    )
    .expect("minimal localFetch query")
}

#[cfg(test)]
mod tests;
