use napi_derive::napi;

#[napi(object)]
#[derive(Debug, Clone, Default)]
pub struct GetExtensionOptions {
    pub lowercase: Option<bool>,
    pub fallback: Option<String>,
}

// ── ripgrep_parser types ──────────────────────────────────────────────────────

#[napi(object)]
#[derive(Debug, Clone, Default)]
pub struct RipgrepParseOptions {
    /// Number of context lines around each match (default 0).
    pub context_lines: Option<u32>,
    /// Max Unicode chars per match snippet (default 500).
    pub max_snippet_chars: Option<u32>,
}

#[napi(object)]
#[derive(Debug, Clone)]
pub struct RipgrepMatch {
    /// 1-based line number.
    pub line: u32,
    /// 0-based column offset of the first submatch.
    pub column: u32,
    /// Assembled match + context window, truncated to `max_snippet_chars`.
    pub value: String,
}

#[napi(object)]
#[derive(Debug, Clone)]
pub struct RipgrepFile {
    pub path: String,
    pub match_count: u32,
    pub matches: Vec<RipgrepMatch>,
}

#[napi(object)]
#[derive(Debug, Clone, Default)]
pub struct RipgrepStats {
    pub match_count: Option<u32>,
    pub matched_lines: Option<u32>,
    pub files_matched: Option<u32>,
    pub files_searched: Option<u32>,
    pub bytes_searched: Option<i64>,
    pub search_time: Option<String>,
}

#[napi(object)]
#[derive(Debug, Clone)]
pub struct RipgrepParseResult {
    pub files: Vec<RipgrepFile>,
    pub stats: RipgrepStats,
}

// ── utf8_offsets types ────────────────────────────────────────────────────────

#[napi(object)]
#[derive(Debug, Clone, Default)]
pub struct SliceContentOptions {
    /// When true, snap start to line start and end to line end (default false).
    pub snap_to_line_boundary: Option<bool>,
}

#[napi(object)]
#[derive(Debug, Clone)]
pub struct SliceContentResult {
    pub text: String,
    /// Actual start char offset (may differ from requested when snapping).
    pub char_offset: u32,
    pub char_length: u32,
    pub byte_offset: u32,
    pub byte_length: u32,
    pub has_more: bool,
    pub next_char_offset: Option<u32>,
}

// ── line_extractor types ──────────────────────────────────────────────────────

#[napi(object)]
#[derive(Debug, Clone, Default)]
pub struct ExtractMatchingLinesOptions {
    /// Treat `pattern` as a regex (default false — literal match).
    pub is_regex: Option<bool>,
    /// Case-sensitive match (default false).
    pub case_sensitive: Option<bool>,
    /// Lines of context to include around each match (default 0).
    pub context_lines: Option<u32>,
    /// Cap the number of matched lines returned.
    pub max_matches: Option<u32>,
}

#[napi(object)]
#[derive(Debug, Clone)]
pub struct MatchRange {
    /// 1-based inclusive start line.
    pub start: u32,
    /// 1-based inclusive end line.
    pub end: u32,
}

#[napi(object)]
#[derive(Debug, Clone)]
pub struct ExtractMatchingLinesResult {
    /// Output lines including context and omission markers.
    pub lines: Vec<String>,
    /// 1-based line numbers of actual matches (capped by `max_matches`).
    pub matching_lines: Vec<u32>,
    /// Total matches before `max_matches` cap.
    pub match_count: u32,
    pub match_ranges: Vec<MatchRange>,
}

// ── diff_parser types ─────────────────────────────────────────────────────────

#[napi(string_enum)]
#[derive(Debug, Clone, PartialEq)]
pub enum PatchLineType {
    Addition,
    Deletion,
    Context,
}

#[napi(object)]
#[derive(Debug, Clone)]
pub struct ParsedPatchLine {
    pub original_line_number: Option<u32>,
    pub new_line_number: Option<u32>,
    pub content: String,
    pub line_type: PatchLineType,
}

#[napi(object)]
#[derive(Debug, Clone, Default)]
pub struct FilterPatchOptions {
    /// Only keep additions at these new-file line numbers.
    pub additions: Option<Vec<i64>>,
    /// Only keep deletions at these original-file line numbers.
    pub deletions: Option<Vec<i64>>,
    /// Apply context trimming (equivalent to `trimDiffContext`, default false).
    pub trim_context: Option<bool>,
    /// Context window size when `trim_context` is true (default 2).
    pub context_lines: Option<u32>,
}

#[napi(object)]
#[derive(Debug, Clone)]
pub struct MinifyResult {
    pub content: String,
    pub failed: bool,
    /// Strategy name or "failed"
    pub r#type: String,
    pub reason: Option<String>,
}

impl MinifyResult {
    pub fn ok(content: String, strategy: &str) -> Self {
        MinifyResult {
            content,
            failed: false,
            r#type: strategy.to_owned(),
            reason: None,
        }
    }
    pub fn fail(content: String, reason: impl Into<String>) -> Self {
        MinifyResult {
            content,
            failed: true,
            r#type: "failed".to_owned(),
            reason: Some(reason.into()),
        }
    }
}

#[napi(object)]
#[derive(Debug, Clone)]
pub struct FileTypeMinifyConfig {
    pub strategy: String,
    /// CommentPatternGroup | CommentPatternGroup[]
    pub comments: Option<serde_json::Value>,
}

#[napi(object)]
#[derive(Debug, Clone, Default)]
pub struct YamlConversionConfig {
    pub sort_keys: Option<bool>,
    pub keys_priority: Option<Vec<String>>,
}
