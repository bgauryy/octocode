//! astSearch `symbols`: outline rows, name filters, recovered parses,
//! paging and the leads an outline offers.
use super::test_rows::*;
use crate::execution::cancel::NeverCancel;
use crate::tools::test_support::{Fixture, checks, simple_policy, skipped_names};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

mod names;
mod recovered;
mod rows;
mod scope;

/// The facts of declaration rows, nested as the wire nests them: per row
/// `line`, `endLine`, `kind`, `symbolName`, `exported` and `column` (each only
/// when the row states it, or its parent's `shared` does) and its `members`.
fn outline_facts(rows: &Value, shared: &Value) -> Value {
    Value::Array(
        rows.as_array()
            .expect("rows")
            .iter()
            .map(|row| {
                let mut facts = serde_json::Map::new();
                for (key, wire) in [
                    ("line", "line"),
                    ("endLine", "endLine"),
                    ("kind", "kind"),
                    ("name", "symbolName"),
                    ("exported", "exported"),
                    ("column", "column"),
                ] {
                    if let Some(value) = row.get(wire).or_else(|| shared.get(wire)) {
                        facts.insert(key.into(), value.clone());
                    }
                }
                if let Some(members) = row.get("members") {
                    let shared = row.get("shared").unwrap_or(&Value::Null);
                    facts.insert("members".into(), outline_facts(members, shared));
                }
                Value::Object(facts)
            })
            .collect(),
    )
}
