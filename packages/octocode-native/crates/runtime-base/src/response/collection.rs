//! The one builder of a paged collection's continuation. A collection is a
//! list inside a row (core `continuationChannels.<tool>.collections`). Its
//! `pagination` object states where the page sits and `hasMore`, and never
//! holds a call: the walk is a call in the `next` of the row or element that
//! holds the list, `next.<collection>` when the tool's rows declare that name
//! as a linear channel (a section's own walk), else `next.nextPage`.
//!
//! Every tool's linear next-page call is built here: [`continued_page`]
//! replays a query at a new cursor, [`walk_calls`] and [`insert_walk`] name
//! the call's `next` key. Debug and test builds check the holder against core's
//! `continuationChannels` where the call is named.
use crate::id::ToolId;
use crate::response::result::Continuation;
use serde_json::{Map, Value};

/// The continuation call that reads cursor `field` (a page number or a
/// token) at `next`: `query` (the complete query row the continuation
/// repeats) with `field` set and each of `set` written after it
/// (`snapshot`, an outer cursor's reset), built as an exact replay of
/// `tool`. `None` only when `query` is not an object.
pub fn continued_page(
    tool: ToolId,
    query: Value,
    field: &str,
    next: impl Into<Value>,
    set: &[(&str, Value)],
) -> Option<Continuation> {
    let mut changes = Map::new();
    changes.insert(field.into(), next.into());
    for (field, value) in set {
        changes.insert((*field).into(), value.clone());
    }
    query.is_object().then(|| {
        let row = crate::response::result::with_changes(query, &Value::Object(changes));
        Continuation::continued(tool, row)
    })
}

/// The row's own page walk of a typed query (a localFetch / ghGetFileContent
/// `next.nextPage`): the call that continues `query`, checked against core's
/// `continuationChannels` like [`insert_walk`].
pub fn walk_query(query: impl Into<octocode_contract_types::ToolQuery>) -> Continuation {
    let query = query.into();
    let Some(tool) = ToolId::from_name(query.tool_name()) else {
        unreachable!("a generated ToolQuery variant names a runtime tool");
    };
    assert_row_walk(tool);
    Continuation::continued_query(query)
}

/// Debug and test builds: `tool`'s rows declare `nextPage` a linear channel.
fn assert_row_walk(tool: ToolId) {
    debug_assert!(row_walks(tool), "{} pages by no linear walk", tool.as_str());
}

/// A `next` bundle holding the one walk of `tool`'s `collection` (`None`: a
/// row's own page), as the row's `next` or a producer's private `calls`.
pub fn walk_calls(tool: ToolId, collection: Option<&str>, call: Continuation) -> Value {
    let mut calls = Value::Object(Map::new());
    insert_walk(&mut calls, tool, collection, call);
    calls
}

/// The row's own page walk as a saved cursor handle: the call that reads on
/// from `cursor` alone, in the row's `next`.
pub fn insert_cursor_walk(row: &mut Value, tool: ToolId, cursor: impl Into<Value>) {
    assert_row_walk(tool);
    row["next"][NEXT_PAGE] = Continuation::cursor(tool.as_str(), cursor);
}

/// The row's own page walk, in the private `calls` bundle of `row`.
pub fn insert_row_walk(row: &mut Value, tool: ToolId, call: Continuation) {
    insert_walk(&mut row["calls"], tool, None, call);
}

/// Moves a walk the builder already issued, `call` (kept in a section's
/// private page block until its row publishes), to `row`'s `next` under the
/// section's own walk name.
pub fn relocate_walk(row: &mut Value, tool: ToolId, section: &str, call: Value) {
    debug_assert!(
        call["tool"] == tool.as_str() && walk_name(tool, section) == section,
        "{section} is not a declared walk of {}",
        tool.as_str()
    );
    row["next"][section] = call;
}

/// Removes the row's own page walk from its private `calls` bundle.
pub fn remove_row_walk(row: &mut Value) {
    if let Some(calls) = row.get_mut("calls").and_then(Value::as_object_mut) {
        calls.shift_remove(NEXT_PAGE);
    }
}

/// Writes the walk's `call` into `calls`, under the key the walk's channel
/// names ([`walk_name`]).
pub fn insert_walk(calls: &mut Value, tool: ToolId, collection: Option<&str>, call: Continuation) {
    debug_assert!(
        match collection {
            Some(collection) =>
                declares(tool, collection) || walk_name(tool, collection) != NEXT_PAGE,
            None => row_walks(tool),
        },
        "{collection:?} is not a declared walk of {}",
        tool.as_str()
    );
    if !calls.is_object() {
        *calls = Value::Object(Map::new());
    }
    let walk = collection.map_or(NEXT_PAGE, |name| walk_name(tool, name));
    calls[walk] = call.build();
}

/// Whether `tool`'s rows declare `nextPage` a linear channel.
fn row_walks(tool: ToolId) -> bool {
    crate::contracts::parsed_contract()
        .ok()
        .and_then(|contract| {
            contract.pointer(&format!(
                "/continuationChannels/{}/row/{NEXT_PAGE}",
                tool.as_str()
            ))
        })
        .is_some_and(|channel| channel == "linear")
}

use crate::response::continuations::LINEAR_WALK as NEXT_PAGE;

/// The `next` key that walks `tool`'s `collection`: the collection's own
/// name when the tool's rows declare it a linear channel, else `nextPage`.
pub(crate) fn walk_name(tool: ToolId, collection: &str) -> &str {
    let own = crate::contracts::parsed_contract()
        .ok()
        .and_then(|contract| {
            contract.pointer(&format!(
                "/continuationChannels/{}/row/{collection}",
                tool.as_str()
            ))
        })
        .is_some_and(|channel| channel == "linear");
    if own { collection } else { NEXT_PAGE }
}

/// Whether core declares `collection` a paged collection of `tool`.
pub(crate) fn declares(tool: ToolId, collection: &str) -> bool {
    crate::contracts::parsed_contract()
        .ok()
        .and_then(|contract| {
            contract.pointer(&format!(
                "/continuationChannels/{}/collections",
                tool.as_str()
            ))
        })
        .and_then(Value::as_array)
        .is_some_and(|names| names.iter().any(|name| name == collection))
}

#[cfg(test)]
mod tests;
