//! Core-owned private progress fields. Only issued cursors may supply them.

use crate::id::ToolId;
use std::{collections::BTreeSet, sync::OnceLock};

/// `tool`'s state fields: top-level query fields marked `x-cursor`.
pub(crate) fn state_fields(tool: ToolId) -> &'static BTreeSet<String> {
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
                        field["x-cursor"] == true
                            || super::schema_facts::resolve_ref(field, defs)["x-cursor"] == true
                    })
            })
            .map(str::to_owned)
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn referenced_progress_is_private_and_only_accepted_from_an_issued_cursor() {
        let row = json!({"path":"src/lib.rs", "readProgress":{"unit":"lines","offset":3}});
        assert!(state_fields(ToolId::LocalFetch).contains("readProgress"));
        assert!(crate::contracts::validate_query("localFetch", row.clone()).is_err());
        crate::contracts::validate_built_query("localFetch", row)
            .expect("the same progress is valid inside a saved request");
    }
}
