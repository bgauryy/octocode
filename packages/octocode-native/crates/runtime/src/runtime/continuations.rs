//! Continuation compaction: drop query fields that validation would restore.
//!
//! Every emitted `{tool, query}` continuation is replayed through contract
//! validation, which re-applies schema defaults. A field is therefore removed
//! only when validating the query without it yields the identical validated
//! query, so the compact continuation executes exactly like the full one.

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::contracts::validate_query;

/// Compacts every `{tool, query}` continuation nested in `value`.
pub fn compact_continuations(value: &mut Value) {
    let mut memo = HashMap::new();
    walk(value, &mut memo);
}

/// Compacts one tool input: a flat query or a `{queries: [...]}` envelope.
pub fn compact_input(tool: &str, input: &mut Value) {
    let mut memo = HashMap::new();
    compact_query_or_envelope(tool, input, &mut memo);
}

type Memo = HashMap<(String, String), Value>;

/// Continuations live only under `next` keys (`data.next.*`,
/// `responsePagination.next`, `semanticRerank.next`); a `{tool, query}` shape
/// anywhere else is tool data and stays untouched.
fn walk(value: &mut Value, memo: &mut Memo) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(|item| walk(item, memo)),
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if key == "next" {
                    compact_next(child, memo);
                }
                walk(child, memo);
            }
        }
        _ => {}
    }
}

/// A `next` value is one continuation or a map of named continuations.
fn compact_next(next: &mut Value, memo: &mut Memo) {
    if compact_continuation(next, memo) {
        return;
    }
    if let Some(map) = next.as_object_mut() {
        map.values_mut().for_each(|continuation| {
            compact_continuation(continuation, memo);
        });
    }
}

fn compact_continuation(value: &mut Value, memo: &mut Memo) -> bool {
    let Some(map) = value.as_object_mut() else {
        return false;
    };
    let Some(tool) = map.get("tool").and_then(Value::as_str).map(str::to_owned) else {
        return false;
    };
    match map.get_mut("query") {
        Some(query) if query.is_object() => {
            compact_query_or_envelope(&tool, query, memo);
            true
        }
        _ => false,
    }
}

fn compact_query_or_envelope(tool: &str, query: &mut Value, memo: &mut Memo) {
    match query.get_mut("queries").and_then(Value::as_array_mut) {
        Some(queries) => queries
            .iter_mut()
            .for_each(|row| compact_query(tool, row, memo)),
        None => compact_query(tool, query, memo),
    }
}

fn compact_query(tool: &str, query: &mut Value, memo: &mut Memo) {
    let Some(object) = query.as_object() else {
        return;
    };
    let key = (tool.to_owned(), query.to_string());
    if let Some(compact) = memo.get(&key) {
        *query = compact.clone();
        return;
    }
    let Ok(full) = validate_query(tool, query.clone()) else {
        return;
    };
    let mut compact: Map<String, Value> = object.clone();
    for field in object.keys() {
        let Some(removed) = compact.remove(field) else {
            continue;
        };
        let restored = validate_query(tool, Value::Object(compact.clone()))
            .is_ok_and(|validated| validated == full);
        if !restored {
            compact.insert(field.clone(), removed);
        }
    }
    // Keep the caller's field order for the fields that remain.
    let ordered: Map<String, Value> = object
        .iter()
        .filter_map(|(field, _)| compact.get(field).map(|v| (field.clone(), v.clone())))
        .collect();
    *query = Value::Object(ordered);
    memo.insert(key, query.clone());
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn drops_only_fields_validation_restores() {
        let mut out = json!({"results":[{"data":{"next":{"nextPage":{"tool":"localSearch","confidence":"exact","query":{
            "searchText":"foo","path":"/tmp","reasoning":"r","debug":false,"caseMode":"smart",
            "matchContentLength":200,"page":2,"contextLines":0,"regex":"rust"
        }}}}}]});
        compact_continuations(&mut out);
        let query = &out["results"][0]["data"]["next"]["nextPage"]["query"];
        assert_eq!(
            query,
            &json!({"searchText":"foo","path":"/tmp","reasoning":"r","page":2,"contextLines":0})
        );
        let full = validate_query("localSearch", query.clone()).expect("valid");
        assert_eq!(full["matchContentLength"], 200);
    }

    #[test]
    fn leaves_tool_query_shaped_data_outside_next_untouched() {
        let data = json!({"tool":"localSearch","query":{"searchText":"a","path":"/tmp","reasoning":"r","debug":false}});
        let mut out =
            json!({"results":[{"data":{"content":data.clone(),"next":{"nextPage":data.clone()}}}]});
        compact_continuations(&mut out);
        assert_eq!(out["results"][0]["data"]["content"], data);
        assert!(
            out["results"][0]["data"]["next"]["nextPage"]["query"]
                .get("debug")
                .is_none()
        );
    }

    #[test]
    fn compacts_each_row_of_a_batch_envelope_and_leaves_invalid_queries() {
        let mut input = json!({"queries":[{"searchText":"a","path":"/tmp","reasoning":"r","debug":false}],
            "responseCharOffset":10});
        compact_input("localSearch", &mut input);
        assert_eq!(
            input["queries"][0],
            json!({"searchText":"a","path":"/tmp","reasoning":"r"})
        );
        assert_eq!(input["responseCharOffset"], 10);
        let mut invalid = json!({"tool":"localSearch","query":{"debug":false}});
        compact_continuations(&mut invalid);
        assert_eq!(invalid["query"], json!({"debug":false}));
    }
}
