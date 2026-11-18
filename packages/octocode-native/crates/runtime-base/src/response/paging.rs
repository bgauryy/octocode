//! The one owner of numeric continuation cursors (`page`,
//! `importerPage`, `scanOffset`, …) and of the partial marks a row ends
//! with. A cursor is a private position with no schema ceiling: every
//! continuation names the next one, however far the result runs. A real
//! provider stop (GitHub's search window, a registry's paging range) is
//! disclosed by the tool that knows the provider's rule, never here.
pub use octocode_contract_types::PartialReason;
use serde_json::{Map, Value, json};

use crate::contracts::envelope::NOTICES;
use crate::id::ToolId;

/// What a candidate page changes in its query: private progress dropped
/// (never the scan's `snapshot` pin, which the page keeps), the
/// `selection` token set, and a newer `snapshot` when the caller has one.
pub fn candidate_page_changes(tool: ToolId, selection: &str, snapshot: Option<&str>) -> Value {
    let mut changes = Map::new();
    for field in crate::contracts::state_fields(tool)
        .iter()
        .filter(|field| *field != "snapshot")
    {
        changes.insert(field.clone(), Value::Null);
    }
    if let Some(snapshot) = snapshot {
        changes.insert("snapshot".into(), json!(snapshot));
    }
    changes.insert("selection".into(), json!(selection));
    Value::Object(changes)
}

/// The continuation that judges the candidates a page left unjudged: `query`
/// with [`candidate_page_changes`], replayed as `tool`. `None` only when
/// `query` is not an object.
pub fn next_candidate_page_call(
    tool: ToolId,
    query: Value,
    selection: &str,
    snapshot: Option<&str>,
) -> Option<Value> {
    query.is_object().then(|| {
        let row = crate::response::result::with_changes(
            query,
            &candidate_page_changes(tool, selection, snapshot),
        );
        crate::response::result::Continuation::continued(tool, row).build()
    })
}

/// A row the partial owners write: a JSON row, or its object a tool is
/// still building. A non-object row is left alone.
pub trait RowFields {
    fn fields(&mut self) -> Option<&mut Map<String, Value>>;
}

impl RowFields for Value {
    fn fields(&mut self) -> Option<&mut Map<String, Value>> {
        self.as_object_mut()
    }
}

impl RowFields for Map<String, Value> {
    fn fields(&mut self) -> Option<&mut Map<String, Value>> {
        Some(self)
    }
}

impl<T: RowFields + ?Sized> RowFields for &mut T {
    fn fields(&mut self) -> Option<&mut Map<String, Value>> {
        (**self).fields()
    }
}

/// Appends each of `values` not already in the list under `key`, creating
/// the list (or replacing a non-list) for a first value.
fn append_once<'a>(
    object: &mut Map<String, Value>,
    key: &str,
    values: impl IntoIterator<Item = &'a str>,
) {
    let mut values = values.into_iter().peekable();
    if values.peek().is_none() {
        return;
    }
    let entry = object.entry(key).or_insert_with(|| json!([]));
    if !entry.is_array() {
        *entry = json!([]);
    }
    if let Some(list) = entry.as_array_mut() {
        for value in values {
            if !list.iter().any(|known| known == value) {
                list.push(json!(value));
            }
        }
    }
}

/// The one writer of `isPartial`: sets it to `partial`. A row that holds
/// more than its page shows, or stopped for no declared reason, is marked
/// with it alone; [`mark_partial`] adds the reason.
pub fn set_partial(row: &mut impl RowFields, partial: bool) {
    if let Some(object) = row.fields() {
        object.insert("isPartial".into(), json!(partial));
    }
}

/// Marks `row` partial for `reason`, a declared contract value: `isPartial`,
/// the reason once after the row's own `partialReasons`, and each warning
/// once in `warnings`.
pub fn mark_partial(row: &mut impl RowFields, reason: PartialReason, warnings: &[String]) {
    let Some(object) = row.fields() else {
        return;
    };
    object.insert("isPartial".into(), json!(true));
    append_once(object, "partialReasons", [reason.to_string().as_str()]);
    append_once(object, "warnings", warnings.iter().map(String::as_str));
}

/// Appends each of `warnings` once to the row's `warnings`, creating the
/// list only for a first warning.
pub fn push_warnings(row: &mut impl RowFields, warnings: impl IntoIterator<Item = String>) {
    let Some(object) = row.fields() else {
        return;
    };
    let warnings = warnings.into_iter().collect::<Vec<_>>();
    append_once(object, "warnings", warnings.iter().map(String::as_str));
}

/// Marks `row` as ending at a limit no continuation lifts: [`mark_partial`]
/// plus `terminalLimit:true`.
pub fn terminal_limit(row: &mut impl RowFields, reason: PartialReason, warnings: &[String]) {
    mark_partial(row, reason, warnings);
    if let Some(object) = row.fields() {
        object.insert("terminalLimit".into(), json!(true));
    }
}

/// Writes a static notice: a note that reads the same on every page and
/// every row of a result (a scope or parse caveat, a classification
/// reminder). The response stage says it once, on the first page only
/// (`response::notices`): on the first row that carries it, or in the
/// envelope's `warnings` when two or more rows with evidence carry it. A
/// continuation page never repeats it. A note that changes between pages
/// (a count, a line range) is a warning ([`push_warnings`]).
pub fn push_notice(row: &mut impl RowFields, notice: impl Into<String>) {
    let Some(object) = row.fields() else {
        return;
    };
    let notice = notice.into();
    append_once(object, NOTICES, [notice.as_str()]);
}

/// Whether `query` continues an earlier page of its result: a page-number
/// cursor past 1, an offset cursor past 0, or a cursor token (`snapshot`,
/// `selection`, …) it carries ([`crate::contracts::state_fields`]).
pub fn continues_a_page(tool: ToolId, query: &Value) -> bool {
    crate::contracts::state_fields(tool).iter().any(|field| {
        query.get(field).is_some_and(|value| {
            !value.is_null()
                && crate::contracts::query_schema_value(
                    tool,
                    query.get("operation").and_then(Value::as_str),
                    field,
                    "default",
                ) != Some(value)
        })
    })
}

#[cfg(test)]
mod tests;
