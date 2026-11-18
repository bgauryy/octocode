//! The billing rule every lead keeps: a judgment is billed only to the call
//! that asked for it.
use serde_json::Value;

/// Every call `value` offers, wherever it sits (`next.*`, `related[]`, an
/// error's calls, a raw call map): each object naming a call `kind`, its
/// `tool` and its `input`.
pub fn offered_calls(value: &Value) -> Vec<&Value> {
    fn walk<'a>(value: &'a Value, calls: &mut Vec<&'a Value>) {
        match value {
            Value::Object(fields) => {
                if fields.contains_key("kind")
                    && fields.contains_key("tool")
                    && fields.contains_key("input")
                {
                    calls.push(value);
                }
                fields.values().for_each(|field| walk(field, calls));
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, calls)),
            _ => {}
        }
    }
    let mut calls = Vec::new();
    walk(value, &mut calls);
    calls
}

/// No call a `tool` result offers bills a judgment the caller did not ask
/// for: a fresh (`related`) call never carries a classification field or a
/// plan, and only `tool`'s own saved work (`continue`) may; a clasify call
/// is only clasify's own.
pub fn assert_bills_only_itself(tool: crate::id::ToolId, value: &Value) {
    for call in offered_calls(value) {
        let rows = call
            .pointer("/input/queries")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        let billed = call["tool"] == crate::id::ToolId::Clasify.as_str()
            || rows.iter().any(|row| {
                crate::contracts::PROVIDER_FIELDS
                    .iter()
                    .any(|field| row.get(*field).is_some())
            });
        if !billed {
            continue;
        }
        let own = call["tool"] == tool.as_str();
        let continued = call["kind"] == "continue";
        let clasify_own = tool == crate::id::ToolId::Clasify && own;
        assert!(
            clasify_own || (own && continued),
            "a {} result offers a call that bills again: {call}",
            tool.as_str()
        );
    }
}
