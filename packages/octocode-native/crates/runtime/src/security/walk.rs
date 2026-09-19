//! JSON walker that redacts every domain string leaf — including `location`
//! values and the `query`/`why` leaves of executable `next` continuations —
//! while preserving object structure and the executable `tool` identifier.
//!
//! Redaction is a no-op unless a real secret matches, so scanning continuation
//! and location leaves is safe: it fires only on an actual secret (better to
//! break a continuation than to leak a credential), and the `tool` field that
//! selects the next call is always left verbatim.
use serde_json::{Map, Value};

pub fn sanitize_json<E>(
    value: &mut Value,
    sanitize: &mut impl FnMut(&str) -> Result<String, E>,
) -> Result<(), E> {
    match value {
        Value::String(text) => {
            *text = sanitize(text)?;
            Ok(())
        }
        Value::Array(values) => {
            for child in values {
                sanitize_json(child, sanitize)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                match key.as_str() {
                    // `location` is scanned like any other subtree — its string
                    // leaves (e.g. a cloned localPath) can carry a secret.
                    "next" => sanitize_next_map(child, sanitize)?,
                    _ => sanitize_json(child, sanitize)?,
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn sanitize_next_map<E>(
    next: &mut Value,
    sanitize: &mut impl FnMut(&str) -> Result<String, E>,
) -> Result<(), E> {
    let Some(map) = next.as_object_mut() else {
        return sanitize_json(next, sanitize);
    };
    // Flat form: `next` is itself a single executable call (`{tool, query, …}`).
    if map.get("tool").is_some_and(Value::is_string) {
        return sanitize_call_fields(map, sanitize);
    }
    // Named-call form: each value under `next` is a call object.
    for call in map.values_mut() {
        match call.as_object_mut() {
            Some(fields) => sanitize_call_fields(fields, sanitize)?,
            None => sanitize_json(call, sanitize)?,
        }
    }
    Ok(())
}

/// Sanitize every leaf of a continuation call (`query`, `why`, …) except the
/// `tool` identifier, which must survive verbatim so the call stays executable.
fn sanitize_call_fields<E>(
    fields: &mut Map<String, Value>,
    sanitize: &mut impl FnMut(&str) -> Result<String, E>,
) -> Result<(), E> {
    for (key, value) in fields.iter_mut() {
        if key == "tool" {
            continue;
        }
        sanitize_json(value, sanitize)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::sanitize_json;
    use serde_json::json;

    fn mask(text: &str) -> Result<String, ()> {
        Ok(text.replace("secret", "[MASKED]"))
    }

    #[test]
    fn redacts_content_and_query_and_location_but_preserves_tool() {
        let mut value = json!({
            "title": "secret",
            "nested": [{"body": "a secret value"}],
            "next": {
                "continue": {
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
        assert_eq!(value["next"]["continue"]["tool"], "secret-tool");
        // …but query, why, and location string leaves are now scanned.
        assert_eq!(value["next"]["continue"]["query"]["path"], "[MASKED].rs");
        assert_eq!(value["next"]["continue"]["why"], "because [MASKED]");
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

    #[test]
    fn non_secret_leaves_pass_through_unchanged() {
        let mut value = json!({
            "next": {"continue": {"tool": "localFetch", "query": {"path": "src/a.rs"}}},
            "location": {"localPath": "/tmp/clean/repo"}
        });
        sanitize_json(&mut value, &mut mask).expect("sanitize");
        assert_eq!(value["next"]["continue"]["query"]["path"], "src/a.rs");
        assert_eq!(value["location"]["localPath"], "/tmp/clean/repo");
    }
}
