//! Required evidence fields retained while splitting output collections into pages.
use crate::id::ToolId;
use serde_json::Value;
use std::collections::BTreeSet;

pub(crate) fn required_entry_fields(tool: ToolId) -> &'static BTreeSet<String> {
    static REQUIRED: PerTool = [const { std::sync::OnceLock::new() }; ToolId::ALL.len()];
    per_tool(&REQUIRED, tool, |schema| {
        let mut required = BTreeSet::new();
        let mut walk = EntryWalk {
            defs: &schema["$defs"],
            seen: std::collections::HashSet::new(),
            required: &mut required,
        };
        walk.visit(schema, false);
        required
    })
}

/// One field table per tool, indexed by [`ToolId::index`].
type PerTool = [std::sync::OnceLock<BTreeSet<String>>; ToolId::ALL.len()];

/// `tool`'s table in `tables`, built on first use by `build` from that
/// tool's output schema only (see [`super::tool_contract`]): a response
/// never parses the contracts of tools it does not answer for. A tool the
/// contract lacks has no fields.
fn per_tool(
    tables: &'static PerTool,
    tool: ToolId,
    build: impl FnOnce(&Value) -> BTreeSet<String>,
) -> &'static BTreeSet<String> {
    tables[tool.index()].get_or_init(|| {
        super::tool_contract(tool)
            .map(|contract| build(&contract["outputSchema"]))
            .unwrap_or_default()
    })
}

/// One output schema's walk: inside an array's `items` (through references
/// and union branches) it collects each object's `required` names. Each
/// schema node is visited once per context, so recursive definitions end.
struct EntryWalk<'a, 'b> {
    defs: &'a Value,
    seen: std::collections::HashSet<(*const Value, bool)>,
    required: &'b mut BTreeSet<String>,
}

impl<'a> EntryWalk<'a, '_> {
    fn visit(&mut self, schema: &'a Value, entry: bool) {
        let schema = super::resolve_ref(schema, self.defs);
        if !self.seen.insert((std::ptr::from_ref(schema), entry)) {
            return;
        }
        let Some(object) = schema.as_object() else {
            return;
        };
        if entry {
            self.required.extend(
                object
                    .get("required")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned),
            );
        }
        for (key, value) in object {
            match key.as_str() {
                "items" => self.visit(value, true),
                "oneOf" | "anyOf" | "allOf" => {
                    for branch in value.as_array().into_iter().flatten() {
                        self.visit(branch, entry);
                    }
                }
                "properties" | "patternProperties" => {
                    for property in value.as_object().into_iter().flat_map(|map| map.values()) {
                        self.visit(property, false);
                    }
                }
                "additionalProperties" => self.visit(value, false),
                _ => {}
            }
        }
    }
}
