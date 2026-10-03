use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use crate::contracts::tool_types::{ChunkType, LocalFetchQuery, MatchString, MinifyMode};

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
    pub fn start_line(&self) -> Option<usize> {
        self.start_line.map(|n| usize_of(n.get()))
    }
    pub fn end_line(&self) -> Option<usize> {
        self.end_line.map(|n| usize_of(n.get()))
    }
    /// Context lines per match, clamped to [`MAX_CONTEXT_LINES`]: a larger
    /// request reads the maximum instead of failing the call.
    pub fn context_lines(&self) -> Option<usize> {
        self.context_lines
            .map(|n| usize_of_signed(n).min(MAX_CONTEXT_LINES))
    }
    /// The requested context when it exceeded the maximum.
    pub fn context_lines_clamped_from(&self) -> Option<usize> {
        self.context_lines
            .map(usize_of_signed)
            .filter(|n| *n > MAX_CONTEXT_LINES)
    }
    pub fn context_bytes(&self) -> Option<usize> {
        self.context_bytes.map(usize_of_signed)
    }
    pub fn offset(&self) -> Option<usize> {
        self.offset.map(usize_of_signed)
    }
    pub fn chunk_size(&self) -> Option<usize> {
        self.chunk_size.map(|n| usize_of(n.get()))
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
                let (start, end) = range.split_once('-')?;
                Some(LineRange {
                    start: start.parse().ok()?,
                    end: end.parse().ok()?,
                })
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

fn usize_of(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

fn usize_of_signed(value: i64) -> usize {
    usize::try_from(value).unwrap_or(0)
}

/// Largest `contextLines` applied around a match.
pub const MAX_CONTEXT_LINES: usize = 100;

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
    pub chunk_type: ChunkType,
    pub offset: usize,
    pub length: usize,
    pub chunk_size: usize,
    pub total_lines: usize,
    pub total_bytes: usize,
    pub has_more: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Continuation {
    pub tool: String,
    pub query: LocalFetchQuery,
    pub confidence: String,
    #[serde(rename = "why", skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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
    /// The context lines a clamped `contextLines` left out.
    #[serde(
        rename = "readContext",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub read_context: Option<Continuation>,
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
            read_context,
        } = other;
        for (mine, theirs) in [
            (&mut self.r#continue, r#continue),
            (&mut self.read_bounded_lines, read_bounded_lines),
            (&mut self.restart, restart),
            (&mut self.whole_lines, whole_lines),
            (&mut self.continue_block, continue_block),
            (&mut self.read_block, read_block),
            (&mut self.read_context, read_context),
        ] {
            if mine.is_none() {
                *mine = theirs;
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == NextCalls::default()
    }
}

/// A source-line read of `ranges` derived from `q`: the extraction selectors,
/// paging cursor, and `block` are cleared so the read returns exactly those
/// lines. One range reads as `startLine`/`endLine`; more than `ranges` holds
/// read as one span from the first start to the last end.
pub fn line_read(q: &LocalFetchQuery, ranges: &[LineRange]) -> Option<LocalFetchQuery> {
    let (first, last) = (ranges.first()?, ranges.last()?);
    let mut query = q.clone();
    query.clear_block_selectors();
    query.match_string = None;
    query.match_string_is_regex = None;
    query.match_string_case_sensitive = None;
    query.context_lines = None;
    query.context_bytes = None;
    query.full_content = None;
    query.offset = None;
    query.chunk_type = None;
    query.chunk_size = None;
    query.snapshot = None;
    query.start_line = None;
    query.end_line = None;
    if ranges.len() == 1 || ranges.len() > MAX_READ_RANGES {
        query.start_line = wire_positive(first.start);
        query.end_line = wire_positive(last.end);
    } else {
        query.ranges = ranges
            .iter()
            .filter_map(|range| format!("{}-{}", range.start, range.end).parse().ok())
            .collect();
    }
    Some(query)
}

/// Most `ranges` one localFetch read accepts.
pub const MAX_READ_RANGES: usize = 10;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<NextCalls>,
}
impl LocalFetchResult {
    pub fn error(_path: String, code: &str, message: String) -> Self {
        Self {
            path: String::new(),
            status: "error".into(),
            resource_missing: false,
            source_sha256: None,
            content: None,
            content_view: None,
            minify_fallback: None,
            error_code: Some(code.into()),
            error: Some(message),
            resolved_path: None,
            warnings: vec![],
            hints: vec![],
            total_lines: None,
            start_line: None,
            end_line: None,
            source_line_ranges: vec![],
            match_ranges: vec![],
            matched_lines: vec![],
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
    fn match_ranges_are_redundant(&self) -> bool {
        self.match_ranges == self.source_line_ranges
            || match (self.start_line, self.end_line, self.match_ranges.as_slice()) {
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
    fn returned_lines_are_derivable(&self) -> bool {
        !self.source_line_ranges.is_empty()
            && self.returned_lines
                == Some(
                    self.source_line_ranges
                        .iter()
                        .map(|range| range.end + 1 - range.start)
                        .sum(),
                )
    }
}

/// Wire view of [`Pagination`]: `length` only when it differs from
/// `chunkSize`, and only the view total in `chunkType` units when it differs
/// from the source total already emitted at the top level.
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
        map.serialize_entry("chunkType", &page.chunk_type)?;
        map.serialize_entry("offset", &page.offset)?;
        if page.length != page.chunk_size {
            map.serialize_entry("length", &page.length)?;
        }
        map.serialize_entry("chunkSize", &page.chunk_size)?;
        match page.chunk_type {
            ChunkType::Lines if self.source_lines != Some(page.total_lines) => {
                map.serialize_entry("totalLines", &page.total_lines)?
            }
            ChunkType::Bytes if self.source_bytes != Some(page.total_bytes) => {
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

impl Serialize for LocalFetchResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        macro_rules! opt {
            ($key:literal, $value:expr) => {
                if let Some(value) = &$value {
                    map.serialize_entry($key, value)?;
                }
            };
        }
        macro_rules! list {
            ($key:literal, $value:expr) => {
                if !$value.is_empty() {
                    map.serialize_entry($key, &$value)?;
                }
            };
        }
        if !self.path.is_empty() {
            map.serialize_entry("path", &self.path)?;
        }
        opt!("content", self.content);
        // `none` is the default view; only a transformed view is news.
        if let Some(view) = self.content_view.filter(|view| *view != MinifyMode::None) {
            map.serialize_entry("contentView", &view)?;
        }
        opt!("minifyFallback", self.minify_fallback);
        opt!("errorCode", self.error_code);
        opt!("error", self.error);
        opt!("resolvedPath", self.resolved_path);
        list!("warnings", self.warnings);
        list!("hints", self.hints);
        opt!("totalLines", self.total_lines);
        if !self.window_is_source_range() {
            opt!("startLine", self.start_line);
            opt!("endLine", self.end_line);
        }
        list!("sourceLineRanges", self.source_line_ranges);
        if !self.match_ranges_are_redundant() {
            list!("matchRanges", self.match_ranges);
        }
        list!("matchedLines", self.matched_lines);
        if self.selected_match_count != Some(self.matched_lines.len()) {
            opt!("selectedMatchCount", self.selected_match_count);
        }
        opt!("modified", self.modified);
        // UTF-16 char counts only when they differ from the UTF-8 byte counts
        // (non-ASCII text); bytes are the unit offsets and chunks use.
        if self.source_chars != self.source_bytes {
            opt!("sourceChars", self.source_chars);
        }
        opt!("sourceBytes", self.source_bytes);
        if self.returned_chars != self.returned_bytes {
            opt!("returnedChars", self.returned_chars);
        }
        opt!("returnedBytes", self.returned_bytes);
        if !self.returned_lines_are_derivable() {
            opt!("returnedLines", self.returned_lines);
        }
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
        opt!("isPartial", self.is_partial);
        list!("partialReasons", self.partial_reasons);
        opt!("terminalLimit", self.terminal_limit);
        list!("metadataUnavailable", self.metadata_unavailable);
        opt!("next", self.next);
        map.end()
    }
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
                PathFailure {
                    code: error.local_error_code("pathValidationFailed").into(),
                    message: error.message,
                    safe_path: error.safe_path,
                    resource_missing: missing,
                    sparse_checkout: missing && in_sparse_checkout(&self.expand_and_resolve(path)),
                }
            })
    }
}

/// Whether `path` lies in a git worktree with sparse checkout enabled (as a
/// `ghCloneRepo` clone with `sparsePath` is).
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
            serde_json::json!({"path": "_", "goal": "test", "reasoning": "test"}),
        )
        .expect("minimal localFetch query")
    }
}
