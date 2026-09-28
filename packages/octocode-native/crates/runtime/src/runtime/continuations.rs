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

/// Fills a missing `goal` on every `next` continuation in each result row
/// with the goal of the input row that produced it: a continuation serves
/// the same goal, and `goal` is required, so an emitter that sets only
/// `reasoning` would otherwise withhold its whole row as a contract violation.
pub fn fill_continuation_goals(structured: &mut Value, input: &Value) {
    let goals: Vec<Option<&str>> = match input.get("queries").and_then(Value::as_array) {
        Some(rows) => rows
            .iter()
            .map(|row| row.get("goal").and_then(Value::as_str))
            .collect(),
        None => vec![input.get("goal").and_then(Value::as_str)],
    };
    let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    for (position, row) in rows.iter_mut().enumerate() {
        let index = row
            .get("index")
            .and_then(Value::as_u64)
            .and_then(|index| usize::try_from(index).ok())
            .unwrap_or(position);
        let Some(goal) = goals
            .get(index)
            .or_else(|| goals.first())
            .copied()
            .flatten()
        else {
            continue;
        };
        goal_walk(row, goal);
    }
}

fn goal_walk(value: &mut Value, goal: &str) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(|item| goal_walk(item, goal)),
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if key == "next" {
                    goal_next(child, goal);
                } else {
                    goal_walk(child, goal);
                }
            }
        }
        _ => {}
    }
}

fn goal_next(next: &mut Value, goal: &str) {
    if continuation_target(next).is_some() {
        fill_goal(next, goal);
        return;
    }
    if let Some(map) = next.as_object_mut() {
        for action in map.values_mut() {
            if continuation_target(action).is_some() {
                fill_goal(action, goal);
            } else {
                goal_walk(action, goal);
            }
        }
    }
}

fn fill_goal(continuation: &mut Value, goal: &str) {
    let Some(query) = continuation.get_mut("query") else {
        return;
    };
    let rows: Vec<&mut Value> = match query.get_mut("queries").and_then(Value::as_array_mut) {
        Some(rows) => rows.iter_mut().collect(),
        None => vec![query],
    };
    for row in rows {
        if let Some(object) = row.as_object_mut()
            && object
                .get("goal")
                .and_then(Value::as_str)
                .is_none_or(|value| value.trim().is_empty())
        {
            object.insert("goal".to_owned(), Value::String(goal.to_owned()));
        }
    }
}

/// Drop optional cross-tool next actions the current surface cannot execute.
pub fn filter_unavailable_cross_tool_next(
    value: &mut Value,
    current_tool: &str,
    is_available: impl Fn(&str) -> bool,
) {
    filter_walk(value, current_tool, &is_available);
}

fn continuation_target(value: &Value) -> Option<&str> {
    value
        .as_object()
        .filter(|map| map.get("query").is_some_and(Value::is_object))?
        .get("tool")?
        .as_str()
}

fn filter_walk(value: &mut Value, current_tool: &str, is_available: &impl Fn(&str) -> bool) {
    // A continuation query is caller content, not another output tree.
    if continuation_target(value).is_some() {
        return;
    }
    match value {
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| filter_walk(item, current_tool, is_available)),
        Value::Object(map) => map.retain(|key, child| {
            if key == "next" {
                filter_next(child, current_tool, is_available)
            } else {
                filter_walk(child, current_tool, is_available);
                true
            }
        }),
        _ => {}
    }
}

fn filter_next(next: &mut Value, current_tool: &str, is_available: &impl Fn(&str) -> bool) -> bool {
    if let Some(target) = continuation_target(next) {
        return target == current_tool || is_available(target);
    }
    if let Some(map) = next.as_object_mut() {
        map.retain(|_, action| {
            if let Some(target) = continuation_target(action) {
                target == current_tool || is_available(target)
            } else {
                filter_walk(action, current_tool, is_available);
                true
            }
        });
        return !map.is_empty();
    }
    true
}

/// Continuations live only under `next` keys (`data.next.*`,
/// `responsePagination.next`, nested `next`); a `{tool, query}` shape
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
    // Required goal and reasoning stay: a replay that drops them no longer validates.
    let mut compact: Map<String, Value> = object.clone();
    let Ok(full) = validate_query(tool, Value::Object(compact.clone())) else {
        return;
    };
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
    fn fills_missing_continuation_goal_from_the_producing_row() {
        let input = json!({"queries":[{"goal":"Find a."},{"goal":"Find b."}]});
        let mut out = json!({"results":[
            {"index":1,"data":{"next":{
                "viewRepo":{"tool":"ghStructure","query":{"owner":"o","repo":"r","reasoning":"r"}},
                "kept":{"tool":"localFetch","query":{"goal":"Own goal.","reasoning":"r"}}
            }}},
            {"index":0,"data":{"items":[{"next":{"tool":"localFetch","query":{"queries":[{"reasoning":"r"}]}}}]}}
        ]});
        fill_continuation_goals(&mut out, &input);
        assert_eq!(
            out["results"][0]["data"]["next"]["viewRepo"]["query"]["goal"],
            "Find b."
        );
        assert_eq!(
            out["results"][0]["data"]["next"]["kept"]["query"]["goal"],
            "Own goal."
        );
        assert_eq!(
            out["results"][1]["data"]["items"][0]["next"]["query"]["queries"][0]["goal"],
            "Find a."
        );
    }

    #[test]
    fn drops_only_fields_validation_restores() {
        let mut out = json!({"results":[{"data":{"next":{"nextPage":{"tool":"localSearch","confidence":"exact","query":{
            "searchText":"foo","path":"/tmp","goal":"Find foo.","reasoning":"r","debug":false,"caseMode":"smart",
            "matchContentLength":200,"page":2,"contextLines":0,"regex":"rust"
        }}}}}]});
        compact_continuations(&mut out);
        let query = &out["results"][0]["data"]["next"]["nextPage"]["query"];
        // matchContentLength has no schema default (it scales with
        // contextLines), so validation cannot restore it and it is kept.
        assert_eq!(
            query,
            &json!({"searchText":"foo","path":"/tmp","goal":"Find foo.","reasoning":"r","matchContentLength":200,"page":2,"contextLines":0})
        );
        let full = validate_query("localSearch", query.clone()).expect("valid");
        assert_eq!(full["matchContentLength"], 200);
    }

    #[test]
    fn leaves_tool_query_shaped_data_outside_next_untouched() {
        let data = json!({"tool":"localSearch","query":{"searchText":"a","path":"/tmp","goal":"Find a.","reasoning":"r","debug":false}});
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
        let mut input = json!({"queries":[{"searchText":"a","path":"/tmp","goal":"Find a.","reasoning":"r","debug":false}],
            "responseCharOffset":10});
        compact_input("localSearch", &mut input);
        assert_eq!(
            input["queries"][0],
            json!({"searchText":"a","path":"/tmp","goal":"Find a.","reasoning":"r"})
        );
        assert_eq!(input["responseCharOffset"], 10);
        let mut invalid = json!({"tool":"localSearch","query":{"debug":false}});
        compact_continuations(&mut invalid);
        assert_eq!(invalid["query"], json!({"debug":false}));
    }

    #[test]
    fn filters_unavailable_cross_tool_actions_without_touching_pagination_or_query_data() {
        let query = json!({"goal": "test", "reasoning":"continue", "next":{"readFile":{"tool":"localFetch","query":{"path":"/tmp/a"}}}});
        let mut out = json!({"results":[{"data":{
            "next":{
                "nextPage":{"tool":"ghSearchCode","query":query.clone()},
                "readTopMatch":{"tool":"ghGetFileContent","query":{"path":"a.rs"}},
                "readSite":{"tool":"localFetch","query":{"path":"/tmp/a"}}
            },
            "content":{"tool":"ghGetFileContent","query":{"next":{"readFile":{"tool":"localFetch","query":{}}}}},
            "nestedEvidence":{"next":{"nextPage":{"tool":"ghSearchCode","query":{"page":2}}}}
        }}]});
        let original_content = out["results"][0]["data"]["content"].clone();
        filter_unavailable_cross_tool_next(&mut out, "ghSearchCode", |target| {
            target == "localFetch"
        });
        let next = &out["results"][0]["data"]["next"];
        assert_eq!(next["nextPage"]["query"], query);
        assert!(next.get("readTopMatch").is_none());
        assert_eq!(next["readSite"]["tool"], "localFetch");
        assert_eq!(out["results"][0]["data"]["content"], original_content);
        assert_eq!(
            out["results"][0]["data"]["nestedEvidence"]["next"]["nextPage"]["tool"],
            "ghSearchCode"
        );
    }

    #[test]
    fn removes_empty_next_map_but_keeps_opaque_query_payload() {
        let mut out = json!({"results":[{"data":{
            "next":{"viewRepo":{"tool":"ghStructure","query":{"next":{"nested":{"tool":"localFetch","query":{}}}}}},
            "context":{"next":{"nested":{"tool":"ghSearchCode","query":{"goal": "test", "reasoning":"r"}}}}
        }}]});
        filter_unavailable_cross_tool_next(&mut out, "artifactSearch", |_| false);
        assert!(out["results"][0]["data"].get("next").is_none());
        assert!(out["results"][0]["data"]["context"].get("next").is_none());
    }
}
