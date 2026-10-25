//! The read result and its wire form.
use super::{DeclaredBlock, LineRange, NextCalls, Pagination, PartialReason, ViewFallback};
use octocode_contract_types::{ErrorCode, ReadProgressUnit, ReadView};
use serde::Serialize;

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
    pub view: Option<ReadView>,
    pub view_fallback: Option<ViewFallback>,
    pub error_code: Option<ErrorCode>,
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
            view: None,
            view_fallback: None,
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
    pub fn error(path: String, code: ErrorCode, message: String) -> Self {
        Self {
            path,
            error_code: Some(code),
            error: Some(message),
            ..Self::blank(ReadStatus::Error)
        }
    }

    /// Ends the row at a limit no continuation lifts, as
    /// [`crate::response::paging::terminal_limit`] marks a JSON row: partial for
    /// `reason`, and `terminalLimit`.
    pub(crate) fn terminal_limit(&mut self, reason: PartialReason) {
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
}

/// Wire view of [`Pagination`]: the view total in `unit`. A line view's
/// total is stated only when it differs from the top-level `totalLines`; a
/// byte view always states its total in bytes.
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
            ReadProgressUnit::Lines => ("lines", page.total_lines),
            ReadProgressUnit::Bytes => ("bytes", page.total_bytes),
        };
        let mut facts =
            crate::response::pages::SpanFacts::window(unit, page.offset, page.length, total)
                .out_of_range(self.out_of_range);
        if page.unit == ReadProgressUnit::Lines && self.source_lines == Some(total) {
            facts = facts.without_total();
        }
        let block = facts.to_value();
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
    /// A page with no text (nothing selected, minified away or past the
    /// end) locates nothing: its leads carry the coordinates to read.
    fn serialize_anchors<M: serde::ser::SerializeMap>(&self, map: &mut M) -> Result<(), M::Error> {
        opt(map, "totalLines", &self.total_lines)?;
        // Hit identity is not derivable from the text: a match-selected
        // page can contain only context, including text identical to a hit.
        if self.selected_match_count.is_some() || !self.matched_lines.is_empty() {
            map.serialize_entry("matchedLines", &self.matched_lines)?;
        }
        if self.content.as_deref().is_some_and(str::is_empty) {
            return opt(map, "selectedMatchCount", &self.selected_match_count);
        }
        if !self.window_is_source_range() {
            opt(map, "startLine", &self.start_line)?;
            opt(map, "endLine", &self.end_line)?;
        }
        list(map, "sourceLineRanges", &self.source_line_ranges)?;
        let match_ranges = self.page_match_ranges();
        if !self.match_ranges_are_redundant(&match_ranges) {
            list(map, "matchRanges", &match_ranges)?;
        }
        if !self.match_keys.is_empty() {
            map.serialize_entry(
                "matchKeys",
                &crate::tools::match_keys::MatchKeys(&self.match_keys),
            )?;
        }
        // A source match can cover several lines; its count is distinct.
        opt(map, "selectedMatchCount", &self.selected_match_count)?;
        Ok(())
    }

    /// Source and returned-size measurements when the reader computed them.
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
        opt(&mut map, "view", &self.view)?;
        opt(&mut map, "viewFallback", &self.view_fallback)?;
        opt(&mut map, "errorCode", &self.error_code)?;
        opt(&mut map, "error", &self.error)?;
        list(&mut map, "warnings", &self.warnings)?;
        list(&mut map, "hints", &self.hints)?;
        self.serialize_anchors(&mut map)?;
        list(&mut map, "blocks", &self.blocks)?;
        self.serialize_counters(&mut map)?;
        // Preserve the reader's pagination facts on every page.
        if let Some(page) = &self.pagination {
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
        let calls =
            crate::response::result::serialize_calls(&self.next, serde_json::value::Serializer)
                .map_err(serde::ser::Error::custom)?;
        for (channel, value) in calls.as_object().into_iter().flatten() {
            map.serialize_entry(channel, value)?;
        }
        map.end()
    }
}
