use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

#[test]
fn optional_context_wraps_the_state_once_and_keeps_it_literal() {
    let state = json_fixture(r#"{"target": {"text": "fn a() {}"}, "context": []}"#);
    assert_eq!(&*contextual_state(&state, None), &state);
    assert_eq!(
        &*contextual_state(&state, Some("  Research context\n")),
        &json!({"context":"  Research context\n","content":state})
    );
}
