use crate::id::local_search_policy;
use crate::num::u32_of_signed;
use octocode_contract_types::{
    CaseMode, DefaultExcludes, LocalSearchQuery, LocalSearchQueryContent, LocalSearchQueryFiles,
    LocalSearchQueryFilesMultiline, LocalSearchQueryFilesView, LocalSearchQueryMatchOnly,
    LocalSearchQueryMatchOnlyMultiline, LocalSearchSort, LocalSearchSortAnyOrderBy, RegexEngine,
    TextMatchTerms,
};
use octocode_engine::num::saturating_u32;
use octocode_engine::types::{SearchContext, SearchSort};
use serde_json::json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum View {
    Content,
    Files,
    FilesWithout,
    CountLines,
    CountMatches,
    MatchOnly,
}
pub(crate) use octocode_contract_types::LocalSearchQueryContentMultiline as Multiline;
pub(crate) use octocode_contract_types::LocalSearchQueryMatchOnlyUnique as Unique;

/// Binds `$field` from whichever view variant `$q` is.
macro_rules! common {
    ($q:expr,$field:ident,$value:expr) => {
        crate::tools::variant_field::variant_field!($q, newtype [
            LocalSearchQuery::Content => LocalSearchQueryContent,
            LocalSearchQuery::Files => LocalSearchQueryFiles,
            LocalSearchQuery::MatchOnly => LocalSearchQueryMatchOnly,
        ] $field => $value)
    };
}
pub(crate) fn path(q: &LocalSearchQuery) -> &str {
    common!(q, path, path.as_str())
}
pub(crate) fn test_code(q: &LocalSearchQuery) -> Option<octocode_contract_types::TestCode> {
    common!(q, test_code, *test_code)
}
pub(crate) fn case_mode(q: &LocalSearchQuery) -> CaseMode {
    common!(q, case_mode, *case_mode)
}
pub(crate) fn regex_mode(q: &LocalSearchQuery) -> RegexEngine {
    common!(q, regex_engine, *regex_engine)
}
pub(crate) fn patterns(q: &LocalSearchQuery) -> &TextMatchTerms {
    common!(q, match_string, match_string)
}
pub(crate) fn include(q: &LocalSearchQuery) -> &[String] {
    common!(q, include, include.as_slice())
}
pub(crate) fn exclude(q: &LocalSearchQuery) -> &[String] {
    common!(q, exclude, exclude.as_slice())
}
pub(crate) fn include_hidden(q: &LocalSearchQuery) -> Option<bool> {
    common!(q, include_hidden, *include_hidden)
}
pub(crate) fn include_ignored(q: &LocalSearchQuery) -> Option<bool> {
    common!(q, include_ignored, *include_ignored)
}
pub(crate) fn invert_match(q: &LocalSearchQuery) -> Option<bool> {
    common!(q, invert_match, *invert_match)
}
pub(crate) fn whole_word(q: &LocalSearchQuery) -> Option<bool> {
    common!(q, whole_word, Some(*whole_word))
}
pub(crate) fn default_excludes(q: &LocalSearchQuery) -> &Option<DefaultExcludes> {
    common!(q, default_excludes, default_excludes)
}
pub(crate) fn page(q: &LocalSearchQuery) -> u32 {
    common!(q, page, saturating_u32(page.get()))
}
/// Rows per page (`limit`): hit rows in content and matchOnly views, files
/// in the others; `None` cuts pages by the budget alone.
pub(crate) fn limit(q: &LocalSearchQuery) -> Option<usize> {
    common!(
        q,
        limit,
        limit.map(|limit| octocode_engine::num::saturating_usize(limit.get()))
    )
}
pub(crate) fn match_page(q: &LocalSearchQuery) -> u32 {
    common!(q, match_page, saturating_u32(match_page.get()))
}
pub(crate) fn max_depth(q: &LocalSearchQuery) -> Option<u32> {
    common!(
        q,
        max_depth,
        max_depth.map(|depth| saturating_u32(depth.get()).saturating_sub(1))
    )
}
pub(crate) fn snapshot(q: &LocalSearchQuery) -> Option<&str> {
    common!(q, snapshot, snapshot.as_deref().map(String::as_str))
}
pub(crate) fn file_type(q: &LocalSearchQuery) -> Option<&str> {
    common!(q, file_type, file_type.as_deref().map(String::as_str))
}
pub(crate) fn file_has(q: &LocalSearchQuery) -> Vec<String> {
    common!(
        q,
        file_has,
        file_has.iter().map(ToString::to_string).collect()
    )
}
pub(crate) fn file_lacks(q: &LocalSearchQuery) -> Vec<String> {
    common!(
        q,
        file_lacks,
        file_lacks.iter().map(ToString::to_string).collect()
    )
}
pub(crate) fn has_classification(q: &LocalSearchQuery) -> bool {
    match q {
        LocalSearchQuery::Content(LocalSearchQueryContent { classification, .. }) => {
            classification.is_some()
        }
        LocalSearchQuery::MatchOnly(LocalSearchQueryMatchOnly { classification, .. }) => {
            classification.is_some()
        }
        _ => false,
    }
}
pub(crate) fn selection(q: &LocalSearchQuery) -> Option<&str> {
    match q {
        LocalSearchQuery::Content(LocalSearchQueryContent { selection, .. }) => {
            selection.as_deref().map(String::as_str)
        }
        LocalSearchQuery::MatchOnly(LocalSearchQueryMatchOnly { selection, .. }) => {
            selection.as_deref().map(String::as_str)
        }
        _ => None,
    }
}
pub(crate) fn context(q: &LocalSearchQuery) -> SearchContext {
    match q {
        LocalSearchQuery::Content(LocalSearchQueryContent {
            context_lines: Some(context),
            ..
        }) => SearchContext {
            before: u32_of_signed(context.before),
            after: u32_of_signed(context.after),
        },
        _ => SearchContext::symmetric(0),
    }
}
pub(crate) fn view(q: &LocalSearchQuery) -> View {
    match q {
        LocalSearchQuery::Content(LocalSearchQueryContent { .. }) => View::Content,
        LocalSearchQuery::MatchOnly(LocalSearchQueryMatchOnly { .. }) => View::MatchOnly,
        LocalSearchQuery::Files(LocalSearchQueryFiles { view, .. }) => match view {
            LocalSearchQueryFilesView::Files => View::Files,
            LocalSearchQueryFilesView::FilesWithout => View::FilesWithout,
            LocalSearchQueryFilesView::CountLines => View::CountLines,
            LocalSearchQueryFilesView::CountMatches => View::CountMatches,
        },
    }
}
pub(crate) fn multiline(q: &LocalSearchQuery) -> Multiline {
    match q {
        LocalSearchQuery::Content(q) => q.multiline,
        LocalSearchQuery::Files(q) => match q.multiline {
            LocalSearchQueryFilesMultiline::Off => Multiline::Off,
            LocalSearchQueryFilesMultiline::On => Multiline::On,
            LocalSearchQueryFilesMultiline::Dotall => Multiline::Dotall,
        },
        LocalSearchQuery::MatchOnly(q) => match q.multiline {
            LocalSearchQueryMatchOnlyMultiline::Off => Multiline::Off,
            LocalSearchQueryMatchOnlyMultiline::On => Multiline::On,
            LocalSearchQueryMatchOnlyMultiline::Dotall => Multiline::Dotall,
        },
    }
}
pub(crate) fn unique(q: &LocalSearchQuery) -> Unique {
    match q {
        LocalSearchQuery::MatchOnly(LocalSearchQueryMatchOnly { unique, .. }) => *unique,
        _ => Unique::Off,
    }
}
/// The query's `sort`, shared by every view.
fn sort_of(q: &LocalSearchQuery) -> &LocalSearchSort {
    common!(q, sort, sort)
}
pub(crate) fn sort(q: &LocalSearchQuery) -> SearchSort {
    match sort_of(q) {
        LocalSearchSort::AscOnly(_) => SearchSort::Traversal,
        LocalSearchSort::AnyOrder(any) => match any.by {
            LocalSearchSortAnyOrderBy::Relevance => SearchSort::Relevance,
            LocalSearchSortAnyOrderBy::MatchCount => SearchSort::MatchCount,
            LocalSearchSortAnyOrderBy::Path => SearchSort::Path,
            LocalSearchSortAnyOrderBy::ModifiedAt => SearchSort::Modified,
            LocalSearchSortAnyOrderBy::AccessedAt => SearchSort::Accessed,
            LocalSearchSortAnyOrderBy::CreatedAt => SearchSort::Created,
        },
    }
}
pub(crate) fn reverse(q: &LocalSearchQuery) -> Option<bool> {
    let descending = match sort_of(q) {
        LocalSearchSort::AnyOrder(any) => any.order == octocode_contract_types::SortOrder::Desc,
        LocalSearchSort::AscOnly(_) => false,
    };
    Some(descending != matches!(sort(q), SearchSort::Relevance | SearchSort::MatchCount))
}
pub(crate) fn pattern(q: &LocalSearchQuery) -> &str {
    patterns(q).0.first().map_or("", |value| value.as_str())
}
/// Each `matchString` entry of a list query as the alternative
/// [`normalized`] joins it as, in order; `None` for a single entry.
pub(crate) fn term_patterns(q: &LocalSearchQuery) -> Option<Vec<String>> {
    (patterns(q).0.len() > 1).then(|| {
        patterns(q)
            .0
            .iter()
            .map(|entry| alternation_of(q, std::iter::once(entry.as_str())))
            .collect()
    })
}

/// The alternation of `entries`, each keeping the query's case rule for
/// itself and escaped unless the query is a regex.
fn alternation_of<'a>(q: &LocalSearchQuery, entries: impl Iterator<Item = &'a str>) -> String {
    let sensitive = |entry: &str| match case_mode(q) {
        CaseMode::Smart => entry.chars().any(char::is_uppercase),
        CaseMode::Sensitive => true,
        CaseMode::Insensitive => false,
    };
    crate::tools::match_list::alternation(
        entries.map(|entry| (entry, sensitive(entry))),
        regex_mode(q) != RegexEngine::Literal,
    )
}

pub(crate) fn normalized(
    q: &LocalSearchQuery,
) -> Result<std::borrow::Cow<'_, LocalSearchQuery>, crate::response::result::ToolError> {
    if patterns(q).0.len() == 1 {
        return Ok(std::borrow::Cow::Borrowed(q));
    }
    let alternation = alternation_of(q, patterns(q).0.iter().map(|entry| entry.as_str()));
    let dialect = if regex_mode(q) == RegexEngine::Pcre2 {
        RegexEngine::Pcre2
    } else {
        RegexEngine::Rust
    };
    let mut query = serde_json::to_value(q).map_err(|error| {
        crate::response::result::ToolError::new(
            octocode_contract_types::ErrorCode::InvalidInput,
            error.to_string(),
        )
    })?;
    query["matchString"] = json!([alternation]);
    query["regexEngine"] = json!(dialect);
    query["caseMode"] = json!(CaseMode::Sensitive);
    serde_json::from_value(query)
        .map(std::borrow::Cow::Owned)
        .map_err(|error| {
            crate::response::result::ToolError::new(
                octocode_contract_types::ErrorCode::InvalidInput,
                error.to_string(),
            )
        })
}
/// The contract's default matches per file (`semanticPolicy`).
pub(super) fn default_matches_per_file() -> u32 {
    saturating_u32(local_search_policy::MATCHES_PER_FILE)
}
pub(super) fn default_page_size(view: View) -> u32 {
    saturating_u32(if matches!(view, View::Content | View::MatchOnly) {
        local_search_policy::SNIPPET_PAGE_SIZE
    } else {
        local_search_policy::LIST_PAGE_SIZE
    })
}
pub(super) fn uses_context(view: View) -> bool {
    view == View::Content
}
pub(super) fn streamed_layout(q: &LocalSearchQuery) -> bool {
    match_page(q) <= 1
}
pub(super) fn shows_span_columns(q: &LocalSearchQuery) -> bool {
    view(q) == View::MatchOnly
}

/// `query` with `changes` applied (a `null` drops that field) and decoded
/// again as a search query: the edit of fields chosen at run time.
pub(crate) fn edited(
    query: &LocalSearchQuery,
    changes: &serde_json::Value,
) -> Result<LocalSearchQuery, crate::response::result::ToolError> {
    crate::response::result::edited_query(query, changes).map_err(|error| {
        crate::response::result::ToolError::new(
            octocode_contract_types::ErrorCode::ExecutionFailed,
            format!("the continuation is no valid localSearch query: {error}"),
        )
    })
}
