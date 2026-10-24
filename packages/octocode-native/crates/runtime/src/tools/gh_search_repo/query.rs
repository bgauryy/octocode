use crate::tools::gh_shared::{query::SearchTerms, ranges};
use octocode_contract_types::{GhSearchRepoQuery, GitHubArchive};

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
            ("forks", &filters.fork_count),
            ("size", &filters.size_ki_b),
            ("good-first-issues", &filters.good_first_issue_count),
            ("help-wanted-issues", &filters.help_wanted_issue_count),
            ("topics", &filters.topic_count),
            ("followers", &filters.follower_count),
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
mod tests {
    use super::*;
    use crate::tools::gh_shared::query::parse;
    use serde_json::json;
    #[test]
    fn typed_repository_filters_preserve_units_bounds_and_or_locations() {
        let q = repositories(&parse(
            json!({"operation":"search", "owner":"o", "matchIn":["name","readme"], "filters":{
                "archive":"only", "stars":{"min":0,"max":10}, "sizeKiB":{"max":5000}, "visibility":"public",
                "language":"Common Lisp", "topics":["machine-learning"], "isSponsorable":true,
                "createdAt":{"from":"2024-01-01T00:00:00Z"}
            }}),
        ));
        for term in [
            "user:o",
            "fork:true",
            "archived:true",
            "stars:0..10",
            "size:<=5000",
            "is:public",
            "language:\"Common Lisp\"",
            "topic:machine-learning",
            "is:sponsorable",
            "created:>=2024-01-01T00:00:00Z",
            "in:name,readme",
        ] {
            assert!(q.contains(term), "{term}: {q}");
        }
        assert!(!q.contains("archived:false"));
    }
    #[test]
    fn archive_default_and_inclusion_are_distinct() {
        for (archive, expected) in [
            (None, Some("archived:false")),
            (Some("include"), None),
            (Some("only"), Some("archived:true")),
        ] {
            let mut row = json!({"operation":"search","keywords":["client"]});
            if let Some(archive) = archive {
                row["filters"] = json!({"archive":archive});
            }
            let q = repositories(&parse(row));
            assert_eq!(
                q.split_whitespace()
                    .find(|part| part.starts_with("archived:")),
                expected
            );
        }
    }
}
