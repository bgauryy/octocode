use serde_json::{Map, Value};

/// Row `data` fields that differ on every execution of the same request:
/// provider rate-limit state (`rateLimit`), the waits the client spent
/// before its sends (`clientWaits`), and when a reused saved answer was
/// observed (`observedAt`).
const VOLATILE_DATA_FIELDS: &[&str] = &["rateLimit", "clientWaits", "observedAt"];

/// Whether a row's `data` field rides outside the evidence measure.
pub(super) fn is_volatile_data_field(field: &str) -> bool {
    VOLATILE_DATA_FIELDS.contains(&field)
}

fn rows_mut(structured: &mut Map<String, Value>) -> impl Iterator<Item = &mut Map<String, Value>> {
    structured
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object_mut)
}

/// Chars the per-call provider facts on `structured`'s rows add to its
/// rendering. Pages are planned and measured without them: they ride the
/// first part of their row after the cut, so whether a run had them never
/// moves a page edge, and as metadata they never make a page `oversized`.
pub(crate) fn volatile_chars(structured: &Map<String, Value>) -> usize {
    structured
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| row.get("data").and_then(Value::as_object))
        .map(data_volatile_chars)
        .sum()
}

/// [`volatile_chars`] of one row's `data`: its whole `"field":value`
/// entries with their commas.
fn data_volatile_chars(data: &Map<String, Value>) -> usize {
    let entries = VOLATILE_DATA_FIELDS
        .iter()
        .filter_map(|field| {
            let value = octocode_engine::json_size::utf16_units(data.get(*field)?).unwrap_or(0);
            // `"field":value`.
            Some(field.len() + 3 + value)
        })
        .collect::<Vec<_>>();
    // One comma per entry, less one when nothing else is left.
    let commas = entries
        .len()
        .saturating_sub(usize::from(entries.len() == data.len()));
    entries.iter().sum::<usize>() + commas
}

/// Move each row's per-call provider facts out of the envelope, by row,
/// keeping the order of the keys left: the snapshot digest reads it.
pub(super) fn take_volatile_fields(
    structured: &mut Map<String, Value>,
) -> Vec<Option<Map<String, Value>>> {
    rows_mut(structured)
        .map(|row| {
            let data = row.get_mut("data").and_then(Value::as_object_mut)?;
            let taken: Map<String, Value> = VOLATILE_DATA_FIELDS
                .iter()
                .filter_map(|field| {
                    data.shift_remove(*field)
                        .map(|value| ((*field).to_owned(), value))
                })
                .collect();
            (!taken.is_empty()).then_some(taken)
        })
        .collect()
}
