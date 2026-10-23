use super::{lead_continuations, shape};
use crate::id::ToolId;
use crate::id::{Channel, channel, continuation_channels, is_remaining};
use serde_json::{Value, json};
mod leads;
mod ordering;
mod publishing;
mod withheld;

/// [`shape`] on every object of every row, as the continuation stage
/// applies it.
fn split_hints(structured: &mut Value, tool: ToolId) {
    let key = if tool == ToolId::Clasify {
        "queries"
    } else {
        "results"
    };
    for row in structured[key].as_array_mut().into_iter().flatten() {
        if let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) {
            super::move_text(data);
        }
        if let Some(object) = row.as_object_mut() {
            super::move_text(object);
        }
    }
    walk(&mut structured[key], tool);
}

fn walk(value: &mut Value, tool: ToolId) {
    if crate::response::result::is_call(value) {
        return;
    }
    match value {
        Value::Array(items) => items.iter_mut().for_each(|item| walk(item, tool)),
        Value::Object(object) => {
            shape(object, tool, false, false);
            for (key, child) in object.iter_mut() {
                if key != "next" && key != "hints" {
                    walk(child, tool);
                }
            }
        }
        _ => {}
    }
}
