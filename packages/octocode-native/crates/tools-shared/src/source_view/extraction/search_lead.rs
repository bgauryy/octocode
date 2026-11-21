//! localSearch leads for exactly what a read looked for.
use super::super::types::local_fetch_query;
use octocode_contract_types::{
    CaseMode, ContentView, LocalFetchQuery, LocalSearchQueryContent, LocalSearchQueryMultiline,
    RegexEngine, TextMatchTerms,
};

/// A content localSearch of `terms` under `path` with every other field at
/// its contract default: the base the localSearch leads refine.
pub fn text_search_query(path: &str, terms: &[&str]) -> Option<LocalSearchQueryContent> {
    let terms = terms
        .iter()
        .map(|term| (*term).try_into().ok())
        .collect::<Option<Vec<_>>>()?;
    Some(LocalSearchQueryContent {
        case_mode: CaseMode::Smart,
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
        key_path: None,
        limit: None,
        main_goal: None,
        match_string: TextMatchTerms(terms),
        max_depth: None,
        multiline: LocalSearchQueryMultiline::Off,
        page: std::num::NonZeroU64::MIN,
        path: path.try_into().ok()?,
        reasoning: None,
        regex_engine: RegexEngine::Literal,
        row_offset: None,
        scan_offset: None,
        snapshot: None,
        // Walk order, the contract's default sort.
        sort: serde_json::from_value(serde_json::json!({ "by": "traversal" })).ok()?,
        test_code: None,
        view: ContentView::Content,
        walk_offset: None,
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
pub fn search_lead(
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
        context_lines: matched.context_lines.clone(),
        regex_engine: matched.regex_engine,
        whole_word: matched.whole_word,
        ..text_search_query(path, patterns)?
    };
    Some(octocode_runtime_base::response::result::Continuation::related_query(query).build())
}
