use crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS;
use crate::num::{usize_of, usize_of_signed};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub use octocode_contract_types::{
    LocalFetchQuery, MatchString, MinifyMode, ReadCaseMode, ReadRegex, WindowUnit,
};

/// The engine works in `usize`; the wire contract (generated from the core
/// Zod schema) owns the field set and its JSON integer types.
pub(crate) mod local_fetch_query {
    use super::*;
    /// The one requested line span, when `ranges` holds exactly one.
    fn single_range(query: &LocalFetchQuery) -> Option<LineRange> {
        match line_ranges(query).as_slice() {
            [one] => Some(one.clone()),
            _ => None,
        }
    }
    pub(crate) fn start_line(query: &LocalFetchQuery) -> Option<usize> {
        single_range(query).map(|range| range.start)
    }
    pub(crate) fn end_line(query: &LocalFetchQuery) -> Option<usize> {
        single_range(query).map(|range| range.end)
    }
    /// Sets the read to the one line span `start..=end`.
    pub(crate) fn set_line_span(query: &mut LocalFetchQuery, start: usize, end: usize) {
        set_line_spans(query, &[(start, end)]);
    }
    /// Sets the read to the line spans `start..=end`, in order.
    pub(crate) fn set_line_spans(query: &mut LocalFetchQuery, spans: &[(usize, usize)]) {
        query.ranges = spans
            .iter()
            .filter_map(|(start, end)| format!("{start}-{end}").parse().ok())
            .collect();
    }
    /// `regex:"rust"` or `"pcre2"`: `matchString` is a regular expression.
    pub(crate) fn is_regex(query: &LocalFetchQuery) -> bool {
        matches!(query.regex, Some(ReadRegex::Rust | ReadRegex::Pcre2))
    }
    /// `regex:"pcre2"`: lookaround and backreferences, under a deadline.
    pub(crate) fn is_pcre2(query: &LocalFetchQuery) -> bool {
        query.regex == Some(ReadRegex::Pcre2)
    }
    /// Whether `pattern` matches case-sensitively. Omitted means smart, as
    /// in localSearch: sensitive only when the pattern has an uppercase
    /// letter.
    pub(crate) fn case_sensitive_for(query: &LocalFetchQuery, pattern: &str) -> bool {
        match query.case_mode {
            Some(ReadCaseMode::Sensitive) => true,
            Some(ReadCaseMode::Smart) | None => pattern.chars().any(char::is_uppercase),
            Some(ReadCaseMode::Insensitive) => false,
        }
    }
    /// Context lines per match; the contract bounds the request.
    pub(crate) fn context_lines(query: &LocalFetchQuery) -> Option<usize> {
        query.context_lines.map(usize_of_signed)
    }
    pub(crate) fn offset(query: &LocalFetchQuery) -> Option<usize> {
        query.offset.map(usize_of_signed)
    }
    pub(crate) fn window_length(query: &LocalFetchQuery) -> Option<usize> {
        query.length.map(|n| usize_of(n.get()))
    }
    /// Whether `matchString` hits must stand alone as words.
    pub(crate) fn whole_word(query: &LocalFetchQuery) -> bool {
        query.whole_word == Some(true)
    }

    /// Every `matchString` entry (a list matches any of them).
    pub(crate) fn match_strings(query: &LocalFetchQuery) -> Vec<&str> {
        match &query.match_string {
            None => Vec::new(),
            Some(MatchString::String(one)) => vec![one.as_str()],
            Some(MatchString::Array(list)) => list.iter().map(|one| one.as_str()).collect(),
        }
    }
    /// `ranges` parsed into 1-based inclusive line ranges, in request order.
    /// The contract admits only `start-end` digits, so a malformed entry
    /// (unreachable after validation) is skipped.
    pub(crate) fn line_ranges(query: &LocalFetchQuery) -> Vec<LineRange> {
        query
            .ranges
            .iter()
            .filter_map(|range| {
                let (start, end) = crate::tools::line_spans::parse_span(range)?;
                Some(LineRange { start, end })
            })
            .collect()
    }
    pub(crate) fn has_ranges(query: &LocalFetchQuery) -> bool {
        !query.ranges.is_empty()
    }
    /// The file's last window as `(start, end)`, as wide as the widest
    /// requested range, when every range starts past the file end. `None`
    /// when any range reaches into the file (that read is served, clamped),
    /// no range was requested, or the file has no line.
    pub(crate) fn last_window_span(
        query: &LocalFetchQuery,
        total_lines: usize,
    ) -> Option<(usize, usize)> {
        let requested = line_ranges(query);
        if total_lines == 0
            || requested.is_empty()
            || requested.iter().any(|range| range.start <= total_lines)
        {
            return None;
        }
        let width = requested
            .iter()
            .map(|range| range.end.saturating_sub(range.start) + 1)
            .max()?;
        Some((total_lines.saturating_sub(width - 1).max(1), total_lines))
    }
    /// `block:true`: widen windows to the enclosing declaration.
    pub(crate) fn block(query: &LocalFetchQuery) -> bool {
        query.block == Some(true)
    }
    /// Drop the multi-window selectors (`ranges`, `block`) from a derived query.
    pub(crate) fn clear_block_selectors(query: &mut LocalFetchQuery) {
        query.ranges = Vec::new();
        query.block = None;
    }
    pub(crate) fn path(query: &LocalFetchQuery) -> &str {
        query.path.as_str()
    }
    /// The requested transformed view; `None` reads the file as is.
    pub(crate) fn minify_mode(query: &LocalFetchQuery) -> Option<MinifyMode> {
        query.minify
    }
}

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
/// One window of the selected view; [`PaginationWire`] writes it.
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
/// A localFetch read a result leads to (a page, a restart, a lead); it
/// serializes through the one shared continuation builder ([`crate::response::result::Continuation`]).
#[derive(Clone, Debug, PartialEq)]
pub struct FetchCall {
    pub query: LocalFetchQuery,
}
impl Serialize for FetchCall {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::Error;
        let row = serde_json::to_value(&self.query).map_err(S::Error::custom)?;
        crate::response::result::Continuation::new(crate::id::ToolId::LocalFetch, row)
            .build()
            .serialize(serializer)
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct NextCalls {
    /// Recovery of a classified view without classification: the complete
    /// unfiltered view.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unfiltered: Option<FetchCall>,
    /// Exactly the unknown chunks, beside `unfiltered`, when they are
    /// narrower than the complete view.
    #[serde(
        rename = "unfilteredUnknown",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub unfiltered_unknown: Option<FetchCall>,
    /// The unknown chunks' `ranges` past what `unfilteredUnknown` reads
    /// (`unfilteredUnknown2`, `unfilteredUnknown3`, …): each a call of as
    /// many rows of `ranges` as one call takes, together reaching every
    /// unknown chunk.
    #[serde(flatten)]
    pub unfiltered_unknown_rest: serde_json::Map<String, serde_json::Value>,
    /// The same classified read judging the chunks its candidate budget
    /// left unjudged.
    #[serde(
        rename = "nextCandidatePage",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub next_candidate_page: Option<FetchCall>,
    #[serde(rename = "continueWindow", skip_serializing_if = "Option::is_none")]
    pub continue_window: Option<FetchCall>,
    #[serde(rename = "readBoundedLines", skip_serializing_if = "Option::is_none")]
    pub read_bounded_lines: Option<FetchCall>,
    /// The requested `ranges` after the span a streamed multi-range page
    /// stopped inside (that span reads on through `continueWindow`).
    #[serde(
        rename = "readOtherRanges",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub read_other_ranges: Option<FetchCall>,
    /// Offset-zero recovery for a page requested past the end of the view.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub restart: Option<FetchCall>,
    /// The whole matched lines a long-line match showed only byte windows of.
    #[serde(
        rename = "wholeLines",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub whole_lines: Option<FetchCall>,
    /// The rest of a declaration a `block` range stopped inside.
    #[serde(
        rename = "continueBlock",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub continue_block: Option<FetchCall>,
    /// The full default match window a byte-bounded window narrowed.
    #[serde(
        rename = "expandContext",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub expand_context: Option<FetchCall>,
    /// A structureSearch tree of a path that is a directory, not a file.
    #[serde(rename = "viewTree", skip_serializing_if = "Option::is_none", default)]
    pub view_tree: Option<serde_json::Value>,
    /// A structureSearch listing that finds a missing file by its stem.
    #[serde(rename = "findFile", skip_serializing_if = "Option::is_none", default)]
    pub find_file: Option<serde_json::Value>,
    /// A localSearch for a missed matchString in the file's directory.
    #[serde(rename = "findText", skip_serializing_if = "Option::is_none", default)]
    pub text_search: Option<serde_json::Value>,
    /// The same read matched case-insensitively, after a case-sensitive
    /// matchString selected no line.
    #[serde(
        rename = "ignoreCase",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub ignore_case: Option<FetchCall>,
    /// The same read with one input fixed: a match read without its
    /// `ranges`, after the ranges selected no line while the file has hits
    /// outside them, or a missing file's mistyped name corrected to its one
    /// close sibling.
    #[serde(rename = "read", skip_serializing_if = "Option::is_none", default)]
    pub read: Option<FetchCall>,
    /// The same read with each regex item that does not compile repaired.
    #[serde(rename = "repair", skip_serializing_if = "Option::is_none", default)]
    pub repair: Option<FetchCall>,
}

/// Which source a continuation read reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadSource {
    /// The version this page read: a page of the same result, or the
    /// evidence a classified page withheld. Bound to it by `snapshot`.
    Page,
    /// The file as it is when the read runs: a restart, a wider context, a
    /// lead.
    Current,
}

/// The localFetch reads of [`NextCalls`]: wire name, the source it reads,
/// field. The one list of them; both accessors come from it.
macro_rules! fetch_reads {
    ($($wire:literal $source:ident $field:ident),+ $(,)?) => {
        /// Every localFetch read among the continuations, with its wire
        /// name and the source it reads.
        fn reads(&self) -> [(&'static str, ReadSource, &Option<FetchCall>); { [$($wire),+].len() }] {
            [$(($wire, ReadSource::$source, &self.$field)),+]
        }

        /// [`Self::reads`], mutable.
        fn reads_slots(
            &mut self,
        ) -> [(&'static str, ReadSource, &mut Option<FetchCall>); { [$($wire),+].len() }] {
            [$(($wire, ReadSource::$source, &mut self.$field)),+]
        }
    };
}

impl NextCalls {
    fetch_reads! {
        "unfiltered" Page unfiltered,
        "unfilteredUnknown" Page unfiltered_unknown,
        "nextCandidatePage" Page next_candidate_page,
        "continueWindow" Page continue_window,
        "readBoundedLines" Current read_bounded_lines,
        "readOtherRanges" Current read_other_ranges,
        "restart" Current restart,
        "wholeLines" Page whole_lines,
        "continueBlock" Page continue_block,
        "expandContext" Current expand_context,
        "ignoreCase" Current ignore_case,
        "read" Current read,
        "repair" Current repair,
    }

    /// Add every continuation `other` carries; `self` keeps its own on a clash.
    pub fn absorb(&mut self, mut other: NextCalls) {
        for ((_, _, mine), (_, _, theirs)) in
            self.reads_slots().into_iter().zip(other.reads_slots())
        {
            if mine.is_none() {
                *mine = theirs.take();
            }
        }
        for (mine, theirs) in [
            (&mut self.view_tree, other.view_tree),
            (&mut self.find_file, other.find_file),
            (&mut self.text_search, other.text_search),
        ] {
            if mine.is_none() {
                *mine = theirs;
            }
        }
        if self.unfiltered_unknown_rest.is_empty() {
            self.unfiltered_unknown_rest = other.unfiltered_unknown_rest;
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == NextCalls::default()
    }

    /// The localFetch reads among these continuations.
    pub fn reads_mut(&mut self) -> impl Iterator<Item = &mut FetchCall> {
        self.reads_slots()
            .into_iter()
            .filter_map(|(_, _, read)| read.as_mut())
    }

    /// The reads of `source`.
    pub fn reads_of_mut(&mut self, source: ReadSource) -> impl Iterator<Item = &mut FetchCall> {
        self.reads_slots()
            .into_iter()
            .filter(move |(_, reads, _)| *reads == source)
            .filter_map(|(_, _, read)| read.as_mut())
    }

    /// Whether a continuation reads more of this result (a page), as opposed
    /// to an optional lead such as `read`; only a page makes the row
    /// partial. A call of another tool is a lead, never a page.
    pub fn leaves_more(&self) -> bool {
        let remaining = |name: &str| crate::id::is_remaining(crate::id::ToolId::LocalFetch, name);
        self.reads()
            .into_iter()
            .any(|(name, _, read)| read.is_some() && remaining(name))
            || self
                .unfiltered_unknown_rest
                .keys()
                .any(|name| remaining(name))
    }
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
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum PartialReason {
    /// The response share cut a `fullContent` read to its first window.
    #[serde(rename = "response-share")]
    ResponseShare,
    #[serde(rename = "full-content-source-size-limit")]
    FullContentSource,
    #[serde(rename = "security-selected-view-size-limit")]
    SecuritySelectedView,
    /// A regex scan stopped at its in-process match limit: the selection is a
    /// prefix of the matches.
    #[serde(rename = "regex-match-limit")]
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

/// What a read produced; the row's wire `status` is its [`Self::row_status`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadStatus {
    Success,
    Empty,
    Error,
}

impl ReadStatus {
    /// The row's wire `status`: none for a success.
    pub fn row_status(self) -> Option<&'static str> {
        match self {
            Self::Success => None,
            Self::Empty => Some("empty"),
            Self::Error => Some("error"),
        }
    }
}

/// Internal read result. Rust callers see every field; the wire form (the
/// manual `Serialize` below) omits values an agent can already derive from
/// another emitted field.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalFetchResult {
    pub path: String,
    pub status: ReadStatus,
    pub resource_missing: bool,
    pub source_sha256: Option<String>,
    pub content: Option<String>,
    pub selection: Option<octocode_contract_types::Selection>,
    pub content_view: Option<MinifyMode>,
    pub minify_fallback: Option<MinifyFallback>,
    pub error_code: Option<String>,
    pub error: Option<String>,
    pub warnings: Vec<String>,
    pub hints: Vec<String>,
    pub total_lines: Option<usize>,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub source_line_ranges: Vec<LineRange>,
    pub match_ranges: Vec<LineRange>,
    pub matched_lines: Vec<usize>,
    /// JSON and TOML sources: each matched line's key path (`matchKeys`).
    pub match_keys: Vec<(usize, String)>,
    /// The declarations a `block:true` match read widened hits to.
    pub blocks: Vec<DeclaredBlock>,
    pub selected_match_count: Option<usize>,
    pub modified: Option<String>,
    pub source_chars: Option<usize>,
    pub source_bytes: Option<usize>,
    pub returned_chars: Option<usize>,
    pub returned_bytes: Option<usize>,
    pub returned_lines: Option<usize>,
    pub pagination: Option<Pagination>,
    pub is_partial: Option<bool>,
    pub partial_reasons: Vec<PartialReason>,
    pub terminal_limit: Option<bool>,
    /// The requested offset is past the end of the selected view; emitted as
    /// `pagination.outOfRange`.
    pub out_of_range: bool,
    pub next: Option<NextCalls>,
}
impl LocalFetchResult {
    /// A result with `status` and every other field empty.
    pub fn blank(status: ReadStatus) -> Self {
        Self {
            path: String::new(),
            status,
            resource_missing: false,
            source_sha256: None,
            content: None,
            selection: None,
            content_view: None,
            minify_fallback: None,
            error_code: None,
            error: None,
            warnings: vec![],
            hints: vec![],
            total_lines: None,
            start_line: None,
            end_line: None,
            source_line_ranges: vec![],
            match_ranges: vec![],
            matched_lines: vec![],
            match_keys: vec![],
            blocks: vec![],
            selected_match_count: None,
            modified: None,
            source_chars: None,
            source_bytes: None,
            returned_chars: None,
            returned_bytes: None,
            returned_lines: None,
            pagination: None,
            is_partial: None,
            partial_reasons: vec![],
            terminal_limit: None,
            out_of_range: false,
            next: None,
        }
    }

    /// An error row for the read of `path`: every row names its file.
    pub fn error(path: String, code: &str, message: String) -> Self {
        Self {
            path,
            error_code: Some(code.into()),
            error: Some(message),
            ..Self::blank(ReadStatus::Error)
        }
    }

    /// Ends the row at a limit no continuation lifts, as
    /// [`crate::response::paging::terminal_limit`] marks a JSON row: partial for
    /// `reason`, and `terminalLimit`.
    pub(super) fn terminal_limit(&mut self, reason: PartialReason) {
        self.is_partial = Some(true);
        self.terminal_limit = Some(true);
        if !self.partial_reasons.contains(&reason) {
            self.partial_reasons.push(reason);
        }
    }
}

impl LocalFetchResult {
    /// The emitted `[startLine, endLine]` span equals the single emitted
    /// source-line range, so repeating it is redundant.
    fn window_is_source_range(&self) -> bool {
        match (
            self.start_line,
            self.end_line,
            self.source_line_ranges.as_slice(),
        ) {
            (Some(start), Some(end), [range]) => range.start == start && range.end == end,
            _ => false,
        }
    }
    /// The matched line ranges this page shows: each clipped to the page's
    /// source lines, so a later page never restates hits an earlier one sent.
    /// Without source lines (a byte page) only the first page carries them.
    fn page_match_ranges(&self) -> Vec<LineRange> {
        if self.source_line_ranges.is_empty() {
            let first_page = self.pagination.as_ref().is_none_or(|page| page.offset == 0);
            return if first_page {
                self.match_ranges.clone()
            } else {
                Vec::new()
            };
        }
        self.match_ranges
            .iter()
            .flat_map(|hit| {
                self.source_line_ranges.iter().filter_map(move |shown| {
                    let start = hit.start.max(shown.start);
                    let end = hit.end.min(shown.end);
                    (start <= end).then_some(LineRange { start, end })
                })
            })
            .collect()
    }
    fn match_ranges_are_redundant(&self, ranges: &[LineRange]) -> bool {
        ranges == self.source_line_ranges.as_slice()
            || match (self.start_line, self.end_line, ranges) {
                (Some(start), Some(end), [range]) => range.start == start && range.end == end,
                _ => false,
            }
    }
    /// A single complete page (offset 0, nothing more) carries no information
    /// beyond the top-level totals and the absence of `next`.
    fn pagination_is_redundant(&self) -> bool {
        !self.out_of_range
            && self
                .pagination
                .as_ref()
                .is_none_or(|page| page.offset == 0 && !page.has_more)
    }
}

/// Wire view of [`Pagination`]: the view total in `unit`. A line view's
/// total is stated only when it differs from the top-level `totalLines`; a
/// byte view always states it, because `sourceBytes` is debug-only.
struct PaginationWire<'a> {
    page: &'a Pagination,
    source_lines: Option<usize>,
    out_of_range: bool,
}
impl Serialize for PaginationWire<'_> {
    /// The one window shape ([`crate::response::pages::SpanFacts`]), with
    /// `total` dropped when it repeats `totalLines` and `outOfRange` added.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let page = self.page;
        let (unit, total) = match page.unit {
            WindowUnit::Lines => ("lines", page.total_lines),
            WindowUnit::Bytes => ("bytes", page.total_bytes),
        };
        let mut block =
            crate::response::pages::SpanFacts::window(unit, page.offset, page.length, total)
                .to_value();
        if let Some(block) = block.as_object_mut() {
            if page.unit == WindowUnit::Lines && self.source_lines == Some(total) {
                block.shift_remove("total");
            }
            if self.out_of_range {
                block.insert("outOfRange".into(), serde_json::Value::Bool(true));
            }
        }
        block.serialize(serializer)
    }
}

fn opt<M: serde::ser::SerializeMap, T: Serialize>(
    map: &mut M,
    key: &'static str,
    value: &Option<T>,
) -> Result<(), M::Error> {
    match value {
        Some(value) => map.serialize_entry(key, value),
        None => Ok(()),
    }
}

fn list<M: serde::ser::SerializeMap, T: Serialize>(
    map: &mut M,
    key: &'static str,
    values: &[T],
) -> Result<(), M::Error> {
    if values.is_empty() {
        Ok(())
    } else {
        map.serialize_entry(key, values)
    }
}

impl LocalFetchResult {
    /// Where the returned text sits in the source: totals, the line span,
    /// and the hits on this page, each only when not derivable from another.
    fn serialize_anchors<M: serde::ser::SerializeMap>(&self, map: &mut M) -> Result<(), M::Error> {
        opt(map, "totalLines", &self.total_lines)?;
        if !self.window_is_source_range() {
            opt(map, "startLine", &self.start_line)?;
            opt(map, "endLine", &self.end_line)?;
        }
        list(map, "sourceLineRanges", &self.source_line_ranges)?;
        let match_ranges = self.page_match_ranges();
        if !self.match_ranges_are_redundant(&match_ranges) {
            list(map, "matchRanges", &match_ranges)?;
        }
        list(map, "matchedLines", &self.matched_lines)?;
        if !self.match_keys.is_empty() {
            map.serialize_entry(
                "matchKeys",
                &crate::tools::match_keys::MatchKeys(&self.match_keys),
            )?;
        }
        if self.selected_match_count != Some(self.matched_lines.len()) {
            opt(map, "selectedMatchCount", &self.selected_match_count)?;
        }
        Ok(())
    }

    /// Explanation counters: the contract classes them verbose, so the
    /// response keeps them only with `extras: ["stats"]`.
    fn serialize_counters<M: serde::ser::SerializeMap>(&self, map: &mut M) -> Result<(), M::Error> {
        opt(map, "modified", &self.modified)?;
        opt(map, "sourceChars", &self.source_chars)?;
        opt(map, "sourceBytes", &self.source_bytes)?;
        opt(map, "returnedChars", &self.returned_chars)?;
        opt(map, "returnedBytes", &self.returned_bytes)?;
        opt(map, "returnedLines", &self.returned_lines)
    }
}

impl Serialize for LocalFetchResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        if !self.path.is_empty() {
            map.serialize_entry("path", &self.path)?;
        }
        opt(&mut map, "content", &self.content)?;
        opt(&mut map, "selection", &self.selection)?;
        // The file as is is the default view; only a transformed view is news.
        opt(&mut map, "contentView", &self.content_view)?;
        opt(&mut map, "minifyFallback", &self.minify_fallback)?;
        opt(&mut map, "errorCode", &self.error_code)?;
        opt(&mut map, "error", &self.error)?;
        list(&mut map, "warnings", &self.warnings)?;
        list(&mut map, "hints", &self.hints)?;
        self.serialize_anchors(&mut map)?;
        list(&mut map, "blocks", &self.blocks)?;
        self.serialize_counters(&mut map)?;
        if !self.pagination_is_redundant()
            && let Some(page) = &self.pagination
        {
            map.serialize_entry(
                "pagination",
                &PaginationWire {
                    page,
                    source_lines: self.total_lines,
                    out_of_range: self.out_of_range,
                },
            )?;
        }
        opt(&mut map, "isPartial", &self.is_partial)?;
        list(&mut map, "partialReasons", &self.partial_reasons)?;
        opt(&mut map, "terminalLimit", &self.terminal_limit)?;
        opt(&mut map, "next", &self.next)?;
        map.end()
    }
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRead {
    pub canonical: PathBuf,
    pub display: String,
    /// What a row names the read file by: the canonical path relative to
    /// the workspace when inside it, else the canonical path; a path read
    /// through a symlink keeps the requested name.
    pub named: String,
    /// The file a path read through a symlink resolved to, named as a
    /// canonical path is.
    pub target: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathFailure {
    /// The row's `errorCode`: the policy's shared code, else the access code.
    pub code: String,
    pub message: String,
    pub safe_path: Option<String>,
    pub resource_missing: bool,
    /// The missing path lies in a sparse git checkout, so it may exist
    /// upstream outside the checked-out paths.
    pub sparse_checkout: bool,
    /// The path names something other than a regular file.
    pub not_a_file: Option<NotAFile>,
    /// Where a missing file's listing searches: the workspace root (`.`)
    /// for a path inside it, else the allowed root that holds the path,
    /// else its closest admitted ancestor directory.
    pub search_root: Option<String>,
    /// A missing path's `viewTree` lead ([`missing_path_lead`]): the tree
    /// of its nearest existing admitted parent, where a mistyped name's
    /// siblings show.
    ///
    /// [`missing_path_lead`]: crate::response::result::missing_path_lead
    pub view_tree: Option<serde_json::Value>,
    /// The path a missing file's name was likely mistyped for, written as
    /// the request wrote its directory: a readable file in the same
    /// directory ([`close_name`]).
    ///
    /// [`close_name`]: crate::tools::leads::find_file::close_name
    pub corrected: Option<String>,
}
/// What a path names instead of a regular file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotAFile {
    Directory,
    /// A FIFO, socket or device: never readable file text.
    Special,
}

pub trait PathAccess {
    fn validate_read(&self, path: &Path) -> Result<ValidatedRead, Box<PathFailure>>;
}
/// Byte spans one regex selected, in source order. `truncated` says the scan
/// stopped at its match limit with further matches unseen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegexSpans {
    pub spans: Vec<(usize, usize)>,
    pub truncated: bool,
}

/// Why a view selection failed; it picks the row's `errorCode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtractionErrorKind {
    /// A `matchString` pattern does not compile.
    InvalidPattern,
    /// The regex engine the pattern needs cannot run it.
    ExecutionFailed,
    /// The request selects nothing the file has (ranges past its end).
    InvalidInput,
}

impl ExtractionErrorKind {
    pub fn error_code(self) -> &'static str {
        match self {
            Self::InvalidPattern => "invalidPattern",
            Self::ExecutionFailed => "executionFailed",
            Self::InvalidInput => "invalidInput",
        }
    }
}

/// A failed view selection: its kind and the reason the message carries
/// after the kind's lead (`Invalid regex pattern: <detail>`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractionError {
    pub kind: ExtractionErrorKind,
    pub detail: String,
}

impl ExtractionError {
    pub fn new(kind: ExtractionErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    /// The row's error message.
    pub fn message(&self) -> String {
        match self.kind {
            ExtractionErrorKind::InvalidPattern => {
                format!("Invalid regex pattern: {}", self.detail)
            }
            ExtractionErrorKind::ExecutionFailed => {
                format!("Regex execution unavailable: {}", self.detail)
            }
            ExtractionErrorKind::InvalidInput => self.detail.clone(),
        }
    }
}

impl PathAccess for crate::policy::path::PathPolicy {
    fn validate_read(&self, path: &Path) -> Result<ValidatedRead, Box<PathFailure>> {
        crate::policy::path::PathPolicy::validate_read(self, path)
            .map(|validated| {
                let resolved = self
                    .workspace_relative(&validated.canonical)
                    .unwrap_or_else(|| validated.canonical.to_string_lossy().into_owned());
                // The request is a symlink, or names its file through a
                // directory link inside the workspace (`link/a.rs`): a
                // component of the path, as `.` and `..` fold it, is one.
                // Links above the workspace (`/tmp`) rename nothing, and a
                // name that differs from the file's only in letter case (a
                // case-insensitive file system) is no link.
                let lexical = self.expand_and_resolve(path);
                let is_link = |at: &Path| {
                    fs::symlink_metadata(at).is_ok_and(|meta| meta.file_type().is_symlink())
                };
                let through_link = is_link(&lexical)
                    || lexical
                        .ancestors()
                        .skip(1)
                        .take_while(|dir| {
                            self.workspace_relative(dir).is_some_and(|name| name != ".")
                        })
                        .any(is_link);
                let (named, target) = if through_link {
                    (path.to_string_lossy().into_owned(), Some(resolved))
                } else {
                    (resolved, None)
                };
                ValidatedRead {
                    named,
                    target,
                    canonical: validated.canonical,
                    display: validated.display,
                }
            })
            .map_err(|error| {
                let missing = error.code == crate::policy::PolicyErrorCode::NotFound;
                let resolved = self.expand_and_resolve(path);
                Box::new(PathFailure {
                    code: error.local_error_code("fileAccessFailed").into(),
                    message: error.message,
                    safe_path: error.safe_path,
                    resource_missing: missing,
                    sparse_checkout: missing && in_sparse_checkout(&resolved),
                    not_a_file: (error.code == crate::policy::PolicyErrorCode::NotRegular).then(
                        || match resolved.is_dir() {
                            true => NotAFile::Directory,
                            false => NotAFile::Special,
                        },
                    ),
                    search_root: missing
                        .then(|| missing_search_root(self, path, &resolved))
                        .flatten(),
                    view_tree: missing
                        .then(|| {
                            crate::response::result::missing_path_lead(
                                self,
                                &path.to_string_lossy(),
                            )
                        })
                        .flatten()
                        .and_then(|mut next| next.get_mut("viewTree").map(serde_json::Value::take)),
                    corrected: missing
                        .then(|| corrected_path(self, path, &resolved))
                        .flatten(),
                })
            })
    }
}

/// The readable file a missing file's name was likely mistyped for: the
/// one close name in its admitted parent directory ([`close_name`]), in the
/// requested path's own directory spelling. Only a path the policy admits
/// reading is named, so a recovery never reveals a withheld file.
///
/// [`close_name`]: crate::tools::leads::find_file::close_name
fn corrected_path(
    policy: &crate::policy::path::PathPolicy,
    requested: &Path,
    resolved: &Path,
) -> Option<String> {
    requested.file_name()?;
    let parent = policy.validate(resolved.parent()?).ok()?;
    let name = resolved.file_name()?.to_string_lossy();
    let close = crate::tools::leads::find_file::close_name(&parent.canonical, &name)?;
    let corrected = requested.with_file_name(close);
    crate::policy::path::PathPolicy::validate_read(policy, &corrected).ok()?;
    Some(corrected.to_string_lossy().into_owned())
}

/// The admitted directory a missing file's listing searches from: the
/// workspace root, or the most specific allowed root holding `resolved`,
/// so a file moved to another tree is found; the closest admitted ancestor
/// when no root holds it.
fn missing_search_root(
    policy: &crate::policy::path::PathPolicy,
    requested: &Path,
    resolved: &Path,
) -> Option<String> {
    if policy.workspace_relative(resolved).is_some() {
        return Some(".".to_owned());
    }
    policy
        .allowed_roots()
        .into_iter()
        .filter(|root| resolved.starts_with(root) && root.is_dir())
        .max_by_key(|root| root.components().count())
        .map(|root| root.to_string_lossy().into_owned())
        .or_else(|| policy.nearest_existing_dir(&requested.to_string_lossy()))
}

/// Whether `path` lies in a git worktree with sparse checkout enabled (as a
/// `ghCloneRepo` clone with `path` is).
fn in_sparse_checkout(path: &Path) -> bool {
    path.ancestors()
        .skip(1)
        .map(|dir| dir.join(".git"))
        .find(|git| git.is_dir())
        .is_some_and(|git| {
            git.join("info/sparse-checkout").is_file()
                && std::fs::read_to_string(git.join("config")).is_ok_and(|config| {
                    config.lines().any(|line| {
                        line.split_whitespace()
                            .collect::<String>()
                            .eq_ignore_ascii_case("sparseCheckout=true")
                    })
                })
        })
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
mod line_read_tests {
    use super::{LineRange, LocalFetchQuery, line_read, local_fetch_query, test_local_fetch_query};
    use crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS;

    /// Every lead `line_read` builds (continueBlock,
    /// readBoundedLines) spells lines with the published `ranges`, never
    /// startLine/endLine, and clears a span the caller sent.
    #[test]
    fn line_reads_spell_published_ranges() {
        let mut q = test_local_fetch_query();
        local_fetch_query::set_line_span(&mut q, 1, 9);
        let wire = |query: LocalFetchQuery| serde_json::to_value(query).expect("query");
        let one = wire(line_read(&q, &[LineRange { start: 57, end: 69 }]).expect("read"));
        assert_eq!(one["ranges"], serde_json::json!(["57-69"]), "{one}");
        assert!(
            one.get("startLine").is_none() && one.get("endLine").is_none(),
            "{one}"
        );
        let many = (0..=RANGES_MAX_ITEMS)
            .map(|i| LineRange {
                start: 10 * i + 1,
                end: 10 * i + 2,
            })
            .collect::<Vec<_>>();
        let span = wire(line_read(&q, &many).expect("read"));
        assert_eq!(span["ranges"], serde_json::json!(["1-102"]), "{span}");
        assert!(span.get("startLine").is_none(), "{span}");
    }

    #[test]
    fn read_range_cap_comes_from_the_contract() {
        let declared = crate::contracts::query_schema_number(
            crate::id::ToolId::LocalFetch,
            None,
            "ranges",
            "maxItems",
        )
        .expect("localFetch ranges declares maxItems");
        assert_eq!(RANGES_MAX_ITEMS as u64, declared);
    }
}

#[cfg(test)]
mod next_calls_tests {
    use super::{FetchCall, NextCalls, ReadSource};

    /// The read list names each read as it serializes, covers every read
    /// field, and decides `leaves_more` and the source binding alone.
    #[test]
    fn the_read_list_is_every_read_by_its_wire_name() {
        let call = FetchCall {
            query: serde_json::from_value(serde_json::json!({"path": "a.txt"})).expect("query"),
        };
        let mut next = NextCalls::default();
        for (_, _, read) in next.reads_slots() {
            *read = Some(call.clone());
        }
        let wire = serde_json::to_value(&next).expect("next");
        let names = next.reads().map(|(name, _, _)| name);
        assert_eq!(
            wire.as_object()
                .expect("object")
                .keys()
                .collect::<Vec<_>>()
                .len(),
            names.len(),
            "{wire}"
        );
        for name in names {
            assert!(
                wire.get(name).is_some(),
                "{name} serializes as listed: {wire}"
            );
        }
        assert!(next.leaves_more());
        let lead = NextCalls {
            read: Some(call.clone()),
            restart: Some(call.clone()),
            ..NextCalls::default()
        };
        assert!(
            !lead.leaves_more(),
            "a lead or a restart reads no more of the result"
        );
        let mut page = NextCalls {
            continue_window: Some(call.clone()),
            restart: Some(call),
            ..NextCalls::default()
        };
        assert_eq!(page.reads_of_mut(ReadSource::Page).count(), 1);
    }
}
