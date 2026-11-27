use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

/// A tool's mark on text that shows its replacement, or text the gate
/// replaced, flags the row; a source that only spells a placeholder does
/// not, a mark whose replaced text the row no longer shows does not, and
/// an error row is never flagged.
#[test]
fn only_a_replacement_flags_a_row() {
    let mark = crate::security::redacted_mark();
    let mut envelope = json!({"results": [
        {"status": "hasResults", "data": {"content": "a [REDACTED-FOO]", "warnings": [mark, "kept"]}},
        {"status": "hasResults", "data": {"content": "x = [REDACTED-FOO]"}},
        {"status": "hasResults", "data": {"content": "gate [REDACTED-GITHUBTOKENS]"}},
        {"status": "error", "data": {"warnings": [mark]}},
        {"status": "hasResults", "data": {"content": "a page cut past it", "warnings": [mark]}},
    ]});
    let before = vec![1, 1, 0, 0, 0];
    // Rows 0, 1, 3 and 4 hold the placeholders they started with; row
    // 2's text gained one at the gate.
    warn_redacted_rows(&mut envelope, &before);
    let rows = envelope["results"].as_array().expect("rows");
    assert_eq!(rows[0]["data"]["redacted"], json!(true));
    assert_eq!(rows[0]["data"]["warnings"], json_fixture(r#"["kept"]"#));
    assert!(rows[1]["data"].get("redacted").is_none(), "{}", rows[1]);
    assert_eq!(rows[2]["data"]["redacted"], json!(true));
    assert!(rows[3]["data"].get("redacted").is_none(), "{}", rows[3]);
    assert!(rows[4]["data"].get("redacted").is_none(), "{}", rows[4]);
    assert!(rows[4]["data"].get("warnings").is_none(), "{}", rows[4]);
}
