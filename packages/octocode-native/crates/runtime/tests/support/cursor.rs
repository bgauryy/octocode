//! Whole-response cursor bytes: the saved response and offset a page names.

use serde_json::{Value, json};

/// The bytes of a page's whole-response cursor: `next` carries it while
/// pages remain, the block names its own page otherwise. White-box layout
/// (`response/cursor.rs`): key 32, content digest 32, scope 1, length 8,
/// offset 8.
fn cursor_bytes(pagination: &Value) -> Vec<u8> {
    use base64::Engine as _;
    let token = match &pagination["next"]["query"]["responseSnapshot"] {
        Value::Null => &pagination["snapshot"],
        token => token,
    };
    let token = token
        .as_str()
        .unwrap_or_else(|| panic!("no cursor: {pagination}"));
    let encoded = token
        .strip_prefix("saved-v2:")
        .unwrap_or_else(|| panic!("not a cursor: {token}"));
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .expect("base64url cursor");
    assert_eq!(bytes.len(), 81, "{token}");
    bytes
}

/// The saved response (key, content, scope, page length) a page's cursor
/// names: equal for every page of one response.
pub fn cursor_response(pagination: &Value) -> Vec<u8> {
    cursor_bytes(pagination)[..73].to_vec()
}

/// The page offset a page's cursor names.
pub fn cursor_offset(pagination: &Value) -> u64 {
    u64::from_be_bytes(
        cursor_bytes(pagination)[73..]
            .try_into()
            .expect("eight offset bytes"),
    )
}

/// `next.query` with its cursor moved to `offset`.
pub fn cursor_at_offset(pagination: &Value, offset: u64) -> Value {
    use base64::Engine as _;
    let mut bytes = cursor_bytes(pagination);
    bytes[73..].copy_from_slice(&offset.to_be_bytes());
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    json!({"responseSnapshot": format!("saved-v2:{encoded}")})
}

/// A whole-response continuation is the opaque cursor alone.
pub fn assert_cursor_only(query: &Value) {
    assert_eq!(
        query
            .as_object()
            .map(|query| query.keys().map(String::as_str).collect::<Vec<_>>()),
        Some(vec!["responseSnapshot"]),
        "{query}"
    );
    assert!(
        query["responseSnapshot"]
            .as_str()
            .is_some_and(|token| token.len() <= 128),
        "{query}"
    );
}
