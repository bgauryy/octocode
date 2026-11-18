//! Core-owned private progress fields. Only issued cursors may supply them.

use crate::id::ToolId;
use std::{collections::BTreeSet, sync::OnceLock};

/// The fields of a query row that brief the caller's task: no form or result
/// depends on them, so fresh work and a repair example leave them out, as
/// they leave out the [`state_fields`].
pub const BRIEF_FIELDS: [&str; 2] = ["mainGoal", "reasoning"];

/// `tool`'s state fields: top-level query fields marked `x-cursor`.
pub fn state_fields(tool: ToolId) -> &'static BTreeSet<String> {
    static FIELDS: [OnceLock<BTreeSet<String>>; ToolId::ALL.len()] =
        [const { OnceLock::new() }; ToolId::ALL.len()];
    FIELDS[tool.index()].get_or_init(|| {
        let Ok(contract) = super::tool_contract(tool) else {
            return BTreeSet::new();
        };
        let schema = &contract["querySchema"];
        let defs = &schema["$defs"];
        super::schema_facts::property_names(schema, defs)
            .into_iter()
            .filter(|name| {
                super::schema_facts::property_schemas(schema, defs, name)
                    .iter()
                    .any(|field| {
                        super::schema_facts::schema_keyword(field, defs, "x-cursor")
                            == Some(&serde_json::Value::Bool(true))
                    })
            })
            .map(str::to_owned)
            .collect()
    })
}

#[cfg(test)]
mod tests;
