use crate::tools::num::{usize_of, usize_of_signed};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use crate::contracts::tool_types::{
    LocalFetchQuery, MatchString, MinifyMode, ReadCaseMode, ReadRegex, WindowUnit,
};

/// A single literal `matchString` (tests and continuations build one).
impl std::str::FromStr for MatchString {
    type Err = <crate::contracts::tool_types::MatchStringString as std::str::FromStr>::Err;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(MatchString::String)
    }
}

impl MatchString {
    /// The searched text as one line: a list joins its entries with ` | `.
    pub fn display(&self) -> String {
        match self {
            MatchString::String(one) => one.to_string(),
            MatchString::Array(list) => list
                .iter()
                .map(|one| one.as_str())
                .collect::<Vec<_>>()
                .join(" | "),
        }
    }
}

/// The engine works in `usize`; the wire contract (generated from the core
/// Zod schema) owns the field set and its JSON integer types.
impl LocalFetchQuery {
    /// The one requested line span, when `ranges` holds exactly one.
    fn single_range(&self) -> Option<LineRange> {
        match self.line_ranges().as_slice() {
            [one] => Some(one.clone()),
            _ => None,
        }
    }
    pub fn start_line(&self) -> Option<usize> {
        self.single_range().map(|range| range.start)
    }
    pub fn end_line(&self) -> Option<usize> {
        self.single_range().map(|range| range.end)
    }
    /// Sets the read to the one line span `start..=end`.
    pub fn set_line_span(&mut self, start: usize, end: usize) {
        self.ranges = format!("{start}-{end}").parse().ok().into_iter().collect();
    }
    /// `regex:"rust"` or `"pcre2"`: `matchString` is a regular expression.
    pub fn is_regex(&self) -> bool {
        matches!(self.regex, Some(ReadRegex::Rust | ReadRegex::Pcre2))
    }
    /// `regex:"pcre2"`: lookaround and backreferences, under a deadline.
    pub fn is_pcre2(&self) -> bool {
        self.regex == Some(ReadRegex::Pcre2)
    }
    /// Whether `pattern` matches case-sensitively. Omitted means smart, as
    /// in localSearch: sensitive only when the pattern has an uppercase
    /// letter.
    pub fn case_sensitive_for(&self, pattern: &str) -> bool {
        match self.case_mode {
            Some(ReadCaseMode::Sensitive) => true,
            Some(ReadCaseMode::Smart) | None => pattern.chars().any(char::is_uppercase),
            Some(ReadCaseMode::Insensitive) => false,
        }
    }
    /// Context lines per match; the contract bounds the request.
    pub fn context_lines(&self) -> Option<usize> {
        self.context_lines.map(usize_of_signed)
    }
    pub fn context_bytes(&self) -> Option<usize> {
        self.context_bytes.map(usize_of_signed)
    }
    pub fn offset(&self) -> Option<usize> {
        self.offset.map(usize_of_signed)
    }
    pub fn window_length(&self) -> Option<usize> {
        self.length.map(|n| usize_of(n.get()))
    }
    /// Every `matchString` entry (a list matches any of them).
    pub fn match_strings(&self) -> Vec<&str> {
        match &self.match_string {
            None => Vec::new(),
            Some(MatchString::String(one)) => vec![one.as_str()],
            Some(MatchString::Array(list)) => list.iter().map(|one| one.as_str()).collect(),
        }
    }
    /// `ranges` parsed into 1-based inclusive line ranges, in request order.
    /// The contract admits only `start-end` digits, so a malformed entry
    /// (unreachable after validation) is skipped.
    pub fn line_ranges(&self) -> Vec<LineRange> {
        self.ranges
            .iter()
            .filter_map(|range| {
                let (start, end) = crate::tools::line_spans::parse_span(range)?;
                Some(LineRange { start, end })
            })
            .collect()
    }
    pub fn has_ranges(&self) -> bool {
        !self.ranges.is_empty()
    }
    /// `block:true`: widen windows to the enclosing declaration.
    pub fn block(&self) -> bool {
        self.block == Some(true)
    }
    /// Drop the multi-window selectors (`ranges`, `block`) from a derived query.
    pub fn clear_block_selectors(&mut self) {
        self.ranges = Vec::new();
        self.block = None;
    }
    pub fn path(&self) -> &str {
        self.path.as_str()
    }
    pub fn minify_mode(&self) -> MinifyMode {
        self.minify.unwrap_or(MinifyMode::None)
    }
}

/// Default line-page request. The 16 KiB page budget, not a line count,
/// bounds each page, so short-line files are not split into many tiny calls.
pub const DEFAULT_LINE_CHUNK: usize = 2000;

/// Lines from which a file counts as large: a read of it with no anchor
/// returns [`HEAD_LINES`] and a locate handoff instead of a full first page.
pub const LARGE_READ_LINES: usize = 2_000;

/// Lines an unanchored read of a large file returns before paging on.
pub const HEAD_LINES: usize = 50;

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
    pub start: usize,
    pub end: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    pub unit: WindowUnit,
    pub offset: usize,
    pub length: usize,
    /// The requested window, in `unit`; never sent (the caller set it).
    pub window: usize,
    pub total_lines: usize,
    pub total_bytes: usize,
    pub has_more: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,
    /// A byte page that finished an oversized line: the 0-based line the
    /// next page resumes line paging at, so only that line is byte-chunked.
    #[serde(skip)]
    pub resume_line: Option<usize>,
}
/// A typed localFetch follow-up; it serializes through the one shared
/// continuation builder ([`crate::tools::result::Continuation`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Continuation {
    pub query: LocalFetchQuery,
    pub reason: Option<String>,
}
impl Serialize for Continuation {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::Error;
        let row = serde_json::to_value(&self.query).map_err(S::Error::custom)?;
        let mut call =
            crate::tools::result::Continuation::new(crate::tools::id::ToolId::LocalFetch, row);
        if let Some(why) = &self.reason {
            call = call.why(why.clone());
        }
        call.build().serialize(serializer)
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct NextCalls {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#continue: Option<Continuation>,
    #[serde(rename = "readBoundedLines", skip_serializing_if = "Option::is_none")]
    pub read_bounded_lines: Option<Continuation>,
    /// Offset-zero recovery for a page requested past the end of the view.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub restart: Option<Continuation>,
    /// The whole matched lines a long-line match showed only byte windows of.
    #[serde(
        rename = "wholeLines",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub whole_lines: Option<Continuation>,
    /// The rest of a declaration a `block` range stopped inside.
    #[serde(
        rename = "continueBlock",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub continue_block: Option<Continuation>,
    /// The whole declarations a `block` match kept only a context window of.
    #[serde(rename = "readBlock", skip_serializing_if = "Option::is_none", default)]
    pub read_block: Option<Continuation>,
    /// The full default match window a byte-bounded window narrowed.
    #[serde(
        rename = "expandContext",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub expand_context: Option<Continuation>,
    /// A structureSearch tree of a path that is a directory, not a file.
    #[serde(rename = "viewTree", skip_serializing_if = "Option::is_none", default)]
    pub view_tree: Option<serde_json::Value>,
    /// A structureSearch listing that finds a missing file by its stem.
    #[serde(
        rename = "viewStructure",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub view_structure: Option<serde_json::Value>,
    /// A localSearch for a missed matchString in the file's directory.
    #[serde(
        rename = "searchContent",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub search_content: Option<serde_json::Value>,
}

impl NextCalls {
    /// Add every continuation `other` carries; `self` keeps its own on a clash.
    pub fn absorb(&mut self, other: NextCalls) {
        let NextCalls {
            r#continue,
            read_bounded_lines,
            restart,
            whole_lines,
            continue_block,
            read_block,
            expand_context,
            view_tree,
            view_structure,
            search_content,
        } = other;
        for (mine, theirs) in [
            (&mut self.r#continue, r#continue),
            (&mut self.read_bounded_lines, read_bounded_lines),
            (&mut self.restart, restart),
            (&mut self.whole_lines, whole_lines),
            (&mut self.continue_block, continue_block),
            (&mut self.read_block, read_block),
            (&mut self.expand_context, expand_context),
        ] {
            if mine.is_none() {
                *mine = theirs;
            }
        }
        for (mine, theirs) in [
            (&mut self.view_tree, view_tree),
            (&mut self.view_structure, view_structure),
            (&mut self.search_content, search_content),
        ] {
            if mine.is_none() {
                *mine = theirs;
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == NextCalls::default()
    }

    /// Whether a continuation reads more of this result (a page), as opposed
    /// to an optional lead such as `readBlock`; only a page makes the row
    /// partial.
    pub fn leaves_more(&self) -> bool {
        serde_json::to_value(self)
            .ok()
            .and_then(|value| {
                value.as_object().map(|calls| {
                    calls.keys().any(|name| {
                        crate::tools::id::is_remaining(crate::tools::id::ToolId::LocalFetch, name)
                    })
                })
            })
            .unwrap_or(false)
    }
}

/// A source-line read of `ranges` derived from `q`: the extraction selectors,
/// paging cursor, and `block` are cleared so the read returns exactly those
/// lines, spelled as the published `ranges`. More ranges than one read
/// holds read as one span from the first start to the last end.
pub fn line_read(q: &LocalFetchQuery, ranges: &[LineRange]) -> Option<LocalFetchQuery> {
    let (first, last) = (ranges.first()?, ranges.last()?);
    let mut query = q.clone();
    query.clear_block_selectors();
    query.match_string = None;
    query.regex = None;
    query.case_mode = None;
    query.context_lines = None;
    query.context_bytes = None;
    query.full_content = None;
    query.offset = None;
    query.unit = None;
    query.length = None;
    query.snapshot = None;
    let span = [LineRange {
        start: first.start,
        end: last.end,
    }];
    let ranges = if ranges.len() > MAX_READ_RANGES {
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

/// Most `ranges` one localFetch read accepts: the contract's `maxItems`.
pub const MAX_READ_RANGES: usize = crate::tools::id::query_limits::local_fetch::RANGES_MAX_ITEMS;

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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinifyFallback {
    pub requested: MinifyMode,
    pub applied: MinifyMode,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PartialReason {
    #[serde(rename = "full-content-size-limit")]
    FullContentLimit,
    #[serde(rename = "full-content-source-size-limit")]
    FullContentSourceSizeLimit,
    #[serde(rename = "security-selected-view-size-limit")]
    SecuritySelectedViewSizeLimit,
}

/// A declaration a `block:true` read widened a hit to: its name, name line
/// and last line, an lspSearch anchor without parsing the text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclaredBlock {
    pub symbol_name: String,
    pub line: usize,
    pub end_line: usize,
}

/// Internal read result. Rust callers see every field; the wire form (the
/// manual `Serialize` below) omits values an agent can already derive from
/// another emitted field.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalFetchResult {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(skip)]
    pub status: String,
    #[serde(skip)]
    pub resource_missing: bool,
    #[serde(skip)]
    pub source_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_view: Option<MinifyMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minify_fallback: Option<MinifyFallback>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub hints: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_lines: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub source_line_ranges: Vec<LineRange>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub match_ranges: Vec<LineRange>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub matched_lines: Vec<usize>,
    /// The declarations a `block:true` match read widened hits to.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub blocks: Vec<DeclaredBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_match_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_chars: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returned_chars: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returned_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returned_lines: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<Pagination>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_partial: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub partial_reasons: Vec<PartialReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_limit: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub metadata_unavailable: Vec<String>,
    /// The requested offset is past the end of the selected view; emitted as
    /// `pagination.outOfRange`.
    #[serde(default)]
    pub out_of_range: bool,
    #[serde(skip_serializing_if = "Option::is_none", skip_deserializing)]
    pub next: Option<NextCalls>,
}
impl LocalFetchResult {
    /// A result with `status` and every other field empty.
    pub fn blank(status: &str) -> Self {
        Self {
            path: String::new(),
            status: status.into(),
            resource_missing: false,
            source_sha256: None,
            content: None,
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
            metadata_unavailable: vec![],
            out_of_range: false,
            next: None,
        }
    }

    pub fn error(_path: String, code: &str, message: String) -> Self {
        Self {
            error_code: Some(code.into()),
            error: Some(message),
            ..Self::blank("error")
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

/// Wire view of [`Pagination`]: the view total in `unit` only when it
/// differs from the source total already emitted at the top level.
struct PaginationWire<'a> {
    page: &'a Pagination,
    source_lines: Option<usize>,
    source_bytes: Option<usize>,
    out_of_range: bool,
}
impl Serialize for PaginationWire<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let page = self.page;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("unit", &page.unit)?;
        map.serialize_entry("offset", &page.offset)?;
        map.serialize_entry("length", &page.length)?;
        match page.unit {
            WindowUnit::Lines if self.source_lines != Some(page.total_lines) => {
                map.serialize_entry("totalLines", &page.total_lines)?
            }
            WindowUnit::Bytes if self.source_bytes != Some(page.total_bytes) => {
                map.serialize_entry("totalBytes", &page.total_bytes)?
            }
            _ => {}
        }
        map.serialize_entry("hasMore", &page.has_more)?;
        if let Some(next_offset) = page.next_offset {
            map.serialize_entry("nextOffset", &next_offset)?;
        }
        if self.out_of_range {
            map.serialize_entry("outOfRange", &true)?;
        }
        map.end()
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
        if self.selected_match_count != Some(self.matched_lines.len()) {
            opt(map, "selectedMatchCount", &self.selected_match_count)?;
        }
        Ok(())
    }

    /// Explanation counters: the contract classes them verbose, so the
    /// response keeps them only under `debug: true`.
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
        // `none` is the default view; only a transformed view is news.
        opt(
            &mut map,
            "contentView",
            &self.content_view.filter(|view| *view != MinifyMode::None),
        )?;
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
                    source_bytes: self.source_bytes,
                    out_of_range: self.out_of_range,
                },
            )?;
        }
        opt(&mut map, "isPartial", &self.is_partial)?;
        list(&mut map, "partialReasons", &self.partial_reasons)?;
        opt(&mut map, "terminalLimit", &self.terminal_limit)?;
        list(&mut map, "metadataUnavailable", &self.metadata_unavailable)?;
        opt(&mut map, "next", &self.next)?;
        map.end()
    }
}

/// What the caller knows about a read besides its bytes.
#[derive(Clone, Debug, Default)]
pub struct SourceFacts {
    /// The file's modification time (ISO 8601), when the caller has one.
    pub modified: Option<String>,
    /// The configured response window: a whole-file view larger than it
    /// returns its first page and `next.continue`.
    pub window: Option<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRead {
    pub canonical: PathBuf,
    pub display: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathFailure {
    pub code: String,
    pub message: String,
    pub safe_path: Option<String>,
    pub resource_missing: bool,
    /// The missing path lies in a sparse git checkout, so it may exist
    /// upstream outside the checked-out paths.
    pub sparse_checkout: bool,
    /// The path is a directory, not a file.
    pub directory: bool,
    /// A missing path's closest existing ancestor directory, named as the
    /// policy displays it.
    pub nearest_dir: Option<String>,
}
pub trait PathAccess {
    fn validate_read(&self, path: &Path) -> Result<ValidatedRead, PathFailure>;
}
pub trait RegexMatch {
    fn matching_ranges(
        &self,
        pattern: &str,
        case_sensitive: bool,
        input: &str,
    ) -> Result<Vec<(usize, usize)>, String>;
}

#[derive(Clone, Default)]
pub struct LocalFetchRegex {
    isolated: Option<Arc<crate::regex::IsolatedRegexEngine>>,
}
impl LocalFetchRegex {
    pub fn new(isolated: Option<Arc<crate::regex::IsolatedRegexEngine>>) -> Self {
        Self { isolated }
    }
}
impl RegexMatch for LocalFetchRegex {
    fn matching_ranges(
        &self,
        source: &str,
        case_sensitive: bool,
        input: &str,
    ) -> Result<Vec<(usize, usize)>, String> {
        use crate::regex::{EcmaPattern, RegexExecutionClass, RegexLimits};
        // Matches select lines, so `^`/`$` must anchor per line.
        let flags = if case_sensitive { "gm" } else { "gim" };
        let limits = RegexLimits {
            max_pattern_bytes: 4_096,
            max_input_bytes: 10 * 1024 * 1024,
            max_matches: 10_000,
        };
        let pattern = EcmaPattern::compile(source, flags, limits)
            .map_err(|error| format!("Invalid regex pattern: {}", error.message))?;
        let ranges = match pattern.execution_class() {
            RegexExecutionClass::LinearInProcess => pattern.find_ranges(input),
            RegexExecutionClass::RequiresIsolatedEngine => self
                .isolated
                .as_ref()
                .ok_or_else(|| {
                    "Regex execution unavailable: pattern requires the configured isolated ECMAScript worker".to_owned()
                })?
                .find_ranges(source, flags, input),
        }
        .map_err(|error| format!("Invalid regex pattern: {}", error.message))?;
        Ok(ranges
            .into_iter()
            .map(|range| (range.start, range.end))
            .collect())
    }
}

impl PathAccess for crate::policy::path::PathPolicy {
    fn validate_read(&self, path: &Path) -> Result<ValidatedRead, PathFailure> {
        crate::policy::path::PathPolicy::validate_read(self, path)
            .map(|validated| ValidatedRead {
                canonical: validated.canonical,
                display: validated.display,
            })
            .map_err(|error| {
                let missing = error.code == crate::policy::PolicyErrorCode::NotFound;
                let resolved = self.expand_and_resolve(path);
                PathFailure {
                    code: error.local_error_code("pathValidationFailed").into(),
                    message: error.message,
                    safe_path: error.safe_path,
                    resource_missing: missing,
                    sparse_checkout: missing && in_sparse_checkout(&resolved),
                    directory: error.code == crate::policy::PolicyErrorCode::NotRegular
                        && resolved.is_dir(),
                    nearest_dir: missing
                        .then(|| self.nearest_existing_dir(&path.to_string_lossy()))
                        .flatten(),
                }
            })
    }
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
impl LocalFetchQuery {
    /// Minimal valid wire query; tests override the fields they exercise.
    pub(crate) fn test_default() -> Self {
        serde_json::from_value(
            serde_json::json!({"path": "_", "mainGoal": "test", "reasoning": "test"}),
        )
        .expect("minimal localFetch query")
    }
}

#[cfg(test)]
mod line_read_tests {
    use super::{LineRange, LocalFetchQuery, MAX_READ_RANGES, line_read};

    /// Every lead `line_read` builds (readBlock, continueBlock,
    /// readBoundedLines) spells lines with the published `ranges`, never
    /// startLine/endLine, and clears a span the caller sent.
    #[test]
    fn line_reads_spell_published_ranges() {
        let mut q = LocalFetchQuery::test_default();
        q.set_line_span(1, 9);
        let wire = |query: LocalFetchQuery| serde_json::to_value(query).expect("query");
        let one = wire(line_read(&q, &[LineRange { start: 57, end: 69 }]).expect("read"));
        assert_eq!(one["ranges"], serde_json::json!(["57-69"]), "{one}");
        assert!(
            one.get("startLine").is_none() && one.get("endLine").is_none(),
            "{one}"
        );
        let many = (0..=MAX_READ_RANGES)
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
            crate::tools::id::ToolId::LocalFetch,
            None,
            "ranges",
            "maxItems",
        )
        .expect("localFetch ranges declares maxItems");
        assert_eq!(MAX_READ_RANGES as u64, declared);
    }
}
