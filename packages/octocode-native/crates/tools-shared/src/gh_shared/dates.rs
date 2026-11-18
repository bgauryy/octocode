//! The one way every GitHub tool shows a provider timestamp: in UTC (`…Z`),
//! whole or as its day.
use serde_json::{Value, json};

/// A provider timestamp in UTC (`…Z`); text that is not a full timestamp
/// passes through, and no value is empty text.
pub fn utc_date(value: Option<&str>) -> String {
    let value = value.unwrap_or("");
    octocode_runtime_base::providers::github::utc_timestamp(value)
        .unwrap_or_else(|| value.to_owned())
}

/// A provider timestamp field as a row value: a string in UTC
/// ([`utc_date`]), any other value as sent, `null` when absent.
pub fn utc_value(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(text)) => json!(utc_date(Some(text))),
        Some(value) => value.clone(),
        None => Value::Null,
    }
}

/// The UTC day (`YYYY-MM-DD`) of a provider timestamp; `None` for text too
/// short to name one.
pub fn utc_day(value: &str) -> Option<String> {
    utc_date(Some(value)).get(..10).map(str::to_owned)
}

#[cfg(test)]
mod tests;
