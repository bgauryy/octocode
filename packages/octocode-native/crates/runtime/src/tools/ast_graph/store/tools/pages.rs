//! State an answer's typed continuations as executable calls on the public
//! row: `next.<channel>` for continued work, `related` for leads.
use super::*;
use serde_json::Map;

/// The call that resumes `row`, the request this answer serves, at `resume`.
fn next(row: &Value, id: &str, resume: &Resume) -> Value {
    let mut request = match resume {
        Resume::Row { offset, list, cycle } => {
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
    Continuation::continued(ToolId::GraphQuery, request).build()
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
            Some("symbol") => &["callers", "callees"],
            _ => &["dependents"],
        };
        let related = operations
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
        data["related"] = json!(related);
    }
    let calls = out
        .next
        .iter()
        .map(|(channel, resume)| (channel.name(), next(row, &id, resume)))
        .collect::<Map<_, _>>();
    if !calls.is_empty() {
        out.value["next"] = Value::Object(calls);
    }
}

/// A not-found reference's ranked nodes as ready calls, the one error shape:
/// the same query at each suggested target.
fn suggested_calls(data: &mut Map<String, Value>, row: &Value) {
    let Some(Value::Array(suggestions)) = data.remove("suggestions") else {
        return;
    };
    let calls = suggestions
        .iter()
        .filter_map(Value::as_str)
        .map(|target| {
            let mut request = row.clone();
            request["target"] = json!(target);
            Continuation::related(ToolId::GraphQuery, request).build()
        })
        .collect::<Vec<_>>();
    data.insert("related".into(), Value::Array(calls));
}
