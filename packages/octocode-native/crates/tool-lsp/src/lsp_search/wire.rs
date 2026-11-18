//! What a finished lspSearch row leaves out because the request or the
//! row's own fields already state it. The dispatcher applies it to the
//! finished row, after pagination has kept it: no later page reads it again.
use serde_json::Value;

/// Drop `column` from each `files[].matches[]` row (no input takes a column;
/// a row is found by `line` and `symbolName`) unless the page marked it as
/// repeating the `line` and `content` of another row of its file, anywhere
/// in the answer: the column then tells the two apart
/// ([`octocode_tools_shared::same_line`]). Drop the row's `path` when it is
/// the request's own `path`. A failed row keeps its `path`.
pub fn lean(row: &mut Value, requested_path: Option<&str>) {
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
            .filter_map(Value::as_object_mut)
        {
            if !octocode_tools_shared::same_line::take_mark(found) {
                found.remove("column");
            }
        }
    }
}

#[cfg(test)]
mod tests;
