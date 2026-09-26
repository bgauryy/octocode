use serde::Serialize;
use std::path::PathBuf;

pub use crate::contracts::tool_types::{
    LocalSearchQuery, LocalSearchQueryCaseMode, LocalSearchQueryMultiline, LocalSearchQueryRegex,
    LocalSearchQueryResultView, LocalSearchQuerySort, LocalSearchQueryUnique,
};

/// The engine counts in `u32`; the wire contract owns the field set and its
/// JSON integer types.
impl LocalSearchQuery {
    pub fn context_lines(&self) -> Option<u32> {
        self.context_lines.map(u32_of_signed)
    }
    pub fn match_content_length(&self) -> Option<u32> {
        self.match_content_length.map(|n| u32_of(n.get()))
    }
    pub fn max_matches_per_file(&self) -> Option<u32> {
        self.max_matches_per_file.map(|n| u32_of(n.get()))
    }
    pub fn max_depth(&self) -> Option<u32> {
        self.max_depth.map(u32_of_signed)
    }
    pub fn match_window(&self) -> Option<u32> {
        self.match_window.map(u32_of_signed)
    }
    pub fn page(&self) -> u32 {
        u32_of(self.page.get())
    }
    pub fn match_page(&self) -> u32 {
        u32_of(self.match_page.get())
    }
    pub fn page_size(&self) -> Option<u32> {
        self.page_size.map(|n| u32_of(n.get()))
    }
    pub fn snapshot(&self) -> Option<&str> {
        self.snapshot.as_deref().map(String::as_str)
    }
}

fn u32_of(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn u32_of_signed(value: i64) -> u32 {
    u32::try_from(value.max(0)).unwrap_or(u32::MAX)
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

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMatch {
    pub line: u32,
    pub column: u32,
    pub value: String,
    /// Every matched line inside a merged context block, in order; present
    /// only when overlapping/adjacent windows were merged into this one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_lines: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_chars: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returned_chars: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemPagination {
    pub current_page: u32,
    pub total_pages: u32,
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
    pub stats: SearchStats,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<SearchFile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<FilePagination>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<serde_json::Value>,
    /// Some candidate content was not searched (unreadable paths, or a binary
    /// file cut at its first NUL), so the returned matches are not the full set
    /// and zero matches do not prove absence.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_partial: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub terminal_limit: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip)]
    pub source_snapshot: Option<String>,
    #[serde(skip)]
    pub source_root: PathBuf,
}
