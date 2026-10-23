use crate::num::{u32_of, u32_of_signed};
use octocode_contract_types::ErrorCode;
pub(super) use octocode_engine::types::SearchContext;
use serde::Serialize;
use std::path::PathBuf;

pub use octocode_contract_types::{
    LocalSearchQuery, LocalSearchQueryCaseMode, LocalSearchQueryMultiline, LocalSearchQueryRegex,
    LocalSearchQueryResultView, LocalSearchQuerySort, LocalSearchQueryUnique, MatchString,
};

/// The engine counts in `u32`; the wire contract owns the field set and its
/// JSON integer types.
pub(crate) mod local_search_query {
    use super::*;
    /// Context lines each side of a hit when `clasifyContext` judges hits
    /// and no context field is set: a bare hit line (`fn prunes(..) {`)
    /// reads as related at best, while the hit with its body shows what it
    /// does, so the judge can select it.
    pub(crate) const CLASIFY_CONTEXT_LINES: u32 = crate::id::CLASIFY_CONTEXT_LINES;

    /// Resolve the shorthand once; explicit zero overrides either side.
    pub(crate) fn context(query: &LocalSearchQuery) -> SearchContext {
        let unset = query.context_lines.is_none()
            && query.context_before.is_none()
            && query.context_after.is_none();
        let fallback = if unset && query.clasify_context.is_some() {
            CLASIFY_CONTEXT_LINES
        } else {
            0
        };
        let symmetric = query.context_lines.map(u32_of_signed).unwrap_or(fallback);
        SearchContext {
            before: query.context_before.map(u32_of_signed).unwrap_or(symmetric),
            after: query.context_after.map(u32_of_signed).unwrap_or(symmetric),
        }
    }
    pub(crate) fn match_content_length(query: &LocalSearchQuery) -> Option<u32> {
        query.match_content_length.map(|n| u32_of(n.get()))
    }
    /// The caller's per-file row cap; `None` lets the result size decide.
    pub(crate) fn match_page_size(query: &LocalSearchQuery) -> Option<u32> {
        query.match_page_size.map(|n| u32_of(n.get()))
    }
    /// The walk depth below `path` in engine terms (0 = files directly in
    /// `path`): the wire `maxDepth` counts levels, 1 = its children.
    pub(crate) fn max_depth(query: &LocalSearchQuery) -> Option<u32> {
        query
            .max_depth
            .map(|depth| u32_of(depth.get()).saturating_sub(1))
    }
    pub(crate) fn page(query: &LocalSearchQuery) -> u32 {
        u32_of(query.page.get())
    }
    pub(crate) fn match_page(query: &LocalSearchQuery) -> u32 {
        u32_of(query.match_page.get())
    }
    pub(crate) fn page_size(query: &LocalSearchQuery) -> Option<u32> {
        query.page_size.map(|n| u32_of(n.get()))
    }
    pub(crate) fn snapshot(query: &LocalSearchQuery) -> Option<&str> {
        query.snapshot.as_deref().map(String::as_str)
    }
    /// The search mode: the caller's `regex`, else literal text in every tool
    /// (`a|b` finds that text; `regex:"rust"` runs it as a pattern).
    pub(crate) fn regex_mode(query: &LocalSearchQuery) -> LocalSearchQueryRegex {
        query.regex.unwrap_or(LocalSearchQueryRegex::Literal)
    }

    /// The text the search runs: the one `matchString`. A list is only ever
    /// seen here after [`LocalSearchQuery::normalized`] folded it into its alternation.
    pub(crate) fn pattern(query: &LocalSearchQuery) -> &str {
        match &query.match_string {
            MatchString::String(one) => one.as_str(),
            MatchString::Array(_) => unreachable!("a list matchString is normalized first"),
        }
    }

    /// The query with a list `matchString` as the one regex query that finds
    /// the same lines (`tools::match_list`): each entry literal text unless
    /// `regex` names a dialect, each judged by `caseMode` on its own (smart:
    /// an uppercase letter makes that entry sensitive). Everything after
    /// this, leads and cursors included, sees an ordinary regex search that
    /// runs as written. A single `matchString`, or a list of one, is that
    /// text as written.
    pub(crate) fn normalized(
        query: &LocalSearchQuery,
    ) -> Result<std::borrow::Cow<'_, LocalSearchQuery>, crate::response::result::ToolError> {
        let MatchString::Array(list) = &query.match_string else {
            return Ok(std::borrow::Cow::Borrowed(query));
        };
        // One entry is that text itself: it keeps its literal or regex
        // reading (and the leads that read it), as a string would.
        if let [only] = list.as_slice() {
            let text: &str = only;
            let mut query = query.clone();
            query.match_string = MatchString::String(text.parse().map_err(|_| {
                crate::response::result::ToolError::new(
                    ErrorCode::InvalidInput,
                    "matchString: an entry needs text.",
                )
            })?);
            return Ok(std::borrow::Cow::Owned(query));
        }
        let sensitive = |entry: &str| match query.case_mode {
            LocalSearchQueryCaseMode::Smart => entry.chars().any(char::is_uppercase),
            LocalSearchQueryCaseMode::Sensitive => true,
            LocalSearchQueryCaseMode::Insensitive => false,
        };
        let dialect = match regex_mode(query) {
            LocalSearchQueryRegex::Pcre2 => LocalSearchQueryRegex::Pcre2,
            LocalSearchQueryRegex::Literal | LocalSearchQueryRegex::Rust => {
                LocalSearchQueryRegex::Rust
            }
        };
        let alternation = crate::tools::match_list::alternation(
            list.iter().map(|entry| {
                let text: &str = entry;
                (text, sensitive(text))
            }),
            regex_mode(query) != LocalSearchQueryRegex::Literal,
        );
        let mut query = query.clone();
        query.match_string = MatchString::String(alternation.parse().map_err(|_| {
            crate::response::result::ToolError::new(
                ErrorCode::InvalidInput,
                "matchString: a list needs at least one entry.",
            )
        })?);
        query.regex = Some(dialect);
        query.case_mode = LocalSearchQueryCaseMode::Sensitive;
        Ok(std::borrow::Cow::Owned(query))
    }

    /// The query with every value that means what the field's absence means
    /// dropped: an off flag (`includeHidden:false`), `regex:"literal"`,
    /// `defaultExcludes:true`, `contextLines:0` and the default
    /// `matchContentLength`. The snapshot key and the page cut are derived
    /// from it, so a cursor never goes stale because a caller spelled a
    /// default; continuations still carry the fields as the caller sent them.
    pub(crate) fn canonical(query: &LocalSearchQuery) -> LocalSearchQuery {
        use crate::policy::prune::DefaultsFlag;
        let mut query = query.clone();
        for flag in [
            &mut query.include_hidden,
            &mut query.include_ignored,
            &mut query.invert_match,
            &mut query.reverse,
            &mut query.whole_word,
        ] {
            if *flag == Some(false) {
                *flag = None;
            }
        }
        if query.regex == Some(LocalSearchQueryRegex::Literal) {
            query.regex = None;
        }
        if query.default_excludes.defaults() {
            query.default_excludes = None;
        }
        let context = context(&query);
        let symmetric = context.before == context.after;
        query.context_lines =
            (symmetric && context.before > 0).then_some(i64::from(context.before));
        query.context_before =
            (!symmetric && context.before > 0).then_some(i64::from(context.before));
        query.context_after = (!symmetric && context.after > 0).then_some(i64::from(context.after));
        let length = query.match_content_length.take();
        if length.map(|n| u32_of(n.get()))
            != Some(crate::tools::local_search::executor::effective_match_content_length(&query))
        {
            query.match_content_length = length;
        }
        query
    }
}

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

#[cfg(test)]
mod context_tests {
    use super::*;

    /// `clasifyContext` judges each hit with context unless the caller set a
    /// context field; an explicit 0 keeps bare hit lines.
    #[test]
    fn clasify_hits_get_context_unless_a_context_field_is_set() {
        let query = |extra: serde_json::Value| -> LocalSearchQuery {
            let mut row = serde_json::json!({"path": ".", "matchString": "x"});
            row.as_object_mut()
                .expect("object")
                .extend(extra.as_object().expect("object").clone());
            serde_json::from_value(row).expect("query")
        };
        let clasify = serde_json::json!({"classifiers": ["Q?"]});
        let lines = local_search_query::CLASIFY_CONTEXT_LINES;
        assert_eq!(
            local_search_query::context(&query(serde_json::json!({"clasifyContext": clasify}))),
            SearchContext::symmetric(lines)
        );
        assert_eq!(
            local_search_query::context(&query(
                serde_json::json!({"clasifyContext": clasify, "contextLines": 0})
            )),
            SearchContext::symmetric(0)
        );
        assert_eq!(
            local_search_query::context(&query(serde_json::json!({}))),
            SearchContext::symmetric(0)
        );
    }
}
