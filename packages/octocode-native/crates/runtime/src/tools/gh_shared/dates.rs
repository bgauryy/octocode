//! The one way every GitHub tool shows a provider timestamp: in UTC (`…Z`),
//! whole or as its day.
use serde_json::{Value, json};

/// A provider timestamp in UTC (`…Z`); text that is not a full timestamp
/// passes through, and no value is empty text.
pub(crate) fn utc_date(value: Option<&str>) -> String {
    let value = value.unwrap_or("");
    crate::providers::github::utc_timestamp(value).unwrap_or_else(|| value.to_owned())
}

/// A provider timestamp field as a row value: a string in UTC
/// ([`utc_date`]), any other value as sent, `null` when absent.
pub(crate) fn utc_value(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(text)) => json!(utc_date(Some(text))),
        Some(value) => value.clone(),
        None => Value::Null,
    }
}

/// The UTC day (`YYYY-MM-DD`) of a provider timestamp; `None` for text too
/// short to name one.
pub(crate) fn utc_day(value: &str) -> Option<String> {
    utc_date(Some(value)).get(..10).map(str::to_owned)
}

#[cfg(test)]
mod tests {
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
}
