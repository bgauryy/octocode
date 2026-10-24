//! A `language` filter GitHub ignored, told apart from an alias it resolved.
use super::{GhSearchRepoQuery, query};
use octocode_github::{GitHubProvider, RepositorySearchRequest, RequestContext};
use serde_json::Value;

/// GitHub drops a `language:` qualifier it does not know and returns
/// unfiltered rows. When no row on the page is that language, the filter
/// was ignored if the rows span several languages; if they share one (an
/// alias GitHub resolved, like `js` or `golang`, or unfiltered rows that
/// happen to agree), one more search without the qualifier decides: an
/// ignored qualifier leaves the count unchanged. That probe runs only on
/// such a page, so an alias costs one extra search request.
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
    if languages.len() == 1 {
        // One other language: an alias GitHub resolved, or an ignored
        // filter whose rows happen to agree. An ignored qualifier changes
        // no count, so the same search without it tells them apart. The
        // probe is advisory: if it fails, nothing is claimed.
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
    }
    Some(format!(
        "GitHub ignored language `{wanted}`, not a language name it knows: these rows are unfiltered."
    ))
}
