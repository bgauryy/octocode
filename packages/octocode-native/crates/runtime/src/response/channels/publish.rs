use crate::id::ToolId;
use crate::id::continuation_channels::{HINT_TEXT_KEY, HINTS_KEY, PAGES_KEY};
use serde_json::Value;

/// Publish optional guidance once, after its calls have been validated and
/// paths compacted. Required continuations remain structured in `next`.
pub(crate) fn publish_hints(value: &mut Value, tool: ToolId) {
    let Ok(contract) = crate::contracts::tool_contract(tool) else {
        return;
    };
    let schema = &contract["outputSchema"];
    publish_at(value, &[schema], &schema["$defs"]);
}

fn publish_at(value: &mut Value, schemas: &[&Value], defs: &Value) {
    match value {
        Value::Array(items) => {
            let items_schemas: Vec<_> = schemas
                .iter()
                .flat_map(|schema| crate::contracts::item_schemas(schema, defs))
                .collect();
            if !items_schemas.is_empty() {
                for item in items {
                    publish_at(item, &items_schemas, defs);
                }
            }
        }
        Value::Object(object) => {
            let mut empty_hints = false;
            for (key, child) in object.iter_mut() {
                let declared: Vec<_> = schemas
                    .iter()
                    .flat_map(|schema| crate::contracts::property_schemas(schema, defs, key))
                    .collect();
                if declared.is_empty() {
                    continue;
                }
                if key == HINTS_KEY {
                    let guidance = match child {
                        Value::Array(items) => items.iter().all(Value::is_string),
                        Value::Object(fields) => fields.iter().all(|(key, value)| {
                            (key == HINT_TEXT_KEY
                                && value
                                    .as_array()
                                    .is_some_and(|items| items.iter().all(Value::is_string)))
                                || crate::response::result::is_call(value)
                        }),
                        _ => false,
                    };
                    if guidance {
                        let mut strings = Vec::new();
                        collect_hints(
                            child.take(),
                            None,
                            &mut strings,
                            &mut std::collections::HashSet::new(),
                        );
                        empty_hints = strings.is_empty();
                        *child = Value::Array(strings);
                    }
                } else if key != PAGES_KEY {
                    publish_at(child, &declared, defs);
                }
            }
            if empty_hints {
                object.remove(HINTS_KEY);
            }
        }
        _ => {}
    }
}

fn collect_hints(
    value: Value,
    label: Option<&str>,
    strings: &mut Vec<Value>,
    seen: &mut std::collections::HashSet<String>,
) {
    let text = match value {
        Value::String(text) => text,
        Value::Array(items) => {
            items
                .into_iter()
                .for_each(|item| collect_hints(item, None, strings, seen));
            return;
        }
        call @ Value::Object(_) if crate::response::result::is_call(&call) => {
            let mut identity = call.clone();
            identity["query"].sort_all_objects();
            let (Some(line), Some(identity)) = (
                crate::response::result::call_line(&call),
                crate::response::result::call_line(&identity),
            ) else {
                return;
            };
            if !seen.insert(identity) {
                return;
            }
            match label {
                Some(label) => format!("{label}: {line}"),
                None => line,
            }
        }
        Value::Object(mut fields) => {
            if let Some(text) = fields.shift_remove(HINT_TEXT_KEY) {
                collect_hints(text, None, strings, seen);
            }
            fields
                .into_iter()
                .for_each(|(name, hint)| collect_hints(hint, Some(&name), strings, seen));
            return;
        }
        _ => return,
    };
    let text = Value::String(text.trim().to_owned());
    if text.as_str().is_some_and(|text| !text.is_empty()) && !strings.contains(&text) {
        strings.push(text);
    }
}
