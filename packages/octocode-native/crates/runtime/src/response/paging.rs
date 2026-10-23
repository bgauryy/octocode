//! The one owner of numeric continuation cursors (`page`, `matchPage`,
//! `importerPage`, `scanOffset`, …): a continuation names a cursor value only
//! when the tool's query schema admits it. Past the schema `maximum` the row
//! ends at a terminal limit instead, written by [`terminal_limit`], the one
//! writer of `terminalLimit`, so the rest is stated as unreachable rather
//! than offered by a lead the tool would reject.
use serde_json::{Map, Value, json};

use crate::id::ToolId;

/// The `partialReasons` entry of a row whose next cursor is past the
/// contract maximum of its field.
pub(crate) const CURSOR_MAXIMUM: &str = "cursorMaximum";

/// `next` when `tool`'s query schema (the variant admitting `operation`)
/// admits it as the value of the cursor `field`: at most its `maximum`.
pub(crate) fn admitted(
    tool: ToolId,
    operation: Option<&str>,
    field: &str,
    next: usize,
) -> Option<usize> {
    (next <= crate::contracts::query_schema_max(tool, operation, field)).then_some(next)
}

/// Rows per page: `requested`, at least one and at most the contract maximum
/// of `tool`'s `pageSize` (the variant admitting `operation`). The one clamp
/// every tool's page-size accessor applies, so no site can forget the bound.
pub(crate) fn clamp_page_size(tool: ToolId, operation: Option<&str>, requested: usize) -> usize {
    requested.clamp(
        1,
        crate::contracts::query_schema_max(tool, operation, "pageSize"),
    )
}

/// The rows a page holds: the caller's `requested` `pageSize` clamped to the
/// contract bound ([`clamp_page_size`]), or `None` for the character budget
/// that fills the response window ([`crate::response::page_windows::Paging::of`]).
/// The one page-size accessor of a tool.
pub(crate) fn page_size(
    tool: ToolId,
    operation: Option<&str>,
    requested: Option<impl Into<u64>>,
) -> Option<usize> {
    // Generated query types carry u32 or u64 sizes; both clamp to the contract bound.
    requested.map(|size| {
        let size = usize::try_from(size.into()).unwrap_or(usize::MAX);
        clamp_page_size(tool, operation, size)
    })
}

/// `query` with cursor `field` set to `next` and each `(field, value)` of
/// `set` written after it; `None` when `query` is not an object.
fn with_cursor(mut query: Value, field: &str, next: usize, set: &[(&str, Value)]) -> Option<Value> {
    let row = query.as_object_mut()?;
    row.insert(field.into(), json!(next));
    for (field, value) in set {
        row.insert((*field).into(), value.clone());
    }
    Some(query)
}

/// Clone a continuation query with one admitted cursor advance and any
/// numeric cursors that must restart when that outer axis advances.
pub(crate) fn admitted_query(
    tool: ToolId,
    operation: Option<&str>,
    base: &Value,
    field: &str,
    current: u32,
    resets: &[(&str, u64)],
) -> Option<Value> {
    let next = admitted(tool, operation, field, current.checked_add(1)? as usize)?;
    let resets = resets
        .iter()
        .map(|(field, value)| (*field, json!(value)))
        .collect::<Vec<_>>();
    with_cursor(base.clone(), field, next, &resets)
}

/// The continuation call that reads cursor `field` at `next`: `query` (the
/// complete query row the continuation repeats) with `field` advanced and
/// each of `set` written (`snapshot`, an outer cursor's reset), built as an
/// exact replay of `tool`. The one next-page builder: the cursor is
/// admitted by the contract maximum ([`next_cursor`]), so a cursor past it
/// ends `row` at a terminal limit and returns `None`.
pub(crate) fn next_page_call(
    row: &mut Value,
    tool: ToolId,
    operation: Option<&str>,
    query: Value,
    field: &str,
    next: usize,
    set: &[(&str, Value)],
) -> Option<Value> {
    let next = next_cursor(row, tool, operation, field, next)?;
    let query = with_cursor(query, field, next, set)?;
    Some(crate::response::result::Continuation::new(tool, query).build())
}

/// The cursor value `next` of `field` when [`admitted`]; otherwise the
/// warning that names the field and its maximum, a [`CURSOR_MAXIMUM`]
/// terminal limit: the caller offers no continuation.
pub(crate) fn cursor_value(
    tool: ToolId,
    operation: Option<&str>,
    field: &str,
    next: usize,
) -> Result<usize, String> {
    admitted(tool, operation, field, next).ok_or_else(|| {
        let maximum = crate::contracts::query_schema_max(tool, operation, field);
        format!(
            "terminalLimit: {field}:{maximum} is the last a continuation may request; the rest is on no page."
        )
    })
}

/// [`cursor_value`], marking `row` as ending at the terminal limit when the
/// cursor is past its maximum.
pub(crate) fn next_cursor(
    row: &mut Value,
    tool: ToolId,
    operation: Option<&str>,
    field: &str,
    next: usize,
) -> Option<usize> {
    cursor_value(tool, operation, field, next)
        .map_err(|warning| terminal_limit(row, CURSOR_MAXIMUM, &[warning]))
        .ok()
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

/// Marks `row` partial for `reason`: `isPartial`, the reason once after the
/// row's own `partialReasons`, and each warning once in `warnings`.
pub(crate) fn mark_partial(row: &mut impl RowFields, reason: &str, warnings: &[String]) {
    let Some(object) = row.fields() else {
        return;
    };
    object.insert("isPartial".into(), json!(true));
    append_once(object, "partialReasons", [reason]);
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
pub(crate) fn terminal_limit(row: &mut impl RowFields, reason: &str, warnings: &[String]) {
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
pub(crate) fn continues_a_page(query: &Value) -> bool {
    crate::response::result::CURSOR_FIELDS.iter().any(|field| {
        let Some(value) = query.get(*field).filter(|value| !value.is_null()) else {
            return false;
        };
        let lower = field.to_ascii_lowercase();
        match value.as_u64() {
            Some(page) if lower.ends_with("page") => page > 1,
            Some(offset) if lower.ends_with("offset") => offset > 0,
            _ => true,
        }
    })
}

#[cfg(test)]
mod tests;
