use super::*;

#[test]
fn panic_in_one_row_does_not_abort_following_rows() {
    let rows = [true, false].map(|panics| {
        execute_row(|| {
            assert!(!panics, "injected row failure with private input");
            Ok(DomainResult::payload(serde_json::json!({"value": 7}), None))
        })
        .expect("each row returns its own result")
    });
    assert_eq!(rows[0].status, Some("error"));
    assert_eq!(rows[0].data["errorCode"], "executionFailed");
    assert!(!rows[0].data.to_string().contains("private input"));
    let message = rows[0].data["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("domain_dispatch/row_tests.rs:"),
        "the failure names the panic site: {message}"
    );
    assert_eq!(rows[1].data["value"], 7);
    assert_eq!(rows[1].status, None);
}
