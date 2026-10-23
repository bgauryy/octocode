use serde_json::{Map, Value};

pub(super) fn rows_key(tool: &str) -> &'static str {
    crate::id::ToolId::from_name(tool).map_or("results", |tool| tool.output().rows_key())
}

/// Per-row fields that describe how a call was served (e.g. cache warmth),
/// not the evidence. A paged response drops them so a copied continuation
/// served from a warmer cache keeps the same snapshot, page plan, and bytes.
const TRANSIENT_ROW_FIELDS: &[&str] = &["cache"];

/// Row `data` fields that differ on every execution of the same request:
/// provider request ids and rate-limit state (remaining, reset, retry-after).
const VOLATILE_DATA_FIELDS: &[&str] = &["requestId", "rateLimit", "retryAfterSeconds"];

fn rows_mut(structured: &mut Map<String, Value>) -> impl Iterator<Item = &mut Map<String, Value>> {
    structured
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object_mut)
}

pub(super) fn strip_row_telemetry(structured: &mut Map<String, Value>) {
    for row in rows_mut(structured) {
        for field in TRANSIENT_ROW_FIELDS {
            row.remove(*field);
        }
    }
}

/// Remove transient telemetry and per-call provider facts from an envelope
/// whose bytes are windowed: every page must hash and slice identical bytes.
pub(crate) fn strip_transient_telemetry(structured: &mut Map<String, Value>) {
    strip_row_telemetry(structured);
    take_volatile_fields(structured);
}

/// Move each row's per-call provider facts out of the envelope, by row.
pub(super) fn take_volatile_fields(
    structured: &mut Map<String, Value>,
) -> Vec<Option<Map<String, Value>>> {
    rows_mut(structured)
        .map(|row| {
            let data = row.get_mut("data").and_then(Value::as_object_mut)?;
            let taken: Map<String, Value> = VOLATILE_DATA_FIELDS
                .iter()
                .filter_map(|field| {
                    data.remove(*field)
                        .map(|value| ((*field).to_owned(), value))
                })
                .collect();
            (!taken.is_empty()).then_some(taken)
        })
        .collect()
}
