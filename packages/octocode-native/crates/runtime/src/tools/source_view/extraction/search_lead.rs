//! localSearch leads for exactly what a read looked for.
use super::super::types::{LocalFetchQuery, local_fetch_query};

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
pub(in crate::tools::source_view) fn search_lead(
    q: &LocalFetchQuery,
    path: &str,
    patterns: &[&str],
) -> Option<serde_json::Value> {
    use crate::id::ToolId;
    let dialect = if local_fetch_query::is_pcre2(q) {
        "pcre2"
    } else {
        "rust"
    };
    let case = |pattern: &str| {
        if local_fetch_query::case_sensitive_for(q, pattern) {
            "sensitive"
        } else {
            "insensitive"
        }
    };
    let mut row = match patterns {
        [] => return None,
        [one] => {
            let mut row = serde_json::json!({
                "path": path,
                "matchString": one,
                "caseMode": case(one),
            });
            if local_fetch_query::is_regex(q) {
                row["regex"] = serde_json::json!(dialect);
            }
            row
        }
        many => {
            let alternation = crate::tools::match_list::alternation(
                many.iter()
                    .map(|pattern| (*pattern, local_fetch_query::case_sensitive_for(q, pattern))),
                local_fetch_query::is_regex(q),
            );
            serde_json::json!({
                "path": path,
                "matchString": alternation,
                "regex": dialect,
                "caseMode": "sensitive",
            })
        }
    };
    if local_fetch_query::whole_word(q) {
        row["wholeWord"] = serde_json::json!(true);
    }
    Some(crate::response::result::Continuation::new(ToolId::LocalSearch, row).build())
}
