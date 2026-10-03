//! GitHub query syntax, matching the canonical provider query builders.
use super::{GhSearchCodeQuery, GhSearchRepoQuery};

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
pub(super) fn validate_code_scope(query: &GhSearchCodeQuery) -> Result<(), ProviderError> {
    validate_scope(
        Some(query.owner.as_str()),
        query.repo.as_deref().map(String::as_str),
    )
}

pub(super) fn validate_repo_scope(query: &GhSearchRepoQuery) -> Result<(), ProviderError> {
    validate_scope(query.owner.as_deref().map(String::as_str), None)
}

fn validate_scope(owner: Option<&str>, repo: Option<&str>) -> Result<(), ProviderError> {
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
    let relative = compact
        .as_bytes()
        .split_last()
        .is_some_and(|(unit, digits)| {
            matches!(unit, b'h' | b'd' | b'w' | b'm' | b'y')
                && !digits.is_empty()
                && digits.iter().all(u8::is_ascii_digit)
        });
    if date
        && relative
        && let Some(resolved) = crate::providers::github::resolve_date_window(&compact).value
    {
        return format!(">={}", resolved.chars().take(10).collect::<String>());
    }
    compact
}

pub(super) fn code_has_narrowing_selector(query: &GhSearchCodeQuery) -> bool {
    let GhSearchCodeQuery {
        keywords,
        path,
        extension,
        filename,
        language,
        ..
    } = query;
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

pub(super) fn code(query: &GhSearchCodeQuery) -> String {
    let GhSearchCodeQuery {
        keywords,
        owner,
        repo,
        language,
        path,
        extension,
        filename,
        match_,
        ..
    } = query;
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

pub(super) fn repositories(query: &GhSearchRepoQuery) -> String {
    let GhSearchRepoQuery {
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
    if let Some(qualifiers) = query.qualifiers.as_deref() {
        push_qualifiers(&mut parts, qualifiers);
    }
    parts.join(" ").trim().into()
}

/// `qualifiers`: space-separated `key:value` filters (the contract allowlists
/// the keys). Each is re-emitted through the same range/date normalization
/// as the dedicated fields, so a value can never start a new term.
fn push_qualifiers(parts: &mut Vec<String>, qualifiers: &str) {
    for term in qualifiers.split_whitespace() {
        let Some((key, value)) = term.split_once(':') else {
            continue;
        };
        if key == "is" {
            push(parts, key, Some(value));
        } else {
            let value = range_value(value, matches!(key, "created" | "pushed"));
            push(parts, key, Some(&value));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_qualifiers_preserve_unicode_without_panicking() {
        for value in ["💥", "10💥", "é", "１２d"] {
            assert_eq!(range_value(value, false), value);
            assert_eq!(range_value(value, true), value);
        }
    }

    fn parse<Q: serde::de::DeserializeOwned>(value: serde_json::Value) -> Q {
        serde_json::from_value(value).expect("query fixture")
    }

    #[test]
    fn repositories_exclude_archived_with_a_real_qualifier() {
        let q = repositories(&parse(
            serde_json::json!({"mainGoal": "test", "reasoning":"test","keywords":["x"]}),
        ));
        assert!(q.contains("archived:false"), "{q}");
        assert!(!q.contains("is:not-archived"), "{q}");
    }

    #[test]
    fn dotted_directory_path_stays_a_path_prefix() {
        let q = code(&parse(
            serde_json::json!({"mainGoal": "test", "reasoning":"test","owner":"o","keywords":["merge"],"path":"types/lodash.merge"}),
        ));
        assert!(q.contains("path:\"types/lodash.merge\""), "{q}");
        assert!(!q.contains("filename:"), "{q}");
    }

    #[test]
    fn qualifier_values_with_whitespace_are_quoted() {
        let q = code(&parse(
            serde_json::json!({"mainGoal": "test", "reasoning":"test","owner":"o","keywords":["x"],"language":"Common Lisp","filename":"my file.txt"}),
        ));
        assert!(q.contains("language:\"Common Lisp\""), "{q}");
        assert!(q.contains("filename:\"my file.txt\""), "{q}");
        let q = repositories(&parse(
            serde_json::json!({"mainGoal": "test", "reasoning":"test","keywords":["x"],"topics":["machine learning"],"stars":"> 100"}),
        ));
        assert!(q.contains("topic:\"machine learning\""), "{q}");
        assert!(q.contains("stars:>100"), "{q}");
    }

    #[test]
    fn reserved_boolean_keywords_are_quoted() {
        let q = code(&parse(
            serde_json::json!({"mainGoal": "test", "reasoning":"test","owner":"o","keywords":["foo","OR","bar","NOT","and"]}),
        ));
        assert!(q.starts_with("foo \"OR\" bar \"NOT\" \"and\""), "{q}");
    }

    #[test]
    fn leading_quote_keyword_cannot_negate_the_repo_scope() {
        // A raw `"hello" NOT` would bind NOT to the repo: qualifier.
        let q = code(&parse(serde_json::json!({
            "mainGoal": "test", "reasoning":"test","owner":"octocat","repo":"Hello-World",
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
            let mut raw = serde_json::json!({"mainGoal": "test", "reasoning":"test","owner":owner,"keywords":["x"]});
            if let Some(repo) = repo {
                raw["repo"] = serde_json::json!(repo);
            }
            let error = validate_code_scope(&parse(raw)).expect_err("rejected");
            assert_eq!(
                error.kind,
                crate::providers::github::ProviderErrorKind::Validation
            );
        }
        assert!(
            validate_code_scope(&parse(
                serde_json::json!({"mainGoal": "test", "reasoning":"test","owner":"octocat","repo":"Hello-World","keywords":["x"]})
            ))
            .is_ok()
        );
    }

    #[test]
    fn qualifiers_append_normalized_filters_and_match_the_old_fields() {
        let with_qualifiers = repositories(&parse(serde_json::json!({
            "mainGoal": "test", "reasoning":"test","keywords":["http client"],
            "qualifiers":"forks:>50  size:<5000 created:>2020-01-01 is:public"
        })));
        let with_fields = repositories(&parse(serde_json::json!({
            "mainGoal": "test", "reasoning":"test","keywords":["http client"],
            "forks":">50","size":"<5000","created":">2020-01-01","visibility":"public"
        })));
        for term in [
            "forks:>50",
            "size:<5000",
            "created:>2020-01-01",
            "is:public",
        ] {
            assert!(with_qualifiers.contains(term), "{with_qualifiers}");
            assert!(with_fields.contains(term), "{with_fields}");
        }
        let relative = repositories(&parse(serde_json::json!({
            "mainGoal": "test", "reasoning":"test","keywords":["x"],"qualifiers":"pushed:30d"
        })));
        assert!(relative.contains("pushed:>="), "{relative}");
    }

    #[test]
    fn relative_repository_dates_resolve_to_absolute_ranges() {
        let q = repositories(&parse(
            serde_json::json!({"mainGoal": "test", "reasoning":"test","keywords":["x"],"updated":"30d","created":">2024-01-01"}),
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
