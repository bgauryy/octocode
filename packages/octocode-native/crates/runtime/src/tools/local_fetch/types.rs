use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChunkType {
    #[default]
    Lines,
    Bytes,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MinifyMode {
    #[default]
    None,
    Standard,
    Symbols,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalFetchRequest {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_content: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_string: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_string_is_regex: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_string_case_sensitive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_lines: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_type: Option<ChunkType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minify: Option<MinifyMode>,
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
    pub query: LocalFetchRequest,
    pub confidence: String,
    #[serde(rename = "why", skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NextCalls {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#continue: Option<Continuation>,
    #[serde(rename = "readBoundedLines", skip_serializing_if = "Option::is_none")]
    pub read_bounded_lines: Option<Continuation>,
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
        self.pagination
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

pub trait CancellationCheck: Sync {
    fn check(&self) -> Result<(), String>;
}
pub struct NeverCancel;
impl CancellationCheck for NeverCancel {
    fn check(&self) -> Result<(), String> {
        Ok(())
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
}
pub trait PathAccess {
    fn validate_read(&self, path: &Path) -> Result<ValidatedRead, PathFailure>;
}
pub trait ContentScan {
    fn sanitize(&self, text: &str, path: &Path) -> Result<(String, Vec<String>), (String, String)>;
    /// Redact whole PEM/OpenSSH/PGP private-key blocks across the FULL file
    /// before any read/search window is cut, closing the interior-window leak the
    /// anchored full-block patterns cannot catch. The default applies to every
    /// implementer (including test mocks); see
    /// [`crate::security::redact_private_key_blocks`].
    fn redact_key_blocks(&self, content: &str) -> (String, bool) {
        crate::security::redact_private_key_blocks(content)
    }
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
        let flags = if case_sensitive { "g" } else { "gi" };
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
                    code: "pathValidationFailed".into(),
                    message: error.message,
                    safe_path: error.safe_path,
                    resource_missing: missing,
                }
            })
    }
}

impl ContentScan for crate::security::ContentSecurity {
    fn sanitize(&self, text: &str, path: &Path) -> Result<(String, Vec<String>), (String, String)> {
        let result = self.sanitize_text(text, Some(path));
        if result
            .secrets_detected
            .iter()
            .any(|name| name == "content-size-exceeded")
        {
            return Err((
                "contentSecurityLimit".into(),
                "The selected content view exceeds the secret scanner size limit.".into(),
            ));
        }
        Ok((result.content, Vec::new()))
    }
}
