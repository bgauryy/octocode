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
use std::collections::HashMap;

/// Shortest redacted value propagated to other leaves; shorter values are too
/// generic to identify as the same secret.
const MIN_PROPAGATED_SECRET_CHARS: usize = 8;

/// What a sanitizer returns for one string: its sanitized text (`String`),
/// or that text only when it differs (`Option`, `None`: unchanged).
pub(crate) trait Rewrite {
    /// The new text; `None` when it equals `original`.
    fn change(self, original: &str) -> Option<String>;
}

impl Rewrite for String {
    fn change(self, original: &str) -> Option<String> {
        (self != original).then_some(self)
    }
}

impl Rewrite for Option<String> {
    fn change(self, original: &str) -> Option<String> {
        self.filter(|text| text != original)
    }
}

/// Each object key's change within one walk, by key text. A sanitizer is a
/// pure function of its text and path, and a key is scanned without a path,
/// so a key every row repeats (`line`, `value`, `path`) is scanned once.
type KeyChanges = HashMap<String, Option<String>>;

/// `sanitize` gets each string leaf and the file path of its enclosing result,
/// if any, and must be a pure function of the two: an object key repeated in
/// the value is handed to it once.
pub(crate) fn sanitize_json<E, R: Rewrite>(
    value: &mut Value,
    sanitize: &mut impl FnMut(&str, Option<&str>) -> Result<R, E>,
) -> Result<(), E> {
    let mut redacted = Vec::new();
    walk(value, None, &mut KeyChanges::new(), &mut |text: &str,
                                                    path: Option<
        &str,
    >| {
        let clean = sanitize(text, path)?.change(text);
        if let Some(clean) = &clean {
            redacted_values(text, clean, &mut redacted);
        }
        Ok(clean)
    })?;
    if redacted.is_empty() {
        return Ok(());
    }
    // Longest first, so a value never leaves the tail of a longer one behind.
    redacted.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
    redacted.dedup_by(|a, b| a.0 == b.0);
    walk(value, None, &mut KeyChanges::new(), &mut |text: &str,
                                                    _: Option<
        &str,
    >| {
        let mut changed: Option<String> = None;
        for (secret, placeholder) in &redacted {
            let current = changed.as_deref().unwrap_or(text);
            if current.contains(secret.as_str()) {
                changed = Some(current.replace(secret.as_str(), placeholder));
            }
        }
        Ok(changed)
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

/// `sanitize` returns a string's new text, `None` when it is unchanged.
fn walk<E>(
    value: &mut Value,
    path: Option<&str>,
    keys: &mut KeyChanges,
    sanitize: &mut impl FnMut(&str, Option<&str>) -> Result<Option<String>, E>,
) -> Result<(), E> {
    match value {
        Value::String(text) => {
            if let Some(clean) = sanitize(text, path)? {
                *text = clean;
            }
            Ok(())
        }
        Value::Array(values) => {
            for child in values {
                walk(child, path, keys, sanitize)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            // Read before any redaction rewrites the entry: only its extension
            // is consulted.
            let own = map.get("path").and_then(Value::as_str).map(str::to_owned);
            let path = own.as_deref().or(path);
            sanitize_keys(map, keys, sanitize)?;
            for (key, child) in map.iter_mut() {
                match key.as_str() {
                    // `location` is scanned like any other subtree — its string
                    // leaves (e.g. a cloned localPath) can carry a secret.
                    "next" => sanitize_next_map(child, path, keys, sanitize)?,
                    _ => walk(child, path, keys, sanitize)?,
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
    keys: &mut KeyChanges,
    sanitize: &mut impl FnMut(&str, Option<&str>) -> Result<Option<String>, E>,
) -> Result<(), E> {
    let mut renamed = Vec::new();
    for (index, key) in map.keys().enumerate() {
        let change = match keys.get(key) {
            Some(change) => change.clone(),
            None => {
                // A key is a name, never a file's text: no path context.
                let change = sanitize(key, None)?;
                keys.insert(key.clone(), change.clone());
                change
            }
        };
        if let Some(clean) = change {
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
    keys: &mut KeyChanges,
    sanitize: &mut impl FnMut(&str, Option<&str>) -> Result<Option<String>, E>,
) -> Result<(), E> {
    let Some(map) = next.as_object_mut() else {
        return walk(next, path, keys, sanitize);
    };
    // Flat form: `next` is itself a single executable call (`{tool, query, …}`).
    if map.get("tool").is_some_and(Value::is_string) {
        return sanitize_call_fields(map, path, keys, sanitize);
    }
    // Named-call form: each value under `next` is a call object.
    for call in map.values_mut() {
        match call.as_object_mut() {
            Some(fields) => sanitize_call_fields(fields, path, keys, sanitize)?,
            None => walk(call, path, keys, sanitize)?,
        }
    }
    Ok(())
}

/// Sanitize every leaf of a continuation call (`query`, `why`, …) except the
/// `tool` identifier, which must survive verbatim so the call stays executable.
fn sanitize_call_fields<E>(
    fields: &mut Map<String, Value>,
    path: Option<&str>,
    keys: &mut KeyChanges,
    sanitize: &mut impl FnMut(&str, Option<&str>) -> Result<Option<String>, E>,
) -> Result<(), E> {
    for (key, value) in fields.iter_mut() {
        if key == "tool" {
            continue;
        }
        walk(value, path, keys, sanitize)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod placeholder_coverage_tests;
