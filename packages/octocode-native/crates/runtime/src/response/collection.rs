//! The one builder of a paged collection's continuation. A collection is a
//! list inside a row with its own `pagination` object (core
//! `continuationChannels.<tool>.collections`); its linear walk is that
//! object's `next.nextPage`, never a row-level key.
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

/// Writes the page state of `tool`'s `collection` into its `pagination`
/// object: `hasMore`, and `next.nextPage` while a page remains; the last
/// page carries no `next`.
pub(crate) fn set_next_page(
    pagination: &mut impl RowFields,
    tool: ToolId,
    collection: &str,
    next: Option<Continuation>,
) {
    debug_assert!(
        declares(tool, collection),
        "{collection} is not a declared collection of {}",
        tool.as_str()
    );
    let Some(fields) = pagination.fields() else {
        return;
    };
    fields.insert("hasMore".into(), json!(next.is_some()));
    match next {
        Some(call) => {
            let mut next = Map::new();
            next.insert(NEXT_PAGE.into(), call.build());
            fields.insert("next".into(), Value::Object(next));
        }
        None => {
            fields.shift_remove("next");
        }
    }
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
