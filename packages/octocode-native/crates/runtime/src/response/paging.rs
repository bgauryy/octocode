//! The one owner of numeric continuation cursors (`page`, `matchPage`,
//! `importerPage`, `scanOffset`, …) and of the partial marks a row ends
//! with. A cursor is a private position with no schema ceiling: every
//! continuation names the next one, however far the result runs. A real
//! provider stop (GitHub's search window, a registry's paging range) is
//! disclosed by the tool that knows the provider's rule, never here.
pub(crate) use octocode_contract_types::PartialReason;
use serde_json::{Map, Value, json};

use crate::id::ToolId;

/// The continuation call that reads cursor `field` at `next`: `query` (the
/// complete query row the continuation repeats) with `field` set and each
/// of `set` written after it (`snapshot`, an outer cursor's reset), built
/// as an exact replay of `tool`. The one next-page builder; `None` only
/// when `query` is not an object.
pub(crate) fn next_page_call(
    tool: ToolId,
    mut query: Value,
    field: &str,
    next: usize,
    set: &[(&str, Value)],
) -> Option<Value> {
    let row = query.as_object_mut()?;
    row.insert(field.into(), json!(next));
    for (field, value) in set {
        row.insert((*field).into(), value.clone());
    }
    Some(crate::response::result::Continuation::continued(tool, query).build())
}

/// A row the partial owners write: a JSON row, or its object a tool is
/// still building. A non-object row is left alone.
pub(crate) trait RowFields {
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

/// Marks `row` partial for `reason`, a declared contract value: `isPartial`,
/// the reason once after the row's own `partialReasons`, and each warning
/// once in `warnings`.
pub(crate) fn mark_partial(row: &mut impl RowFields, reason: PartialReason, warnings: &[String]) {
    let Some(object) = row.fields() else {
        return;
    };
    object.insert("isPartial".into(), json!(true));
    append_once(object, "partialReasons", [reason.to_string().as_str()]);
    append_once(object, "warnings", warnings.iter().map(String::as_str));
}

/// Appends each of `warnings` once to the row's `warnings`, creating the
/// list only for a first warning.
pub(crate) fn push_warnings(row: &mut impl RowFields, warnings: impl IntoIterator<Item = String>) {
    let Some(object) = row.fields() else {
        return;
    };
    let warnings = warnings.into_iter().collect::<Vec<_>>();
    append_once(object, "warnings", warnings.iter().map(String::as_str));
}

/// Marks `row` as ending at a limit no continuation lifts: [`mark_partial`]
/// plus `terminalLimit:true`.
pub(crate) fn terminal_limit(row: &mut impl RowFields, reason: PartialReason, warnings: &[String]) {
    mark_partial(row, reason, warnings);
    if let Some(object) = row.fields() {
        object.insert("terminalLimit".into(), json!(true));
    }
}

/// The key a static notice rides until the response stage places it.
pub(crate) const NOTICES: &str = "notices";

/// Writes a static notice: a note that reads the same on every page and
/// every row of a result (a scope or parse caveat, a classification
/// reminder). The response stage says it once, on the first row that
/// carries it and on the first page only (`response::notices`): a
/// continuation page never repeats it. A note that changes between pages
/// (a count, a line range) is a warning ([`push_warnings`]).
pub(crate) fn push_notice(row: &mut impl RowFields, notice: impl Into<String>) {
    let Some(object) = row.fields() else {
        return;
    };
    let notice = notice.into();
    append_once(object, NOTICES, [notice.as_str()]);
}

/// Whether `query` continues an earlier page of its result: a page-number
/// cursor past 1, an offset cursor past 0, or a cursor token (`snapshot`,
/// `selection`, …) it carries ([`crate::response::result::CURSOR_FIELDS`]).
pub(crate) fn continues_a_page(tool: ToolId, query: &Value) -> bool {
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
