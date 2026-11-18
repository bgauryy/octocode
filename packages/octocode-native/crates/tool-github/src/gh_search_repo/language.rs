//! Evidence about a `language` filter, without mistaking an alias for an ignored qualifier.
use super::{GhSearchRepoQuery, query};
use octocode_github::{GitHubProvider, RepositorySearchRequest, RequestContext};
use serde_json::Value;

/// GitHub drops a `language:` qualifier it does not know and returns
/// unfiltered rows. Neither equal filtered/unfiltered totals nor differing
/// repository language values establish that the qualifier was ignored:
/// aliases and other languages in the repositories can explain the result.
/// Disclose uncertainty when removing the qualifier leaves the same count.
pub(super) async fn ignored_language(
    provider: &GitHubProvider,
    query: &GhSearchRepoQuery,
    rows: &[Value],
    matched: usize,
    context: &RequestContext,
) -> Option<String> {
    let wanted = super::access::filters(query)?.language.as_deref()?;
    let languages = rows
        .iter()
        .filter_map(|row| row["language"].as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if languages.is_empty()
        || languages
            .iter()
            .any(|language| language.eq_ignore_ascii_case(wanted))
    {
        return None;
    }
    // This probe is advisory: a failed or differently counted search
    // provides no evidence for an ignored qualifier.
    let mut without = query.clone();
    if let GhSearchRepoQuery::Search(q) = &mut without
        && let Some(filters) = &mut q.filters
    {
        filters.language = None;
    }
    let probe = provider
        .transport
        .search_repositories(
            &RepositorySearchRequest {
                query: query::repositories(&without),
                sort: None,
                page: 1,
                per_page: 1,
            },
            context,
        )
        .await
        .ok()?;
    if probe.total_count != matched {
        return None;
    }
    Some(format!(
        "GitHub returned repository language values different from `{wanted}`, with the same count as the unfiltered search; a language alias or other languages in these repositories may explain this, so filter validity is unverified."
    ))
}
