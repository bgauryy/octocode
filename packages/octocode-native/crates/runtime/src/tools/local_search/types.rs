pub(super) use octocode_engine::types::SearchContext;
use serde::Serialize;
use std::path::PathBuf;

pub use octocode_contract_types::{
    LocalSearchQuery, LocalSearchQueryCaseMode, LocalSearchQueryMultiline, LocalSearchQueryRegex,
    LocalSearchQueryResultView, LocalSearchQuerySort, LocalSearchQueryUnique,
};

/// Characters that give a text a regex reading besides its literal one.
pub(super) const REGEX_METACHARACTERS: [char; 11] =
    ['.', '(', '|', '[', '*', '+', '?', '^', '$', '{', '\\'];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SearchStatus {
    #[default]
    Success,
    Empty,
    Partial,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMatch {
    pub line: u32,
    /// Start of the span on its line; only span rows (`matchOnly`) carry it,
    /// since several spans can share a line. A line row's value shows it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    pub value: String,
    /// Byte offset of the hit in its line: where a byte-window read of a long
    /// line starts at the match. Read by leads, never shown.
    #[serde(skip)]
    pub byte_offset: u32,
    /// Every matched line inside a merged context block, in order; present
    /// only when overlapping/adjacent windows were merged into this one.
    #[serde(rename = "matchedLines", skip_serializing_if = "Option::is_none")]
    pub match_lines: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    /// The value is clipped (`originalChars`, `returnedChars` say by how much).
    /// Clipping a value leaves no row out, so it is not a partial result.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub clipped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_chars: Option<usize>,
    /// The clipped value's whole length in UTF-8 bytes, beside
    /// `original_chars`: whether a line read of it passes one localFetch
    /// page. Read by leads, never shown.
    #[serde(skip)]
    pub original_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returned_chars: Option<usize>,
    /// The innermost declaration around the hit (its name, kind, name line
    /// and last line: an lspSearch anchor and a localFetch span), on the
    /// first row of this file inside it; later rows inside it carry
    /// `enclosing_line` instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enclosing: Option<Enclosing>,
    /// The name line of the declaration enclosing this hit: the `enclosing`
    /// of the nearest earlier row of this file whose `enclosing.line` is it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enclosing_line: Option<u32>,
    /// `true` on a row whose hit line declares a name (the lspSearch
    /// anchor among same-name hits); absent on every other row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declaration: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemPagination {
    /// Match-page position; a streamed page has no per-file match pages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_pages: Option<u32>,
    /// Omitted on a streamed page where it equals the shown rows plus
    /// `moreLines` and `moreLinesUnlisted`.
    #[serde(rename = "totalItems", skip_serializing_if = "Option::is_none")]
    pub total_matches: Option<u32>,
    pub has_more: bool,
    /// Lines of this file's hits on later pages, ascending (numbers,
    /// not a packed list): read them directly or page on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub more_lines: Option<Vec<u32>>,
    /// Later-page hit lines past the listed `moreLines` (the match pages
    /// still hold them).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub more_lines_unlisted: Option<u32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub out_of_range: bool,
}

/// A declaration enclosing a hit, named like the inputs that take it
/// (`symbolName` + `line` = `line`, `ranges` = `line-endLine`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Enclosing {
    pub symbol_name: String,
    pub kind: String,
    pub line: u32,
    pub end_line: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFile {
    pub path: String,
    /// JSON and TOML files: each hit line's key path (`matchKeys`).
    #[serde(
        rename = "matchKeys",
        skip_serializing_if = "Vec::is_empty",
        serialize_with = "crate::tools::match_keys::serialize"
    )]
    pub match_keys: Vec<(usize, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matches: Option<Vec<SearchMatch>>,
    #[serde(rename = "matchCount", skip_serializing_if = "Option::is_none")]
    pub total_occurrences: Option<u32>,
    #[serde(rename = "matchedLineCount", skip_serializing_if = "Option::is_none")]
    pub total_matched_lines: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<ItemPagination>,
}

/// The response's totals. They serialize flat into the row (`matchCount`,
/// `matchedLineCount`, `filesScanned`, `bytesSearched`, `binarySkipped`,
/// `fileErrors`, `partialReasons`); the fields marked `skip` are what the
/// runtime reads and the row never carries.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchStats {
    /// Occurrences; absent when the view did not count them.
    #[serde(rename = "matchCount", skip_serializing_if = "Option::is_none")]
    pub total_occurrences: Option<u32>,
    /// Matched lines; absent when the view did not count them.
    #[serde(rename = "matchedLineCount", skip_serializing_if = "Option::is_none")]
    pub matched_lines: Option<u32>,
    #[serde(skip)]
    pub files_matched: u32,
    #[serde(rename = "filesScanned")]
    pub files_searched: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_searched: Option<i64>,
    #[serde(skip)]
    pub capped: Option<bool>,
    #[serde(skip)]
    pub cap_reasons: Vec<octocode_engine::types::CapReason>,
    /// The limits that ended the scan early, one per reason (the engine's
    /// [`octocode_engine::types::CapReason`] names).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub partial_reasons: Vec<String>,
    #[serde(skip)]
    pub error_count: Option<u32>,
    #[serde(skip)]
    pub first_error: Option<String>,
    /// Files the search could not read, and the first of them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_errors: Option<FileErrors>,
    /// Files skipped as binary from their leading bytes, on a first page
    /// with hits (an empty page states it in `warnings`).
    #[serde(rename = "binarySkipped", skip_serializing_if = "Option::is_none")]
    pub binary_skipped: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FileErrors {
    pub count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePagination {
    /// What `total_items` counts: files on list and grid pages, hit lines on
    /// streamed line pages; absent (items) for matchOnly span rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<octocode_contract_types::OutputPaginationVariant0Unit>,
    pub current_page: u32,
    /// The match page this file page shows, past the first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_page: Option<u32>,
    pub total_pages: u32,
    /// Grid pages only; streamed pages are cut by the response budget.
    #[serde(rename = "pageSize", skip_serializing_if = "Option::is_none")]
    pub files_per_page: Option<u32>,
    /// In the pages' `unit`.
    pub total_items: u32,
    pub has_more: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub out_of_range: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSearchResult {
    #[serde(skip)]
    pub status: SearchStatus,
    #[serde(flatten)]
    pub stats: SearchStats,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<SearchFile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<FilePagination>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection: Option<octocode_contract_types::Selection>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<serde_json::Value>,
    /// Some candidate content was not searched (unreadable paths, or a binary
    /// file cut at its first NUL) or is past a cursor maximum, so the returned
    /// matches are not the full set and zero matches do not prove absence.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_partial: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub terminal_limit: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped_paths: Option<octocode_contract_types::SkippedPaths>,
    #[serde(skip)]
    pub source_snapshot: Option<String>,
    #[serde(skip)]
    pub source_root: PathBuf,
}
