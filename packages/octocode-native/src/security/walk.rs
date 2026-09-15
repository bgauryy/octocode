//! JSON walker that redacts domain strings but never rewrites executable `next` or `location`.
use serde_json::Value;

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
                    "location" => {}
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
    for call in map.values_mut() {
        let Some(fields) = call.as_object_mut() else {
            continue;
        };
        if let Some(why) = fields.get_mut("why") {
            sanitize_json(why, sanitize)?;
        }
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
    fn redacts_content_and_preserves_next_and_location() {
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
        assert_eq!(value["next"]["continue"]["tool"], "secret-tool");
        assert_eq!(value["next"]["continue"]["query"]["path"], "secret.rs");
        assert_eq!(value["next"]["continue"]["why"], "because [MASKED]");
        assert_eq!(value["location"]["localPath"], "/tmp/secret/repo");
    }

    #[test]
    fn flat_next_tool_query_is_left_unchanged() {
        let mut value = json!({
            "next": {"tool": "secret-tool", "query": {"path": "secret.rs"}}
        });
        sanitize_json(&mut value, &mut mask).expect("sanitize");
        assert_eq!(value["next"]["tool"], "secret-tool");
        assert_eq!(value["next"]["query"]["path"], "secret.rs");
    }
}
