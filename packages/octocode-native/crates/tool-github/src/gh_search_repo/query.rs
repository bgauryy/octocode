use octocode_contract_types::{GhSearchRepoQuery, GitHubArchive};
use octocode_tools_shared::gh_shared::{query::SearchTerms, ranges};

pub(super) fn repositories(query: &GhSearchRepoQuery) -> String {
    let keywords = super::access::keywords(query);
    let mut terms = SearchTerms::new(keywords.iter().map(String::as_str));
    terms.scope(super::access::owner(query), super::access::repo(query));
    if super::access::owner(query).is_some() {
        terms.push("fork:true");
    }
    if let Some(filters) = super::access::filters(query) {
        for topic in &filters.topics {
            terms.text("topic", Some(topic));
        }
        for (key, value) in [
            ("stars", &filters.stars),
            ("forks", &filters.forks),
            ("size", &filters.size_ki_b),
            ("good-first-issues", &filters.good_first_issue_count),
            ("help-wanted-issues", &filters.help_wanted_issue_count),
        ] {
            terms.text(key, value.as_ref().and_then(ranges::numeric).as_deref());
        }
        for (key, value) in [
            ("created", &filters.created_at),
            ("pushed", &filters.pushed_at),
        ] {
            terms.text(key, value.as_ref().and_then(ranges::date).as_deref());
        }
        terms.text("language", filters.language.as_ref().map(|v| v.as_str()));
        terms.text("license", filters.license.as_ref().map(|v| v.as_str()));
        terms.text(
            "is",
            filters
                .visibility
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
        );
        if filters.is_sponsorable == Some(true) {
            terms.push("is:sponsorable");
        }
    }
    if let GhSearchRepoQuery::Search(q) = query
        && !q.match_in.is_empty()
    {
        terms.text(
            "in",
            Some(
                &q.match_in
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        );
    }
    match super::access::archive(query) {
        GitHubArchive::Exclude => terms.push("archived:false"),
        GitHubArchive::Only => terms.push("archived:true"),
        GitHubArchive::Include => {}
    }
    terms.finish()
}

#[cfg(test)]
mod tests;
