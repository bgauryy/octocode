//! Continuation compaction: drop query fields that validation would restore.
//!
//! Every emitted `{tool, query}` continuation is replayed through contract
//! validation, which re-applies schema defaults. A field is therefore removed
//! only when validating the query without it yields the identical validated
//! query, so the compact continuation executes exactly like the full one.

use std::collections::HashMap;

use serde_json::{Map, Value};

use super::channels::{HINTS_KEY, PAGES_KEY};
use crate::contracts::validate_query;
use crate::tools::id::ToolId;

/// The optional per-row brief a continuation inherits from its source row.
pub const BRIEF_FIELDS: [&str; 2] = ["mainGoal", "reasoning"];

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

/// Gives every runtime-emitted continuation the brief (`mainGoal`,
/// `reasoning`) of the input query that produced its row, but only the
/// fields that query sent: a caller who sent no brief gets hint queries
/// without one. `row_queries[i]` is the input query of result row `i`
/// (`None` for a rejected row). A brief a continuation already carries is
/// kept.
pub fn inherit_briefs(structured: &mut Value, row_queries: &[Option<&Value>]) {
    let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    for (position, row) in rows.iter_mut().enumerate() {
        let index = row
            .get("index")
            .and_then(Value::as_u64)
            .and_then(|index| usize::try_from(index).ok())
            .unwrap_or(position);
        if let Some(Some(query)) = row_queries.get(index) {
            brief_walk(row, &brief_of(query));
        }
    }
}

/// Clasify results: each result query inherits the brief of the input
/// matrix with the same id (else the one at the same position).
pub fn inherit_clasify_briefs(structured: &mut Value, input: &Value) {
    let queries: Vec<&Value> = match input.get("queries").and_then(Value::as_array) {
        Some(rows) => rows.iter().collect(),
        None => vec![input],
    };
    let Some(rows) = structured.get_mut("queries").and_then(Value::as_array_mut) else {
        return;
    };
    for (position, row) in rows.iter_mut().enumerate() {
        let id = row.get("queryId").cloned();
        let query = queries
            .iter()
            .find(|query| id.is_some() && query.get("id") == id.as_ref())
            .or_else(|| queries.get(position));
        if let Some(query) = query {
            // next.clasify reads inherit the matrix brief when replayed, so
            // their nested queries do not repeat it.
            let resume = row
                .get_mut(PAGES_KEY)
                .and_then(Value::as_object_mut)
                .and_then(|next| next.remove("clasify"));
            brief_walk(row, &brief_of(query));
            if let Some(resume) = resume {
                row[PAGES_KEY]["clasify"] = resume;
            }
        }
    }
}

fn brief_of(query: &Value) -> Map<String, Value> {
    BRIEF_FIELDS
        .into_iter()
        .filter_map(|field| {
            query
                .get(field)
                .filter(|value| value.is_string())
                .map(|value| (field.to_owned(), value.clone()))
        })
        .collect()
}

fn brief_walk(value: &mut Value, brief: &Map<String, Value>) {
    if brief.is_empty() {
        return;
    }
    if continuation_target(value).is_some() {
        fill_brief(value, brief);
        return;
    }
    match value {
        Value::Array(items) => items.iter_mut().for_each(|item| brief_walk(item, brief)),
        Value::Object(map) => map.values_mut().for_each(|child| brief_walk(child, brief)),
        _ => {}
    }
}

fn fill_brief(continuation: &mut Value, brief: &Map<String, Value>) {
    let Some(query) = continuation.get_mut("query") else {
        return;
    };
    let rows: Vec<&mut Value> = match query.get_mut("queries").and_then(Value::as_array_mut) {
        Some(rows) => rows.iter_mut().collect(),
        None => vec![query],
    };
    for row in rows {
        if let Some(object) = row.as_object_mut() {
            for (field, value) in brief {
                object.entry(field.clone()).or_insert_with(|| value.clone());
            }
        }
    }
}

/// Drop optional cross-tool next actions the current surface cannot execute.
/// Returns the target of every dropped action, in walk order.
pub fn filter_unavailable_cross_tool_next(
    value: &mut Value,
    current_tool: &str,
    is_available: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut dropped = Vec::new();
    filter_walk(value, current_tool, &is_available, &mut dropped);
    dropped
}

/// [`filter_unavailable_cross_tool_next`] for each result row, then the
/// envelope. A row whose dropped targets `disclose` names (with the reason)
/// gets one warning that counts them, so a scoped surface never loses a lead
/// silently; drops `disclose` skips (a tool the user disabled) stay silent.
/// Returns the number of dropped actions.
pub fn filter_rows_and_disclose<'a>(
    structured: &mut Value,
    current_tool: &str,
    is_available: impl Fn(&str) -> bool,
    disclose: impl Fn(&str) -> Option<&'a str>,
) -> usize {
    let mut total = 0;
    for row in structured
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let dropped = filter_unavailable_cross_tool_next(row, current_tool, &is_available);
        total += dropped.len();
        let named: Vec<(&str, &str)> = dropped
            .iter()
            .filter_map(|target| disclose(target).map(|reason| (target.as_str(), reason)))
            .collect();
        let Some(&(_, reason)) = named.first() else {
            continue;
        };
        let mut tools: Vec<&str> = named.iter().map(|(target, _)| *target).collect();
        tools.sort_unstable();
        tools.dedup();
        let count = named.len();
        let warning = format!(
            "Dropped {count} lead{} to {}: outside {reason}.",
            if count == 1 { "" } else { "s" },
            tools.join(", ")
        );
        if let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) {
            match data
                .entry("warnings")
                .or_insert_with(|| Value::Array(Vec::new()))
            {
                Value::Array(warnings) => warnings.push(Value::String(warning)),
                other => *other = Value::Array(vec![Value::String(warning)]),
            }
        }
    }
    total + filter_unavailable_cross_tool_next(structured, current_tool, &is_available).len()
}

fn continuation_target(value: &Value) -> Option<&str> {
    value
        .as_object()
        .filter(|map| map.get("query").is_some_and(Value::is_object))?
        .get("tool")?
        .as_str()
}

fn filter_walk(
    value: &mut Value,
    current_tool: &str,
    is_available: &impl Fn(&str) -> bool,
    dropped: &mut Vec<String>,
) {
    // A continuation query is caller content, not another output tree.
    if continuation_target(value).is_some() {
        return;
    }
    match value {
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| filter_walk(item, current_tool, is_available, dropped)),
        Value::Object(map) => map.retain(|key, child| {
            if key == PAGES_KEY || key == HINTS_KEY {
                filter_next(child, current_tool, is_available, dropped)
            } else {
                filter_walk(child, current_tool, is_available, dropped);
                true
            }
        }),
        _ => {}
    }
}

fn filter_next(
    next: &mut Value,
    current_tool: &str,
    is_available: &impl Fn(&str) -> bool,
    dropped: &mut Vec<String>,
) -> bool {
    let keep = |target: &str, dropped: &mut Vec<String>| {
        let kept = target == current_tool || is_available(target);
        if !kept {
            dropped.push(target.to_owned());
        }
        kept
    };
    if let Some(target) = continuation_target(next) {
        return keep(target, dropped);
    }
    if let Some(map) = next.as_object_mut() {
        map.retain(|_, action| {
            if let Some(target) = continuation_target(action) {
                keep(target, dropped)
            } else {
                filter_walk(action, current_tool, is_available, dropped);
                true
            }
        });
        return !map.is_empty();
    }
    true
}

/// Continuations live only under `next` and `hints` keys (`data.next.*`,
/// `data.hints.*`, `responsePagination.next`, nested ones); a `{tool,
/// query}` shape anywhere else is tool data and stays untouched.
fn walk(value: &mut Value, memo: &mut Memo) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(|item| walk(item, memo)),
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if key == PAGES_KEY || key == HINTS_KEY {
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
        for (name, continuation) in map.iter_mut() {
            // clasify's own `next.clasify` is a bare matrix, not `{tool, query}`.
            if !compact_continuation(continuation, memo) && name == ToolId::Clasify.as_str() {
                compact_clasify_contexts(continuation, memo);
            }
        }
    }
}

/// A clasify matrix nests one ordinary read query per resource; compact each
/// against its own tool (the matrix itself is validated as clasify).
fn compact_clasify_contexts(matrix: &mut Value, memo: &mut Memo) {
    for resource in matrix
        .get_mut("resources")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let Some(context) = resource.get_mut("context").and_then(Value::as_object_mut) else {
            continue;
        };
        let Some(tool) = context
            .get("tool")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            continue;
        };
        if let Some(query) = context.get_mut("query").filter(|query| query.is_object()) {
            compact_query(&tool, query, memo);
        }
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
        Some(queries) => queries.iter_mut().for_each(|row| {
            compact_query(tool, row, memo);
            if tool == ToolId::Clasify.as_str() {
                compact_clasify_contexts(row, memo);
            }
        }),
        None => {
            compact_query(tool, query, memo);
            if tool == ToolId::Clasify.as_str() {
                compact_clasify_contexts(query, memo);
            }
        }
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
    fn continuations_inherit_the_brief_of_their_input_query() {
        let first = json!({"mainGoal":"Find a.","reasoning":"First."});
        let second = json!({"mainGoal":"Find b.","reasoning":"Second."});
        let mut out = json!({"results":[
            {"index":0,"data":{"next":{
                "viewRepo":{"tool":"ghStructure","query":{"owner":"o","repo":"r"}},
                "clasify":{"tool":"clasify","query":{"mainGoal":"Locate.","reasoning":"Unread.","resources":[],"questions":[]}}
            }}},
            {"index":1,"data":{"items":[{"next":{"tool":"localFetch","query":{"queries":[{"path":"/a"}]}}}]}}
        ]});
        inherit_briefs(&mut out, &[Some(&first), Some(&second)]);
        let view = &out["results"][0]["data"]["next"]["viewRepo"]["query"];
        assert_eq!(
            view,
            &json!({"owner":"o","repo":"r","mainGoal":"Find a.","reasoning":"First."})
        );
        let clasify = &out["results"][0]["data"]["next"]["clasify"]["query"];
        assert_eq!(clasify["mainGoal"], "Locate.", "an existing brief is kept");
        assert_eq!(
            out["results"][1]["data"]["items"][0]["next"]["query"]["queries"][0],
            json!({"path":"/a","mainGoal":"Find b.","reasoning":"Second."})
        );
    }

    #[test]
    fn clasify_page_reads_inherit_their_matrix_brief_by_query_id() {
        let input = json!({"queries":[{"id":"m1","mainGoal":"G1","reasoning":"R1"},{"id":"m2","mainGoal":"G2","reasoning":"R2"}]});
        let mut out = json!({"queries":[{"queryId":"m2","resources":[{"pages":[{"next":{"read":{"tool":"localFetch","query":{"path":"/a"}}}}]}]}]});
        inherit_clasify_briefs(&mut out, &input);
        assert_eq!(
            out["queries"][0]["resources"][0]["pages"][0]["next"]["read"]["query"],
            json!({"path":"/a","mainGoal":"G2","reasoning":"R2"})
        );
    }

    #[test]
    fn drops_only_fields_validation_restores() {
        let mut out = json!({"results":[{"data":{"next":{"nextPage":{"tool":"localSearch","confidence":"exact","query":{
            "searchText":"foo","path":"/tmp","mainGoal":"Find foo.","reasoning":"r","debug":false,"caseMode":"smart",
            "matchContentLength":200,"page":2,"contextLines":0,"regex":"rust"
        }}}}}]});
        compact_continuations(&mut out);
        let query = &out["results"][0]["data"]["next"]["nextPage"]["query"];
        // matchContentLength has no schema default (it scales with
        // contextLines), so validation cannot restore it and it is kept.
        assert_eq!(
            query,
            &json!({"searchText":"foo","path":"/tmp","mainGoal":"Find foo.","reasoning":"r","matchContentLength":200,"page":2,"contextLines":0})
        );
        let full = validate_query("localSearch", query.clone()).expect("valid");
        assert_eq!(full["matchContentLength"], 200);
    }

    #[test]
    fn leaves_tool_query_shaped_data_outside_next_untouched() {
        let data = json!({"tool":"localSearch","query":{"searchText":"a","path":"/tmp","mainGoal":"Find a.","reasoning":"r","debug":false}});
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
        let mut input = json!({"queries":[{"searchText":"a","path":"/tmp","mainGoal":"Find a.","reasoning":"r","debug":false}],
            "responseCharOffset":10});
        compact_input("localSearch", &mut input);
        assert_eq!(
            input["queries"][0],
            json!({"searchText":"a","path":"/tmp","mainGoal":"Find a.","reasoning":"r"})
        );
        assert_eq!(input["responseCharOffset"], 10);
        let mut invalid = json!({"tool":"localSearch","query":{"debug":false}});
        compact_continuations(&mut invalid);
        assert_eq!(invalid["query"], json!({"debug":false}));
    }

    #[test]
    fn filters_unavailable_cross_tool_actions_without_touching_pagination_or_query_data() {
        let query = json!({"mainGoal": "test", "reasoning":"continue", "next":{"readFile":{"tool":"localFetch","query":{"path":"/tmp/a"}}}});
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
    fn filter_reports_every_dropped_target() {
        let mut out = json!({"results":[{"data":{"next":{
            "readTopMatch":{"tool":"ghGetFileContent","query":{"path":"a.rs"}},
            "readHits2":{"tool":"ghGetFileContent","query":{"path":"b.rs"}},
            "nextPage":{"tool":"ghSearchCode","query":{"page":2}}
        },"hints":{"searchLocal":{"tool":"localSearch","query":{"path":"/x"}}}}}]});
        let dropped = filter_unavailable_cross_tool_next(&mut out, "ghSearchCode", |target| {
            target == "localSearch"
        });
        assert_eq!(dropped, vec!["ghGetFileContent", "ghGetFileContent"]);
        assert_eq!(
            out["results"][0]["data"]["next"]["nextPage"]["tool"],
            "ghSearchCode"
        );
        assert_eq!(
            out["results"][0]["data"]["hints"]["searchLocal"]["tool"],
            "localSearch"
        );
    }

    #[test]
    fn rows_disclose_only_drops_the_disclose_rule_names() {
        let mut out = json!({"results":[
            {"data":{"next":{"materialized":{"tool":"localSearch","query":{"path":"/x"}},
                             "clasify":{"tool":"clasify","query":{}}},
                     "warnings":["tool warning"]}},
            {"data":{"next":{"clasify":{"tool":"clasify","query":{}}}}},
            {"data":{"hints":{"a":{"tool":"localFetch","query":{"path":"/a"}},
                              "b":{"tool":"localSearch","query":{"path":"/b"}},
                              "c":{"tool":"localFetch","query":{"path":"/c"}}}}}
        ]});
        let dropped = filter_rows_and_disclose(
            &mut out,
            "ghStructure",
            |_| false,
            |target| (target != "clasify").then_some("tools.family github"),
        );
        assert_eq!(dropped, 6);
        let rows = out["results"].as_array().unwrap();
        assert_eq!(
            rows[0]["data"]["warnings"],
            json!([
                "tool warning",
                "Dropped 1 lead to localSearch: outside tools.family github."
            ])
        );
        assert!(rows[0]["data"].get("next").is_none());
        // A drop the rule does not name (a disabled tool) stays silent.
        assert!(rows[1]["data"].get("warnings").is_none());
        assert_eq!(
            rows[2]["data"]["warnings"],
            json!(["Dropped 3 leads to localFetch, localSearch: outside tools.family github."])
        );
    }

    #[test]
    fn removes_empty_next_map_but_keeps_opaque_query_payload() {
        let mut out = json!({"results":[{"data":{
            "next":{"viewRepo":{"tool":"ghStructure","query":{"next":{"nested":{"tool":"localFetch","query":{}}}}}},
            "context":{"next":{"nested":{"tool":"ghSearchCode","query":{"mainGoal": "test", "reasoning":"r"}}}}
        }}]});
        filter_unavailable_cross_tool_next(&mut out, "artifactSearch", |_| false);
        assert!(out["results"][0]["data"].get("next").is_none());
        assert!(out["results"][0]["data"]["context"].get("next").is_none());
    }
}
