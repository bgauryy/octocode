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
            "nextPage": {
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
    assert_eq!(value["next"]["nextPage"]["tool"], "secret-tool");
    // …but query, why, and location string leaves are now scanned.
    assert_eq!(value["next"]["nextPage"]["query"]["path"], "[MASKED].rs");
    assert_eq!(value["next"]["nextPage"]["why"], "because [MASKED]");
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
        "next": {"nextPage": {"tool": "localFetch", "query": {"path": "src/a.rs"}}},
        "location": {"localPath": "/tmp/clean/repo"}
    });
    sanitize_json(&mut value, &mut mask).expect("sanitize");
    assert_eq!(value["next"]["nextPage"]["query"]["path"], "src/a.rs");
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
