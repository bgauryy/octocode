use serde_json::Value;

pub fn assert_cursor_only(input: &Value) {
    assert_eq!(
        input
            .as_object()
            .map(|object| object.keys().map(String::as_str).collect::<Vec<_>>()),
        Some(vec!["cursor"]),
        "{input}"
    );
    assert!(
        input["cursor"]
            .as_str()
            .is_some_and(|token| !token.is_empty() && token.len() <= 128),
        "{input}"
    );
}
