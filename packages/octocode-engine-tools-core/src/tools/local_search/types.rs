use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaseMode {
    Sensitive,
    Insensitive,
    #[default]
    Smart,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RegexMode {
    Literal,
    #[default]
    Rust,
    Pcre2,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResultView {
    MatchOnly,
    Discovery,
    Detailed,
    #[default]
    Paginated,
    Content,
    Files,
    FilesWithout,
    CountLines,
    CountMatches,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MultilineMode {
    #[default]
    Off,
    On,
    Dotall,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortMode {
    #[default]
    Relevance,
    Traversal,
    MatchCount,
    Path,
    Modified,
    Accessed,
    Created,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UniqueMode {
    #[default]
    Off,
    List,
    Count,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SearchStatus {
    #[default]
    Success,
    Empty,
    Partial,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalSearchError {
    pub code: &'static str,
    pub message: String,
    pub hints: Vec<String>,
    pub next: Option<Box<serde_json::Value>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalSearchRequest {
    pub search_text: String,
    pub path: String,
    pub case_mode: Option<CaseMode>,
    pub whole_word: Option<bool>,
    pub invert_match: Option<bool>,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub exclude_dir: Option<Vec<String>>,
    pub no_ignore: Option<bool>,
    pub hidden: Option<bool>,
    pub context_lines: Option<u32>,
    pub match_content_length: Option<u32>,
    pub max_matches_per_file: Option<u32>,
    pub max_files: Option<u32>,
    pub max_depth: Option<u32>,
    pub multiline: Option<MultilineMode>,
    pub sort: Option<SortMode>,
    pub ranking_profile: Option<String>,
    pub lang_type: Option<String>,
    pub unique: Option<UniqueMode>,
    pub match_window: Option<u32>,
    pub match_page: Option<u32>,
    pub page: Option<u32>,
    pub snapshot: Option<String>,
    pub regex: Option<RegexMode>,
    pub result_view: Option<ResultView>,
    pub page_size: Option<u32>,
    pub reverse: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMatch {
    pub line: u32,
    pub column: u32,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemPagination {
    pub current_page: u32,
    pub total_pages: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matches_per_page: Option<u32>,
    pub total_matches: u32,
    pub has_more: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_match_page: Option<u32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub out_of_range: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFile {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matches: Option<Vec<SearchMatch>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_occurrences: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_matched_lines: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_match_rows: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returned_match_rows: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<ItemPagination>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchStats {
    pub total_occurrences: u32,
    pub matched_lines: u32,
    pub files_matched: u32,
    pub files_searched: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_searched: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capped: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cap_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePagination {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
    pub current_page: u32,
    pub total_pages: u32,
    pub files_per_page: u32,
    pub total_files: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_matches: Option<u32>,
    pub has_more: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page: Option<u32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub out_of_range: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSearchResult {
    #[serde(skip)]
    pub status: SearchStatus,
    pub search_engine: String,
    pub stats: SearchStats,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<SearchFile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<FilePagination>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip)]
    pub source_snapshot: Option<String>,
    #[serde(skip)]
    pub source_root: PathBuf,
}
