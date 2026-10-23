//! ghSearchRepo query syntax.
use super::GhSearchRepoQuery;
use crate::tools::gh_shared::query::SearchTerms;

pub(super) fn repositories(query: &GhSearchRepoQuery) -> String {
    let GhSearchRepoQuery {
        keywords,
        owner,
        language,
        stars,
        pushed,
        created,
        match_in,
        include_archived,
        license,
        topics,
        ..
    } = query;
    let mut terms = SearchTerms::new(keywords.iter().map(String::as_str));
    // owner+repo with filters searches inside that one repository.
    terms.scope(
        owner.as_deref().map(String::as_str),
        query.repo.as_deref().map(String::as_str),
    );
    // Search leaves forks out unless asked; an owner's repositories include
    // its forks, as its listing does, so every sort covers one set.
    if owner.is_some() {
        terms.push("fork:true");
    }
    for topic in topics {
        terms.text("topic", Some(topic));
    }
    terms.range("stars", stars.as_deref().map(String::as_str));
    terms.range("pushed", pushed.as_deref().map(String::as_str));
    terms.range("created", created.as_deref().map(String::as_str));
    terms.text("language", language.as_ref().map(|value| value.as_str()));
    terms.text("license", license.as_ref().map(|value| value.as_str()));
    for kind in match_in {
        terms.text("in", Some(&kind.to_string()));
    }
    // Repository search ignores `is:archived`/`is:not-archived`; they become
    // the `archived:` filter it honours, which replaces the default
    // exclusion. `includeArchived` sends no archive filter at all.
    let mut archive = None;
    let qualifiers = query.qualifiers.as_deref().map(|qualifiers| {
        qualifiers
            .split_whitespace()
            .filter(|term| {
                let filter = match *term {
                    "is:archived" => "archived:true",
                    "is:not-archived" => "archived:false",
                    _ => return true,
                };
                archive = Some(filter);
                false
            })
            .collect::<Vec<_>>()
            .join(" ")
    });
    if *include_archived != Some(true) || archive.is_some() {
        terms.push(archive.unwrap_or("archived:false"));
    }
    if let Some(qualifiers) = qualifiers {
        terms.qualifiers(&qualifiers);
    }
    terms.finish()
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

    /// `includeArchived` sends no archive qualifier. Repository search
    /// ignores `is:archived`/`is:not-archived`, so those become the
    /// `archived:` filter it honours, replacing the default.
    #[test]
    fn archived_true_includes_and_is_archived_replaces_the_default() {
        let included = repositories(&parse(
            serde_json::json!({"mainGoal": "test", "reasoning":"test","keywords":["x"],"includeArchived":true}),
        ));
        assert!(!included.contains("archived:"), "{included}");
        let q = |qualifiers: &str| {
            repositories(&parse(serde_json::json!({
                "mainGoal": "test", "reasoning":"test","keywords":["x"],"qualifiers": qualifiers
            })))
        };
        assert_eq!(q("is:archived"), "x archived:true");
        assert_eq!(q("is:not-archived"), "x archived:false");
        assert_eq!(q("is:public"), "x archived:false is:public");
    }

    /// An owner-scoped search covers the forks its listing shows; an
    /// unscoped search keeps GitHub's fork exclusion.
    #[test]
    fn owner_scoped_search_includes_forks() {
        let q = |row: serde_json::Value| repositories(&parse(row));
        assert_eq!(
            q(
                serde_json::json!({"mainGoal": "test", "reasoning":"test","owner":"o","sort":"stars:desc"})
            ),
            "user:o fork:true archived:false"
        );
        assert_eq!(
            q(
                serde_json::json!({"mainGoal": "test", "reasoning":"test","owner":"o","repo":"r","keywords":["x"]})
            ),
            "x repo:o/r fork:true archived:false"
        );
        assert!(
            !q(serde_json::json!({"mainGoal": "test", "reasoning":"test","keywords":["x"]}))
                .contains("fork:")
        );
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
