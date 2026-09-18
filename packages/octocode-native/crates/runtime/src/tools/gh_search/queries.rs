//! GitHub query syntax, matching the canonical provider query builders.
use super::GhSearchQuery;

fn keyword(value: &str) -> String {
    if value.starts_with('"')
        || (!value.is_empty()
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'))
    {
        value.into()
    } else {
        format!("\"{}\"", value.replace('"', "\\\""))
    }
}

fn push(parts: &mut Vec<String>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        parts.push(format!("{key}:{value}"));
    }
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
    let mut path = path.as_deref();
    let mut filename = filename.as_deref();
    if filename.is_none()
        && let Some(candidate) = path
    {
        let tail = candidate.rsplit('/').next().unwrap_or(candidate);
        if let Some((stem, suffix)) = tail.rsplit_once('.')
            && !stem.is_empty()
            && !suffix.is_empty()
            && suffix.len() <= 10
            && suffix.as_bytes()[0].is_ascii_alphabetic()
            && suffix.bytes().all(|c| c.is_ascii_alphanumeric())
        {
            filename = Some(tail);
            let directory =
                &candidate[..candidate.len().saturating_sub(tail.len()).saturating_sub(1)];
            path = (!directory.is_empty()).then_some(directory);
        }
    }
    push(&mut parts, "filename", filename);
    push(&mut parts, "extension", extension.as_deref());
    if let Some(path) = path {
        let path = if !path.starts_with('"') && (path.contains('/') || path.contains('@')) {
            format!("\"{path}\"")
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
    for (key, value) in [
        ("stars", stars),
        ("size", size),
        ("created", created),
        ("pushed", updated),
        ("language", language),
        ("forks", forks),
        ("license", license),
        ("good-first-issues", good_first_issues),
    ] {
        push(&mut parts, key, value.as_deref());
    }
    for kind in match_kind.iter().flatten() {
        push(&mut parts, "in", Some(kind));
    }
    parts.push(
        if *archived == Some(true) {
            "archived:true"
        } else {
            "is:not-archived"
        }
        .into(),
    );
    if let Some(visibility) = visibility {
        push(&mut parts, "is", Some(visibility));
    }
    parts.join(" ").trim().into()
}
