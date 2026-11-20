//! State an answer's typed continuations as executable calls on the public
//! row: `next.<channel>` for continued work, `related` for leads.
use super::*;
use octocode_runtime_base::id::ToolId;
use serde_json::Map;

/// The call that resumes `row`, the request this answer serves, at `resume`.
fn next(row: &Value, id: &str, resume: &Resume) -> Continuation {
    let mut request = match resume {
        Resume::Row {
            offset,
            list,
            cycle,
        } => {
            let mut request = row.clone();
            request["offset"] = json!(offset);
            if let Some(list) = list {
                request["list"] = json!(list);
            }
            if let Some(cycle) = cycle {
                request["cycle"] = json!(cycle);
            }
            request
        }
        Resume::Query(options) => {
            let mut request = super::super::query::query_row(options, id);
            request.insert("offset".into(), json!(options.offset.unwrap_or(0)));
            Value::Object(request)
        }
    };
    request["graph"] = json!(id);
    request["page"] = json!(row["page"].as_u64().unwrap_or(1) + 1);
    Continuation::continued(ToolId::GraphQuery, request)
}

pub(super) fn convert(out: &mut GraphOutput, row: &Value) {
    let data = &mut out.value;
    if let Some(object) = data.as_object_mut() {
        suggested_calls(object, row);
    }
    let Some(id) = data["graph"]["id"].as_str().map(str::to_owned) else {
        return;
    };
    if let Some(node) = data.get("node").and_then(Value::as_object) {
        let operations: &[&str] = match node.get("kind").and_then(Value::as_str) {
            Some("file") => &["symbols", "deps", "dependents"],
            // A symbol states its declaration kind beside its symbolName.
            _ if node.contains_key("symbolName") => &["callers", "callees"],
            _ => &["dependents"],
        };
        let graph_calls = operations
            .iter()
            .map(|operation| {
                let target = node
                    .get("id")
                    .cloned()
                    .unwrap_or_else(|| row["target"].clone());
                let request = json!({
                    "operation": operation,
                    "graph": id,
                    "path": row["path"].as_str().unwrap_or("."),
                    "target": target,
                });
                Continuation::related(ToolId::GraphQuery, request).build()
            })
            .collect::<Vec<_>>();
        for call in graph_calls {
            octocode_runtime_base::response::result::attach_related(data, call);
        }
    }
    for (channel, resume) in &out.next {
        let call = next(row, &id, resume);
        match channel {
            Channel::NextPage => octocode_runtime_base::response::collection::insert_walk(
                &mut out.value["next"],
                ToolId::GraphQuery,
                None,
                call,
            ),
            Channel::Cycle(_) | Channel::List(_) => {
                octocode_runtime_base::response::result::set_next(
                    &mut out.value,
                    &channel.name(),
                    call.build(),
                );
            }
        }
    }
}

/// A not-found reference's ranked nodes as ready calls, the one error shape:
/// the same query at each suggested target, then the `find` call for its
/// name.
fn suggested_calls(data: &mut Map<String, Value>, row: &Value) {
    let mut calls = Vec::new();
    if let Some(Value::Array(suggestions)) = data.remove("suggestions") {
        calls.extend(suggestions.iter().filter_map(Value::as_str).map(|target| {
            let mut request = row.clone();
            request["target"] = json!(target);
            Continuation::related(ToolId::GraphQuery, request).build()
        }));
    }
    if let Some(Value::String(name)) = data.remove("find") {
        let mut request = json!({
            "operation": "find",
            "path": row.get("path").cloned().unwrap_or_else(|| json!(".")),
            "target": name,
        });
        if let Some(graph) = row.get("graph") {
            request["graph"] = graph.clone();
        }
        calls.push(Continuation::related(ToolId::GraphQuery, request).build());
    }
    if !calls.is_empty() {
        data.insert("related".into(), Value::Array(calls));
    }
}
