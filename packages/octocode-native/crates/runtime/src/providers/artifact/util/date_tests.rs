//! Date prefixes from registry timestamps.
use super::*;

#[test]
fn dates_from_millis_and_timestamps() {
    assert_eq!(
        crate::civil_date::date_millis(1_789_341_914_484),
        "2026-09-13"
    );
    assert_eq!(
        date_prefix(Some(&Value::from("2019-09-08T01:56:06.955881Z"))).as_deref(),
        Some("2019-09-08")
    );
    assert_eq!(date_prefix(Some(&Value::from("soon"))), None);
}
