//! The one builder of a paged collection's continuation. A collection is a
//! list inside a row (core `continuationChannels.<tool>.collections`). Its
//! `pagination` object states where the page sits and `hasMore`, and never
//! holds a call: the walk is a call in the `next` of the row or element that
//! holds the list, `next.<collection>` when the tool's rows declare that name
//! as a linear channel (a section's own walk), else `next.nextPage`.
//!
//! Compiled for tests until its first emitter migrates (lanes B, C, D, E):
//! the crate denies dead code, and that migration removes the `cfg(test)`
//! on this module in `response.rs`.
use crate::id::ToolId;
use crate::response::paging::RowFields;
use crate::response::result::Continuation;
use serde_json::{Map, Value, json};

/// The name of every linear walk (core `continuationChannels`).
pub(crate) const NEXT_PAGE: &str = "nextPage";

/// Writes the page state of `tool`'s `collection` into `holder`, the row or
/// element that holds the list: `pagination.hasMore`, and the walk's call in
/// `next` while a page remains; the last page carries no walk.
pub(crate) fn set_next_page(
    holder: &mut impl RowFields,
    tool: ToolId,
    collection: &str,
    next: Option<Continuation>,
) {
    debug_assert!(
        declares(tool, collection),
        "{collection} is not a declared collection of {}",
        tool.as_str()
    );
    let Some(fields) = holder.fields() else {
        return;
    };
    let walk = walk_name(tool, collection);
    if let Some(pagination) = fields
        .entry("pagination")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
    {
        pagination.insert("hasMore".into(), json!(next.is_some()));
    }
    match next {
        Some(call) => {
            if let Some(calls) = fields
                .entry("next")
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
            {
                calls.insert(walk.into(), call.build());
            }
        }
        None => {
            let emptied = fields
                .get_mut("next")
                .and_then(Value::as_object_mut)
                .is_some_and(|calls| {
                    calls.shift_remove(walk);
                    calls.is_empty()
                });
            if emptied {
                fields.shift_remove("next");
            }
        }
    }
}

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
