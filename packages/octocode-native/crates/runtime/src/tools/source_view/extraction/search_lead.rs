//! localSearch leads for exactly what a read looked for.
use super::super::types::local_fetch_query;
use octocode_contract_types::{
    CaseMode, LocalFetchQuery, LocalSearchQueryContent, LocalSearchQueryContentContextLines,
    LocalSearchQueryContentMultiline, LocalSearchQueryContentView, LocalSearchSort,
    LocalSearchSortAnyOrder, LocalSearchSortAnyOrderBy, RegexEngine, SortOrder, TextMatchTerms,
};

/// A content localSearch of `terms` under `path` with every other field at
/// its contract default: the base the localSearch leads refine.
pub(crate) fn text_search_query(path: &str, terms: &[&str]) -> Option<LocalSearchQueryContent> {
    let terms = terms
        .iter()
        .map(|term| (*term).try_into().ok())
        .collect::<Option<Vec<_>>>()?;
    Some(LocalSearchQueryContent {
        case_mode: CaseMode::Smart,
        classification: None,
        context_lines: None,
        default_excludes: None,
        exclude: Vec::new(),
        file_has: Vec::new(),
        file_lacks: Vec::new(),
        file_type: None,
        include: Vec::new(),
        include_hidden: None,
        include_ignored: None,
        invert_match: None,
        limit: None,
        main_goal: None,
        match_page: std::num::NonZeroU64::MIN,
        match_string: TextMatchTerms(terms),
        max_depth: None,
        multiline: LocalSearchQueryContentMultiline::Off,
        page: std::num::NonZeroU64::MIN,
        path: path.try_into().ok()?,
        reasoning: None,
        regex_engine: RegexEngine::Literal,
        selection: None,
        snapshot: None,
        sort: LocalSearchSort::AnyOrder(LocalSearchSortAnyOrder {
            by: LocalSearchSortAnyOrderBy::Relevance,
            order: SortOrder::Desc,
        }),
        test_code: None,
        view: LocalSearchQueryContentView::Content,
        whole_word: false,
    })
}

/// A localSearch over the same file and pattern(s): it streams and pages
/// every match, past the read's in-process match limit.
pub(super) fn match_limit_lead(
    q: &LocalFetchQuery,
    patterns: &[&str],
) -> Option<serde_json::Value> {
    search_lead(q, local_fetch_query::path(q), patterns)
}

/// A localSearch under `path` for exactly what the read looked for, so the
/// lead never matches more than the read would: the regex dialect, each
/// pattern's case rule and `wholeWord` carry over. Several patterns join as
/// one alternation, each keeping its own case rule; literal ones are
/// escaped first.
pub(crate) fn search_lead(
    q: &LocalFetchQuery,
    path: &str,
    patterns: &[&str],
) -> Option<serde_json::Value> {
    if patterns.is_empty() {
        return None;
    }
    let matched = local_fetch_query::matched(q)?;
    let query = LocalSearchQueryContent {
        case_mode: matched.case_mode,
        context_lines: matched.context_lines.as_ref().map(|context| {
            LocalSearchQueryContentContextLines {
                after: context.after,
                before: context.before,
            }
        }),
        regex_engine: matched.regex_engine,
        whole_word: matched.whole_word,
        ..text_search_query(path, patterns)?
    };
    Some(crate::response::result::Continuation::related_query(query).build())
}
