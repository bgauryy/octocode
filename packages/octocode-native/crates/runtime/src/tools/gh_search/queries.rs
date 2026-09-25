//! GitHub query syntax, matching the canonical provider query builders.
use super::GhSearchQuery;

use crate::providers::github::{
    ProviderError, SearchName, qualifier_value, quote_search_keyword, search_phrase,
    validate_search_name,
};

/// Emit `key:value` as one term (quoted when the value holds whitespace,
/// quotes, or parentheses) so it cannot split into stray keywords.
fn push(parts: &mut Vec<String>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        let value = qualifier_value(value);
        if !value.is_empty() {
            parts.push(format!("{key}:{value}"));
        }
    }
}

/// Owner and repository names become `repo:`/`user:` scopes; reject any value
/// that is not a GitHub name before it can rewrite the scope.
pub(super) fn validate_scope(query: &GhSearchQuery) -> Result<(), ProviderError> {
    let (owner, repo) = match query {
        GhSearchQuery::Code { owner, repo, .. } => {
            (Some(owner.as_str()), repo.as_deref().map(String::as_str))
        }
        GhSearchQuery::Repositories { owner, .. } => (owner.as_deref().map(String::as_str), None),
        GhSearchQuery::Tree { .. } => return Ok(()),
    };
    if let Some(owner) = owner {
        validate_search_name("owner", owner, SearchName::Owner)?;
    }
    if let Some(repo) = repo {
        validate_search_name("repo", repo, SearchName::Repository)?;
    }
    Ok(())
}

/// Range qualifiers (`>100`, `a..b`) never contain meaningful whitespace;
/// strip it instead of quoting so the range syntax still applies. A bare
/// relative window (`30d`, `2w`, `6m`, `1y`) on a date qualifier resolves to
/// an absolute `>=YYYY-MM-DD` lower bound, which GitHub understands.
fn range_value(value: &str, date: bool) -> String {
    let compact: String = value.chars().filter(|c| !c.is_whitespace()).collect();
    let relative = compact.len() > 1
        && compact[..compact.len() - 1]
            .bytes()
            .all(|c| c.is_ascii_digit())
        && compact.ends_with(['h', 'd', 'w', 'm', 'y']);
    if date
        && relative
        && let Some(resolved) = crate::providers::github::resolve_date_window(&compact).value
    {
        return format!(">={}", resolved.chars().take(10).collect::<String>());
    }
    compact
}

pub(super) fn code_has_narrowing_selector(query: &GhSearchQuery) -> bool {
    let GhSearchQuery::Code {
        keywords,
        path,
        extension,
        filename,
        language,
        ..
    } = query
    else {
        return false;
    };
    keywords.iter().any(|value| !value.trim().is_empty())
        || [
            path.as_deref().map(String::as_str),
            extension.as_deref(),
            filename.as_deref(),
            language.as_deref(),
        ]
        .into_iter()
        .flatten()
        .any(|value| !value.trim().is_empty())
}

pub(super) fn code(query: &GhSearchQuery) -> String {
    let GhSearchQuery::Code {
        keywords,
        owner,
        repo,
        language,
        path,
        extension,
        filename,
        match_,
        ..
    } = query
    else {
        return String::new();
    };
    let mut parts: Vec<String> = keywords
        .iter()
        .map(|v| quote_search_keyword(v))
        .filter(|v| !v.is_empty())
        .collect();
    // `path` is a repository path prefix (schema contract); a dotted segment
    // such as `types/lodash.merge` is a directory, so never split it into a
    // `filename:` qualifier.
    push(&mut parts, "filename", filename.as_deref());
    push(&mut parts, "extension", extension.as_deref());
    if let Some(path) = path.as_deref() {
        let path = if path.contains('/') || path.contains('@') {
            search_phrase(path)
        } else {
            qualifier_value(path)
        };
        if !path.is_empty() {
            parts.push(format!("path:{path}"));
        }
    }
    push(&mut parts, "language", language.as_deref());
    if let Some(repo) = repo {
        push(&mut parts, "repo", Some(&format!("{owner}/{repo}")));
    } else {
        push(&mut parts, "user", Some(owner));
    }
    push(&mut parts, "in", Some(&match_.to_string()));
    parts.join(" ").trim().into()
}

pub(super) fn repositories(query: &GhSearchQuery) -> String {
    let GhSearchQuery::Repositories {
        keywords,
        owner,
        language,
        stars,
        forks,
        good_first_issues,
        updated,
        created,
        size,
        match_,
        archived,
        visibility,
        license,
        topics,
        ..
    } = query
    else {
        return String::new();
    };
    let mut parts: Vec<String> = keywords
        .iter()
        .map(|v| quote_search_keyword(v))
        .filter(|v| !v.is_empty())
        .collect();
    push(&mut parts, "user", owner.as_deref().map(String::as_str));
    for topic in topics {
        push(&mut parts, "topic", Some(topic));
    }
    for (key, value, date) in [
        ("stars", stars, false),
        ("size", size, false),
        ("created", created, true),
        ("pushed", updated, true),
        ("forks", forks, false),
        ("good-first-issues", good_first_issues, false),
    ] {
        let value = value.as_deref().map(|value| range_value(value, date));
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
    if let Some(visibility) = visibility {
        push(&mut parts, "is", Some(&visibility.to_string()));
    }
    parts.join(" ").trim().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(value: serde_json::Value) -> GhSearchQuery {
        serde_json::from_value(value).expect("query fixture")
    }

    #[test]
    fn repositories_exclude_archived_with_a_real_qualifier() {
        let q = repositories(&parse(
            serde_json::json!({"operation":"repositories","reasoning":"test","keywords":["x"]}),
        ));
        assert!(q.contains("archived:false"), "{q}");
        assert!(!q.contains("is:not-archived"), "{q}");
    }

    #[test]
    fn dotted_directory_path_stays_a_path_prefix() {
        let q = code(&parse(
            serde_json::json!({"operation":"code","reasoning":"test","owner":"o","keywords":["merge"],"path":"types/lodash.merge"}),
        ));
        assert!(q.contains("path:\"types/lodash.merge\""), "{q}");
        assert!(!q.contains("filename:"), "{q}");
    }

    #[test]
    fn qualifier_values_with_whitespace_are_quoted() {
        let q = code(&parse(
            serde_json::json!({"operation":"code","reasoning":"test","owner":"o","keywords":["x"],"language":"Common Lisp","filename":"my file.txt"}),
        ));
        assert!(q.contains("language:\"Common Lisp\""), "{q}");
        assert!(q.contains("filename:\"my file.txt\""), "{q}");
        let q = repositories(&parse(
            serde_json::json!({"operation":"repositories","reasoning":"test","keywords":["x"],"topics":["machine learning"],"stars":"> 100"}),
        ));
        assert!(q.contains("topic:\"machine learning\""), "{q}");
        assert!(q.contains("stars:>100"), "{q}");
    }

    #[test]
    fn reserved_boolean_keywords_are_quoted() {
        let q = code(&parse(
            serde_json::json!({"operation":"code","reasoning":"test","owner":"o","keywords":["foo","OR","bar","NOT","and"]}),
        ));
        assert!(q.starts_with("foo \"OR\" bar \"NOT\" \"and\""), "{q}");
    }

    #[test]
    fn leading_quote_keyword_cannot_negate_the_repo_scope() {
        // A raw `"hello" NOT` would bind NOT to the repo: qualifier.
        let q = code(&parse(serde_json::json!({
            "operation":"code","reasoning":"test","owner":"octocat","repo":"Hello-World",
            "keywords":["\"hello\" NOT", "x\" OR repo:evil/x"]
        })));
        assert_eq!(
            q, "\"hello NOT\" \"x OR repo:evil/x\" repo:octocat/Hello-World in:file",
            "{q}"
        );
    }

    #[test]
    fn scope_names_that_are_not_github_names_are_rejected() {
        for (owner, repo) in [("octocat OR is:public", None), ("a", Some("b\" OR x"))] {
            let mut raw = serde_json::json!({"operation":"code","reasoning":"test","owner":owner,"keywords":["x"]});
            if let Some(repo) = repo {
                raw["repo"] = serde_json::json!(repo);
            }
            let error = validate_scope(&parse(raw)).expect_err("rejected");
            assert_eq!(
                error.kind,
                crate::providers::github::ProviderErrorKind::Validation
            );
        }
        assert!(
            validate_scope(&parse(
                serde_json::json!({"operation":"code","reasoning":"test","owner":"octocat","repo":"Hello-World","keywords":["x"]})
            ))
            .is_ok()
        );
    }

    #[test]
    fn relative_repository_dates_resolve_to_absolute_ranges() {
        let q = repositories(&parse(
            serde_json::json!({"operation":"repositories","reasoning":"test","keywords":["x"],"updated":"30d","created":">2024-01-01"}),
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
