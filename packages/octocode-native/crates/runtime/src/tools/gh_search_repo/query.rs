//! ghSearchRepo query syntax.
use super::GhSearchRepoQuery;
use crate::providers::github::{ProviderError, quote_search_keyword};
use crate::tools::gh_shared::query::{push, push_qualifiers, range_value, validate_scope};

pub(super) fn validate_repo_scope(query: &GhSearchRepoQuery) -> Result<(), ProviderError> {
    validate_scope(query.owner.as_deref().map(String::as_str), None)
}

pub(super) fn repositories(query: &GhSearchRepoQuery) -> String {
    let GhSearchRepoQuery {
        keywords,
        owner,
        language,
        stars,
        pushed,
        created,
        match_,
        archived,
        license,
        topics,
        ..
    } = query;
    let mut parts: Vec<String> = keywords
        .iter()
        .map(|v| quote_search_keyword(v))
        .filter(|v| !v.is_empty())
        .collect();
    push(&mut parts, "user", owner.as_deref().map(String::as_str));
    for topic in topics {
        push(&mut parts, "topic", Some(topic));
    }
    let stars = stars.as_deref().map(|value| range_value(value, false));
    push(&mut parts, "stars", stars.as_deref());
    let dates = [("pushed", pushed.as_deref()), ("created", created.as_deref())];
    for (key, value) in dates {
        let value = value.map(|value| range_value(value, true));
        push(&mut parts, key, value.as_deref());
    }
    push(&mut parts, "language", language.as_deref());
    push(&mut parts, "license", license.as_deref());
    for kind in match_ {
        push(&mut parts, "in", Some(&kind.to_string()));
    }
    parts.push(
        if *archived == Some(true) {
            "archived:true"
        } else {
            "archived:false"
        }
        .into(),
    );
    if let Some(qualifiers) = query.qualifiers.as_deref() {
        push_qualifiers(&mut parts, qualifiers);
    }
    parts.join(" ").trim().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::gh_shared::query::parse;

    #[test]
    fn repositories_exclude_archived_with_a_real_qualifier() {
        let q = repositories(&parse(
            serde_json::json!({"mainGoal": "test", "reasoning":"test","keywords":["x"]}),
        ));
        assert!(q.contains("archived:false"), "{q}");
        assert!(!q.contains("is:not-archived"), "{q}");
    }

    #[test]
    fn topic_and_star_values_are_quoted_or_compacted() {
        let q = repositories(&parse(
            serde_json::json!({"mainGoal": "test", "reasoning":"test","keywords":["x"],"topics":["machine learning"],"stars":"> 100"}),
        ));
        assert!(q.contains("topic:\"machine learning\""), "{q}");
        assert!(q.contains("stars:>100"), "{q}");
    }

    #[test]
    fn qualifiers_append_normalized_filters() {
        let with_qualifiers = repositories(&parse(serde_json::json!({
            "mainGoal": "test", "reasoning":"test","keywords":["http client"],
            "qualifiers":"forks:>50  size:<5000 followers:>10 is:public"
        })));
        for term in ["forks:>50", "size:<5000", "followers:>10", "is:public"] {
            assert!(with_qualifiers.contains(term), "{with_qualifiers}");
        }
    }

    #[test]
    fn relative_repository_dates_resolve_to_absolute_ranges() {
        let q = repositories(&parse(
            serde_json::json!({"mainGoal": "test", "reasoning":"test","keywords":["x"],"pushed":"30d","created":">2024-01-01"}),
        ));
        let pushed = q
            .split(' ')
            .find(|part| part.starts_with("pushed:"))
            .expect("pushed qualifier");
        assert!(pushed.starts_with("pushed:>="), "{q}");
        assert_eq!(pushed.len(), "pushed:>=2026-01-01".len(), "{q}");
        assert!(q.contains("created:>2024-01-01"), "{q}");
    }
}
