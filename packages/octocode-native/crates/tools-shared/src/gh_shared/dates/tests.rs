use super::*;

#[test]
fn history_dates_are_utc() {
    assert_eq!(
        utc_date(Some("2024-01-01T02:30:00+02:00")),
        "2024-01-01T00:30:00Z"
    );
    assert_eq!(
        utc_date(Some("2024-01-01T00:00:00Z")),
        "2024-01-01T00:00:00Z"
    );
    assert_eq!(utc_date(Some("not a date")), "not a date");
    assert_eq!(utc_date(None), "");
}

#[test]
fn a_timestamp_value_is_utc_and_other_values_pass_through() {
    assert_eq!(
        utc_value(Some(&json!("2024-01-01T02:30:00+02:00"))),
        json!("2024-01-01T00:30:00Z")
    );
    assert_eq!(utc_value(Some(&Value::Null)), Value::Null);
    assert_eq!(utc_value(Some(&json!(3))), json!(3));
    assert_eq!(utc_value(None), Value::Null);
}

/// A day is the UTC day: an offset timestamp past midnight UTC names
/// the next day, not its local one.
#[test]
fn a_day_is_the_utc_day() {
    assert_eq!(
        utc_day("2024-01-01T23:30:00-02:00").as_deref(),
        Some("2024-01-02")
    );
    assert_eq!(
        utc_day("2024-01-01T00:00:00Z").as_deref(),
        Some("2024-01-01")
    );
    assert_eq!(utc_day("2024-01-01").as_deref(), Some("2024-01-01"));
    assert_eq!(utc_day("2024"), None);
}
