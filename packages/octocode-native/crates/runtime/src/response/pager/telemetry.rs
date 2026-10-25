use serde_json::{Map, Value};

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
