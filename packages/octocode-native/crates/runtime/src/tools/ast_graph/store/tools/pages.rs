//! Convert engine page metadata into executable tool continuations; never parse shell text.
use super::*;

fn next(row: &Value, id: &str, offset: usize, list: Option<&str>, target: Option<&str>) -> Value {
    let mut request = row.clone();
    request["graph"] = json!(id);
    request["offset"] = json!(offset);
    if let Some(list) = list {
        request["list"] = json!(list);
    }
    if let Some(target) = target {
        request["cycle"] = json!(target);
    }
    Continuation::continued(ToolId::GraphQuery, request).build()
}

pub(super) fn convert(data: &mut Value, row: &Value, intent: Option<&QueryOptions>) {
    let Some(id) = data["graph"]["id"].as_str().map(str::to_owned) else {
        return;
    };
    if let Some(node) = data.get_mut("node").and_then(Value::as_object_mut) {
        node.remove("next");
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
    let mut calls = serde_json::Map::new();
    if let Some(intent) = intent {
        let mut request = json!({
            "operation": intent.op, "path": row["path"].as_str().unwrap_or("."),
            "graph": id, "target": intent.target, "offset": intent.offset.unwrap_or(0),
        });
        if let Some(kind) = &intent.kind {
            request["kind"] = json!(kind);
        }
        if let Some(limit) = intent.limit {
            request["limit"] = json!(limit);
        }
        if let Some(provenance) = row.get("includeProvenance") {
            request["includeProvenance"] = provenance.clone();
        }
        calls.insert(
            "page".into(),
            Continuation::continued(ToolId::GraphQuery, request).build(),
        );
    } else if data["next"].is_string() {
        let offset = data["offset"].as_u64().unwrap_or(0) as usize;
        let shown = data["results"].as_array().map_or(0, Vec::len);
        calls.insert("page".into(), next(row, &id, offset + shown, None, None));
    }
    if let Some(lists) = data.get("nextLists").and_then(Value::as_object) {
        for list in lists.keys() {
            if let Some((section, field)) = list.split_once('.') {
                let shown = data[section][field].as_array().map_or(0, Vec::len);
                calls.insert(list.clone(), next(row, &id, shown, Some(list), None));
            }
        }
    }
    if let Some(rows) = data.get_mut("results").and_then(Value::as_array_mut) {
        for (index, result) in rows.iter_mut().enumerate() {
            if result["nextNodes"].is_string() {
                let nodes = result["nodes"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                calls.insert(
                    format!("cycle{index}"),
                    next(
                        row,
                        &id,
                        nodes.len(),
                        Some("cycleNodes"),
                        nodes.first().and_then(Value::as_str),
                    ),
                );
                result.as_object_mut().map(|r| r.remove("nextNodes"));
            }
        }
    }
    if let Some(object) = data.as_object_mut() {
        object.remove("next");
        object.remove("nextLists");
        if !calls.is_empty() {
            object.insert("next".into(), Value::Object(calls));
        }
    }
}
