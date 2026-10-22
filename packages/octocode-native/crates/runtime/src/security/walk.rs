//! JSON walker that redacts every domain string leaf — including `location`
//! values and the `query`/`why` leaves of executable `next` continuations —
//! while preserving object structure and the executable `tool` identifier.
//!
//! Every leaf is handed to the sanitizer with the `path` of the nearest
//! enclosing object that names one (a returned file's content, a search
//! match's line): the path says what language the text is in, and nothing
//! else about the net is relaxed.
//!
//! Redaction is a no-op unless a real secret matches, so scanning continuation
//! and location leaves is safe: it fires only on an actual secret (better to
//! break a continuation than to leak a credential), and the `tool` field that
//! selects the next call is always left verbatim.
//!
//! A secret is often recognisable only by its context (`awsSecret = "…"`),
//! while another leaf of the same response repeats the bare value (an
//! astSearch capture, a hover fragment). Every value the sanitizer redacted
//! is therefore redacted again wherever it recurs in the response.
use serde_json::{Map, Value};

/// Shortest redacted value propagated to other leaves; shorter values are too
/// generic to identify as the same secret.
const MIN_PROPAGATED_SECRET_CHARS: usize = 8;

/// `sanitize` gets each string leaf and the file path of its enclosing result, if any.
pub(crate) fn sanitize_json<E>(
    value: &mut Value,
    sanitize: &mut impl FnMut(&str, Option<&str>) -> Result<String, E>,
) -> Result<(), E> {
    let mut redacted = Vec::new();
    walk(value, None, &mut |text: &str, path: Option<&str>| {
        let clean = sanitize(text, path)?;
        if clean != text {
            redacted_values(text, &clean, &mut redacted);
        }
        Ok(clean)
    })?;
    if redacted.is_empty() {
        return Ok(());
    }
    // Longest first, so a value never leaves the tail of a longer one behind.
    redacted.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
    redacted.dedup_by(|a, b| a.0 == b.0);
    walk(value, None, &mut |text: &str, _: Option<&str>| {
        let mut text = text.to_owned();
        for (secret, placeholder) in &redacted {
            if text.contains(secret.as_str()) {
                text = text.replace(secret.as_str(), placeholder);
            }
        }
        Ok(text)
    })
}

/// The original values behind each placeholder of `clean`, paired with that
/// placeholder. Redaction keeps line count, so lines align one to one; a line
/// the redaction emptied (a multi-line secret's body) is wholly secret.
fn redacted_values(original: &str, clean: &str, out: &mut Vec<(String, String)>) {
    let original_lines: Vec<&str> = original.split('\n').collect();
    let clean_lines: Vec<&str> = clean.split('\n').collect();
    if original_lines.len() != clean_lines.len() {
        return;
    }
    let mut last_placeholder: Option<&str> = None;
    for (before, after) in original_lines.into_iter().zip(clean_lines) {
        if before == after {
            continue;
        }
        if after.trim().is_empty() {
            if let Some(placeholder) = last_placeholder {
                push_value(before.trim(), placeholder, out);
            }
            continue;
        }
        let placeholders: Vec<_> = octocode_engine::security::placeholder_regex()
            .find_iter(after)
            .collect();
        let Some(last) = placeholders.last() else {
            continue;
        };
        last_placeholder = Some(last.as_str());
        let Some(rest) = before.strip_prefix(&after[..placeholders[0].start()]) else {
            continue;
        };
        let tail = &after[last.end()..];
        let Some(mut rest) = rest.strip_suffix(tail) else {
            continue;
        };
        // Between two placeholders the kept text separates their values; an
        // empty separator leaves the two values joined under the first.
        for (index, placeholder) in placeholders.iter().enumerate() {
            let separator = placeholders
                .get(index + 1)
                .map(|next| &after[placeholder.end()..next.start()])
                .filter(|separator| !separator.is_empty());
            match separator.and_then(|separator| rest.find(separator).map(|at| (at, separator))) {
                Some((at, separator)) => {
                    push_value(&rest[..at], placeholder.as_str(), out);
                    rest = &rest[at + separator.len()..];
                }
                None => {
                    push_value(rest, placeholder.as_str(), out);
                    break;
                }
            }
        }
    }
}

fn push_value(value: &str, placeholder: &str, out: &mut Vec<(String, String)>) {
    if value.chars().count() >= MIN_PROPAGATED_SECRET_CHARS
        && value.chars().any(char::is_alphanumeric)
        && !octocode_engine::security::placeholder_regex().is_match(value)
    {
        out.push((value.to_owned(), placeholder.to_owned()));
    }
}

fn walk<E>(
    value: &mut Value,
    path: Option<&str>,
    sanitize: &mut impl FnMut(&str, Option<&str>) -> Result<String, E>,
) -> Result<(), E> {
    match value {
        Value::String(text) => {
            *text = sanitize(text, path)?;
            Ok(())
        }
        Value::Array(values) => {
            for child in values {
                walk(child, path, sanitize)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            // Read before any redaction rewrites the entry: only its extension
            // is consulted.
            let own = map.get("path").and_then(Value::as_str).map(str::to_owned);
            let path = own.as_deref().or(path);
            sanitize_keys(map, sanitize)?;
            for (key, child) in map.iter_mut() {
                match key.as_str() {
                    // `location` is scanned like any other subtree — its string
                    // leaves (e.g. a cloned localPath) can carry a secret.
                    "next" => sanitize_next_map(child, path, sanitize)?,
                    _ => walk(child, path, sanitize)?,
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Redact secrets in object keys (content-derived keys such as paths or ids
/// can carry one). The map is rebuilt in order only when a key changes; a
/// redacted key that collides with another gets a ` #n` suffix so no entry
/// is dropped.
fn sanitize_keys<E>(
    map: &mut Map<String, Value>,
    sanitize: &mut impl FnMut(&str, Option<&str>) -> Result<String, E>,
) -> Result<(), E> {
    let mut renamed = Vec::new();
    for (index, key) in map.keys().enumerate() {
        // A key is a name, never a file's text: no path context.
        let clean = sanitize(key, None)?;
        if clean != *key {
            renamed.push((index, clean));
        }
    }
    if renamed.is_empty() {
        return Ok(());
    }
    let mut renamed = renamed.into_iter().peekable();
    let entries = std::mem::take(map);
    for (index, (key, value)) in entries.into_iter().enumerate() {
        let key = match renamed.next_if(|(at, _)| *at == index) {
            Some((_, clean)) => clean,
            None => key,
        };
        let mut unique = key.clone();
        let mut n = 2;
        while map.contains_key(&unique) {
            unique = format!("{key} #{n}");
            n += 1;
        }
        map.insert(unique, value);
    }
    Ok(())
}

fn sanitize_next_map<E>(
    next: &mut Value,
    path: Option<&str>,
    sanitize: &mut impl FnMut(&str, Option<&str>) -> Result<String, E>,
) -> Result<(), E> {
    let Some(map) = next.as_object_mut() else {
        return walk(next, path, sanitize);
    };
    // Flat form: `next` is itself a single executable call (`{tool, query, …}`).
    if map.get("tool").is_some_and(Value::is_string) {
        return sanitize_call_fields(map, path, sanitize);
    }
    // Named-call form: each value under `next` is a call object.
    for call in map.values_mut() {
        match call.as_object_mut() {
            Some(fields) => sanitize_call_fields(fields, path, sanitize)?,
            None => walk(call, path, sanitize)?,
        }
    }
    Ok(())
}

/// Sanitize every leaf of a continuation call (`query`, `why`, …) except the
/// `tool` identifier, which must survive verbatim so the call stays executable.
fn sanitize_call_fields<E>(
    fields: &mut Map<String, Value>,
    path: Option<&str>,
    sanitize: &mut impl FnMut(&str, Option<&str>) -> Result<String, E>,
) -> Result<(), E> {
    for (key, value) in fields.iter_mut() {
        if key == "tool" {
            continue;
        }
        walk(value, path, sanitize)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::sanitize_json;
    use serde_json::json;

    fn mask(text: &str, _: Option<&str>) -> Result<String, ()> {
        Ok(text.replace("secret", "[MASKED]"))
    }

    #[test]
    fn redacts_content_and_query_and_location_but_preserves_tool() {
        let mut value = json!({
            "title": "secret",
            "nested": [{"body": "a secret value"}],
            "next": {
                "continueWindow": {
                    "tool": "secret-tool",
                    "query": {"path": "secret.rs"},
                    "confidence": "exact",
                    "why": "because secret"
                }
            },
            "location": {"localPath": "/tmp/secret/repo"}
        });
        sanitize_json(&mut value, &mut mask).expect("sanitize");
        assert_eq!(value["title"], "[MASKED]");
        assert_eq!(value["nested"][0]["body"], "a [MASKED] value");
        // The executable tool identifier is preserved verbatim…
        assert_eq!(value["next"]["continueWindow"]["tool"], "secret-tool");
        // …but query, why, and location string leaves are now scanned.
        assert_eq!(
            value["next"]["continueWindow"]["query"]["path"],
            "[MASKED].rs"
        );
        assert_eq!(value["next"]["continueWindow"]["why"], "because [MASKED]");
        assert_eq!(value["location"]["localPath"], "/tmp/[MASKED]/repo");
    }

    #[test]
    fn flat_next_preserves_tool_but_scans_query() {
        let mut value = json!({
            "next": {"tool": "secret-tool", "query": {"path": "secret.rs"}}
        });
        sanitize_json(&mut value, &mut mask).expect("sanitize");
        // Tool preserved so the flat continuation stays executable…
        assert_eq!(value["next"]["tool"], "secret-tool");
        // …while the query leaf is scanned for secrets.
        assert_eq!(value["next"]["query"]["path"], "[MASKED].rs");
    }

    /// An object key can be content (a path, an id); a secret in one
    /// is redacted like a leaf, in place, and every value survives even
    /// when two keys redact to the same text.
    #[test]
    fn redacts_secret_object_keys_and_keeps_every_entry() {
        let mut value = json!({
            "a": 1,
            "secret-one": {"secret-two": "x"},
            "[MASKED]-one": 3,
            "z": 4
        });
        sanitize_json(&mut value, &mut mask).expect("sanitize");
        let keys: Vec<_> = value.as_object().expect("object").keys().cloned().collect();
        assert_eq!(keys.len(), 4, "{value}");
        assert!(keys.iter().all(|key| !key.contains("secret")), "{value}");
        assert_eq!(keys[0], "a");
        assert_eq!(keys[3], "z");
        assert_eq!(value["[MASKED]-one"]["[MASKED]-two"], "x", "{value}");
        let values: Vec<_> = value
            .as_object()
            .expect("object")
            .values()
            .cloned()
            .collect();
        assert!(values.contains(&json!(3)), "{value}");
    }

    /// A value redacted where its key context shows (a match `value`)
    /// is redacted where a sibling repeats it bare (a capture), on every
    /// placeholder of a line and on the body lines of a multi-line secret.
    #[test]
    fn redacted_value_is_redacted_wherever_it_recurs() {
        let secret = "c7Tvo+hBKqFYY+kv5ZJr3J1TWDtk";
        let other = "Zm9vYmFyYmF6cXV4Zm9v";
        let body = "MIIEvQIBADANBgkqhkiG9w0BAQEFAASC";
        let mut value = json!({
            "value": format!("const awsSecret = \"{secret}\", t = '{other}';"),
            "block": format!("-----BEGIN KEY-----\n{body}\n-----END KEY-----"),
            "metavarRanges": {"V": [{"text": format!("\"{secret}\"")}], "W": [other]},
            "elsewhere": format!("line: {body}"),
            "next": {"tool": "astSearch", "query": {"pattern": secret}}
        });
        let mut detector = |text: &str, _: Option<&str>| -> Result<String, ()> {
            Ok(text
                .replace(&format!("\"{secret}\""), "\"[REDACTED-ENVVARSECRETS]\"")
                .replace(&format!("'{other}'"), "'[REDACTED-OTHER]'")
                .replace(
                    &format!("-----BEGIN KEY-----\n{body}\n-----END KEY-----"),
                    "[REDACTED-RSAPRIVATEKEY]\n\n",
                ))
        };
        sanitize_json(&mut value, &mut detector).expect("sanitize");
        let rendered = value.to_string();
        assert!(!rendered.contains(secret), "{rendered}");
        assert!(!rendered.contains(other), "{rendered}");
        assert!(!rendered.contains(body), "{rendered}");
        assert_eq!(
            value["metavarRanges"]["V"][0]["text"],
            "\"[REDACTED-ENVVARSECRETS]\""
        );
        assert_eq!(value["metavarRanges"]["W"][0], "[REDACTED-OTHER]");
        assert_eq!(value["next"]["tool"], "astSearch");
    }

    #[test]
    fn non_secret_leaves_pass_through_unchanged() {
        let mut value = json!({
            "next": {"continueWindow": {"tool": "localFetch", "query": {"path": "src/a.rs"}}},
            "location": {"localPath": "/tmp/clean/repo"}
        });
        sanitize_json(&mut value, &mut mask).expect("sanitize");
        assert_eq!(value["next"]["continueWindow"]["query"]["path"], "src/a.rs");
        assert_eq!(value["location"]["localPath"], "/tmp/clean/repo");
    }

    /// A leaf is sanitized knowing the `path` of its nearest enclosing object;
    /// a nested path overrides, keys and the path itself carry none of their
    /// own, and a leaf outside any path gets none.
    #[test]
    fn leaves_see_the_path_of_their_nearest_enclosing_object() {
        let mut value = json!({
            "outside": "o",
            "files": [{
                "path": "src/a.ts",
                "content": "c",
                "matches": [{"value": "m", "enclosing": {"path": "b.yaml", "value": "e"}}]
            }]
        });
        let mut seen = Vec::new();
        let mut record = |text: &str, path: Option<&str>| -> Result<String, ()> {
            seen.push((text.to_owned(), path.map(str::to_owned)));
            Ok(text.to_owned())
        };
        sanitize_json(&mut value, &mut record).expect("sanitize");
        let path_of = |text: &str| {
            seen.iter()
                .find(|(leaf, _)| leaf == text)
                .map(|(_, path)| path.clone())
                .expect(text)
        };
        assert_eq!(path_of("outside"), None);
        assert_eq!(path_of("c").as_deref(), Some("src/a.ts"));
        assert_eq!(path_of("m").as_deref(), Some("src/a.ts"));
        assert_eq!(path_of("e").as_deref(), Some("b.yaml"));
        assert_eq!(path_of("content"), None, "keys carry no path");
    }
}

#[cfg(test)]
mod placeholder_coverage_tests {
    #[test]
    fn placeholder_regex_matches_every_emitted_placeholder() {
        let pattern = octocode_engine::security::placeholder_regex();
        for placeholder in [
            octocode_engine::security::redaction_placeholder("githubTokens"),
            "[CONTENT-REDACTED-SIZE-LIMIT]".to_owned(),
            "[CONTENT-REDACTED-SANITIZER-FAILURE]".to_owned(),
        ] {
            assert_eq!(
                pattern.find(&placeholder).map(|found| found.as_str()),
                Some(placeholder.as_str())
            );
        }
    }
}
