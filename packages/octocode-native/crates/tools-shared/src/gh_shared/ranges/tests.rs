use super::*;
use octocode_test_support::json_fixture;

#[test]
fn inclusive_bounds_preserve_zero_and_both_endpoints() {
    let numeric_range =
        serde_json::from_value(json_fixture(r#"{"min": 0, "max": 12}"#)).expect("range");
    assert_eq!(numeric(&numeric_range).as_deref(), Some("0..12"));
    let date_range = serde_json::from_value(json_fixture(
        r#"{
            "from": "2026-01-01T00:00:00Z",
            "to": "2026-02-01T00:00:00+02:00"
        }"#,
    ))
    .expect("range");
    assert_eq!(
        date(&date_range).as_deref(),
        Some("2026-01-01T00:00:00Z..2026-02-01T00:00:00+02:00")
    );
}
