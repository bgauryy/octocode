//! GitHub query syntax, matching the canonical provider query builders.
use super::GhSearchQuery;

fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

/// GitHub search treats bare OR/AND/NOT as boolean operators; keywords are
/// documented as ANDed literal terms, so quote the reserved words.
fn reserved_operator(value: &str) -> bool {
    ["OR", "AND", "NOT"]
        .iter()
        .any(|word| value.eq_ignore_ascii_case(word))
}

fn keyword(value: &str) -> String {
    if value.starts_with('"')
        || (!value.is_empty()
            && !reserved_operator(value)
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'))
    {
        value.into()
    } else {
        quoted(value)
    }
}

/// Emit `key:value`, quoting values that contain whitespace so the qualifier
/// is not split into a qualifier plus stray keywords.
fn push(parts: &mut Vec<String>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        if !value.starts_with('"') && value.chars().any(char::is_whitespace) {
            parts.push(format!("{key}:{}", quoted(value)));
        } else {
            parts.push(format!("{key}:{value}"));
        }
    }
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
    keywords
        .iter()
        .flatten()
        .any(|value| !value.trim().is_empty())
        || [path, extension, filename, language]
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
        match_kind,
        ..
    } = query
    else {
        return String::new();
    };
    let mut parts: Vec<String> = keywords
        .iter()
        .flatten()
        .filter(|v| !v.trim().is_empty())
        .map(|v| keyword(v))
        .collect();
    // `path` is a repository path prefix (schema contract); a dotted segment
    // such as `types/lodash.merge` is a directory, so never split it into a
    // `filename:` qualifier.
    push(&mut parts, "filename", filename.as_deref());
    push(&mut parts, "extension", extension.as_deref());
    if let Some(path) = path.as_deref() {
        let path = if !path.starts_with('"') && (path.contains('/') || path.contains('@')) {
            quoted(path)
        } else {
            path.into()
        };
        push(&mut parts, "path", Some(&path));
    }
    push(&mut parts, "language", language.as_deref());
    if let Some(owner) = owner {
        if let Some(repo) = repo {
            push(&mut parts, "repo", Some(&format!("{owner}/{repo}")));
        } else {
            push(&mut parts, "user", Some(owner));
        }
    }
    push(
        &mut parts,
        "in",
        Some(match_kind.as_deref().unwrap_or("file")),
    );
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
        match_kind,
        archived,
        visibility,
        license,
        topics,
        ..
    } = query
    else {
        return String::new();
    };
    let mut parts: Vec<String> = keywords.iter().flatten().map(|v| keyword(v)).collect();
    push(&mut parts, "user", owner.as_deref());
    for topic in topics.iter().flatten() {
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
    for kind in match_kind.iter().flatten() {
        push(&mut parts, "in", Some(kind));
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
        push(&mut parts, "is", Some(visibility));
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
            serde_json::json!({"operation":"repositories","keywords":["x"]}),
        ));
        assert!(q.contains("archived:false"), "{q}");
        assert!(!q.contains("is:not-archived"), "{q}");
    }

    #[test]
    fn dotted_directory_path_stays_a_path_prefix() {
        let q = code(&parse(
            serde_json::json!({"operation":"code","keywords":["merge"],"path":"types/lodash.merge"}),
        ));
        assert!(q.contains("path:\"types/lodash.merge\""), "{q}");
        assert!(!q.contains("filename:"), "{q}");
    }

    #[test]
    fn qualifier_values_with_whitespace_are_quoted() {
        let q = code(&parse(
            serde_json::json!({"operation":"code","keywords":["x"],"language":"Common Lisp","filename":"my file.txt"}),
        ));
        assert!(q.contains("language:\"Common Lisp\""), "{q}");
        assert!(q.contains("filename:\"my file.txt\""), "{q}");
        let q = repositories(&parse(
            serde_json::json!({"operation":"repositories","keywords":["x"],"topics":["machine learning"],"stars":"> 100"}),
        ));
        assert!(q.contains("topic:\"machine learning\""), "{q}");
        assert!(q.contains("stars:>100"), "{q}");
    }

    #[test]
    fn reserved_boolean_keywords_are_quoted() {
        let q = code(&parse(
            serde_json::json!({"operation":"code","keywords":["foo","OR","bar","NOT","and"]}),
        ));
        assert!(q.starts_with("foo \"OR\" bar \"NOT\" \"and\""), "{q}");
    }

    #[test]
    fn relative_repository_dates_resolve_to_absolute_ranges() {
        let q = repositories(&parse(
            serde_json::json!({"operation":"repositories","keywords":["x"],"updated":"30d","created":">2024-01-01"}),
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
