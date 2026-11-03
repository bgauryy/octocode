//! The generated query a continuation row denotes: leads built from run-time
//! values (a path, a SHA, a page) decode them through the contract's own
//! types, so a row the contract rejects never becomes a call.
use super::remove_nulls;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// `row` as the generated query type `Q`, with the contract's defaults
/// applied. A row the type rejects (an empty name, a zero page) yields no
/// query: its lead is dropped, and debug and test builds panic at the call
/// site so the bad row is found where it was built.
#[track_caller]
pub(crate) fn wire_query<Q: DeserializeOwned>(row: Value) -> Option<Q> {
    match serde_json::from_value(row) {
        Ok(query) => Some(query),
        Err(error) => {
            debug_assert!(
                false,
                "a continuation row breaks its query contract at {}: {error}",
                std::panic::Location::caller()
            );
            None
        }
    }
}

/// `row` with `changes` applied: each change sets its field, and a `null`
/// change drops it.
pub(crate) fn with_changes(mut row: Value, changes: &Value) -> Value {
    if let (Some(to), Some(from)) = (row.as_object_mut(), changes.as_object()) {
        for (field, value) in from {
            if value.is_null() {
                to.shift_remove(field);
            } else {
                to.insert(field.clone(), value.clone());
            }
        }
    }
    row
}

/// `query` with `changes` applied (a `null` change drops that field),
/// decoded again as the same generated type: a field the type does not
/// have or a value it rejects is an error at the editing call site. An
/// interim for edits whose fields are chosen at run time; a fixed edit is a
/// struct update on a named variant.
pub(crate) fn edited_query<Q: Serialize + serde::de::DeserializeOwned>(
    query: &Q,
    changes: &Value,
) -> Result<Q, serde_json::Error> {
    serde_json::from_value(with_changes(query_row(query, &[]), changes))
}

/// A generated query as JSON, every field as written. A generated query is
/// plain data, so serializing it cannot fail.
pub(crate) fn query_value(query: &impl Serialize) -> Value {
    serde_json::to_value(query).unwrap_or_default()
}

/// A typed query as a continuation row: every field it sets, except `omit`.
pub(crate) fn query_row(query: &impl Serialize, omit: &[&str]) -> Value {
    let mut row = query_value(query);
    remove_nulls(&mut row);
    if let Some(row) = row.as_object_mut() {
        for key in omit {
            row.remove(*key);
        }
    }
    row
}
