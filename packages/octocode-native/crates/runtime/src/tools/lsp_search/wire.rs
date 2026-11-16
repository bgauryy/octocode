//! What a finished lspSearch row leaves out because the request or the
//! row's own fields already state it. The dispatcher applies it to the
//! finished row, after pagination has kept it: no later page reads it again.
use serde_json::Value;

/// Drop `column` from each `files[].matches[]` row (no input takes a column;
/// a row is found by `line` and `symbolName`) and the row's `path` when it
/// is the request's own `path`. A failed row keeps its `path`.
pub(crate) fn lean(row: &mut Value, requested_path: Option<&str>) {
    let Some(fields) = row.as_object_mut() else {
        return;
    };
    if let Some(requested) = requested_path
        && !fields.contains_key("errorCode")
        && fields.get("path").and_then(Value::as_str) == Some(requested)
    {
        fields.remove("path");
    }
    for file in fields
        .get_mut("files")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        for found in file
            .get_mut("matches")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            if let Some(found) = found.as_object_mut() {
                found.remove("column");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::lean;
    use serde_json::json;

    #[test]
    fn a_row_drops_the_column_and_the_path_the_request_states() {
        let mut row = json!({"path":"a.rs","workspaceRoot":"/w","files":[{"path":"a.rs",
            "matches":[{"line":3,"column":7,"role":"call"}]}]});
        lean(&mut row, Some("a.rs"));
        assert_eq!(
            row,
            json!({"workspaceRoot":"/w","files":[{"path":"a.rs","matches":[{"line":3,"role":"call"}]}]})
        );
        // A path the request did not state, and a failed row's path, stay.
        let mut other = json!({"path":"b.rs"});
        lean(&mut other, Some("a.rs"));
        assert_eq!(other, json!({"path":"b.rs"}));
        let mut failed = json!({"path":"a.rs","errorCode":"notFound"});
        lean(&mut failed, Some("a.rs"));
        assert_eq!(failed["path"], "a.rs");
    }
}
