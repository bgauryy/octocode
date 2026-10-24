//! Follow-up leads shared by several tools, built once: leads one tool
//! offers into another (so no tool imports the target tool's internals), an
//! empty search's broadening, and the prose hint beside them.

pub(crate) mod broaden;
pub(crate) mod find_file;
pub(crate) mod lsp;
pub(crate) mod skipped;

use serde_json::{Value, json};

pub(crate) fn push_hint(row: &mut Value, hint: impl Into<String>) {
    let hint = Value::String(hint.into());
    match &mut row["hints"] {
        Value::Array(hints) => hints.push(hint),
        hints => *hints = json!([hint]),
    }
}
