//! Shared assertions for typed continuations and complete evidence page walks.
use super::{publish_for_assertion, validate_output, validate_test_input};
use serde_json::Value;

pub fn assert_output_contract(tool: &str, output: &Value) {
    let published = publish_for_assertion(tool, output);
    validate_output(tool, &published)
        .unwrap_or_else(|error| panic!("{tool} output contract: {error:?}\n{published}"));
    assert_leads_valid(output);
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Lead {
    pub channel: String,
    pub name: String,
    pub tool: String,
    pub query: Value,
    pub kind: String,
}

pub fn leads(output: &Value) -> Vec<Lead> {
    fn visit(value: &Value, channel: &str, name: &str, out: &mut Vec<Lead>) {
        if let (Some(kind), Some(tool), Some(input)) = (
            value["kind"].as_str(),
            value["tool"].as_str(),
            value.get("input"),
        ) {
            assert!(matches!(kind, "continue" | "related"));
            out.push(Lead {
                channel: channel.into(),
                name: name.into(),
                tool: tool.into(),
                query: input.clone(),
                kind: kind.into(),
            });
            return;
        }
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    if key == "input" {
                        continue;
                    }
                    let next_channel = if matches!(key.as_str(), "next" | "related") {
                        key.as_str()
                    } else {
                        channel
                    };
                    visit(child, next_channel, key, out);
                }
            }
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    visit(item, channel, &index.to_string(), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    visit(output, "", "", &mut out);
    out
}

pub fn assert_leads_valid(output: &Value) {
    for lead in leads(output) {
        if lead.channel == "next" {
            assert_eq!(lead.kind, "continue", "{lead:?}");
        }
        if lead.channel == "related" {
            assert_eq!(lead.kind, "related", "{lead:?}");
        }
        validate_test_input(&lead.tool, &lead.query)
            .unwrap_or_else(|error| panic!("Invalid typed request: {lead:?}: {error:?}"));
    }
}

/// `needle` appears nowhere in `value`: in no string leaf and no object key.
pub fn assert_absent(value: &Value, needle: &str) {
    fn find<'a>(value: &'a Value, needle: &str) -> Option<&'a str> {
        match value {
            Value::String(text) => text.contains(needle).then_some(text.as_str()),
            Value::Array(items) => items.iter().find_map(|item| find(item, needle)),
            Value::Object(map) => map.iter().find_map(|(key, child)| {
                if key.contains(needle) {
                    Some(key.as_str())
                } else {
                    find(child, needle)
                }
            }),
            _ => None,
        }
    }
    if let Some(hit) = find(value, needle) {
        panic!("`{needle}` present in `{hit}`:\n{value}");
    }
}

/// Every string under a `path` key of `value`, at any depth (leads included),
/// in document order.
pub(crate) fn paths_in(value: &Value) -> Vec<String> {
    fn visit(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Array(items) => items.iter().for_each(|item| visit(item, out)),
            Value::Object(map) => {
                for (key, child) in map {
                    if key == "path"
                        && let Some(path) = child.as_str()
                    {
                        out.push(path.to_owned());
                    }
                    visit(child, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    visit(value, &mut out);
    out
}

/// `value` with every object's keys sorted: two calls are the same call when
/// their canonical texts are equal.
pub fn canonical(value: &Value) -> String {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
            Value::Object(map) => {
                let mut keys = map.keys().collect::<Vec<_>>();
                keys.sort();
                Value::Object(
                    keys.into_iter()
                        .map(|key| (key.clone(), sorted(&map[key])))
                        .collect(),
                )
            }
            other => other.clone(),
        }
    }
    sorted(value).to_string()
}

#[path = "walker.rs"]
pub mod walker;
pub use walker::{Page, Walk, Walker};

/// Select an explicitly related request by target and its complete input.
/// The predicate sees the envelope, so batched requests retain every row.
pub(crate) fn related_call<'a>(
    data: &'a Value,
    tool: &str,
    accepts: impl Fn(&Value) -> bool,
) -> &'a Value {
    find_related_call(data, tool, accepts)
        .unwrap_or_else(|| panic!("No matching related {tool} request in {data}"))
}

pub(crate) fn find_related_call<'a>(
    data: &'a Value,
    tool: &str,
    accepts: impl Fn(&Value) -> bool,
) -> Option<&'a Value> {
    data["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|call| call["kind"] == "related" && call["tool"] == tool && accepts(&call["input"]))
}
