//! localSearch leads for exactly what a read looked for.
use super::super::types::local_fetch_query;
use octocode_contract_types::LocalFetchQuery;

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
    use crate::id::ToolId;
    if patterns.is_empty() {
        return None;
    }
    let matched = local_fetch_query::matched(q)?;
    let mut row = serde_json::json!({
        "path":path,"matchString":patterns,"regexEngine":matched.regex_engine,
        "caseMode":matched.case_mode,"wholeWord":matched.whole_word,
        "contextLines":matched.context_lines
    });
    if matched.context_lines.is_none() {
        row.as_object_mut()?.remove("contextLines");
    }
    Some(crate::response::result::Continuation::related(ToolId::LocalSearch, row).build())
}
