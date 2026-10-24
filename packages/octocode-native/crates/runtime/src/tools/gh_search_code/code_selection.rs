//! A classified code page finished after shared selection: its reads, judge leads and field pick.
use crate::id::ToolId;

/// Finish a classified provider page after shared sanitization and selection.
/// Read leads may only name selected evidence; `unfiltered` remains the explicit
/// route to the original page. Output fields never change the judged evidence.
/// A `hidden` file (judged related or unknown on its listed lines) whose hits
/// were not all listed gets `next.judgeHits` instead of its `readHits`: the
/// same keyword read with this query's `classification`, so its unlisted hits
/// are judged in their own windows (the deciding line of a capped file is
/// often past the five it lists).
pub(super) fn finish_code_selection(
    value: &mut serde_json::Value,
    query: &serde_json::Value,
    hidden: &[serde_json::Value],
) {
    use serde_json::Value;
    type Identity = (String, String, String);
    let identities = |file: &Value, value: &Value| -> Vec<Identity> {
        let identity = |key: &str| {
            file[key]
                .as_str()
                .or_else(|| value[key].as_str())
                .or_else(|| query[key].as_str())
                .unwrap_or_default()
                .to_owned()
        };
        let (owner, repo) = (identity("owner"), identity("repo"));
        file["path"]
            .as_str()
            .into_iter()
            .chain(
                file["alsoAt"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str),
            )
            .map(|path| (owner.clone(), repo.clone(), path.to_owned()))
            .collect()
    };
    let selected = value["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|file| identities(file, value))
        .collect::<std::collections::HashSet<_>>();
    let hidden = hidden
        .iter()
        .flat_map(|file| identities(file, value))
        .collect::<std::collections::HashSet<_>>();
    let read_identity = |read: &Value| -> Option<Identity> {
        Some((
            read["owner"].as_str()?.to_owned(),
            read["repo"].as_str()?.to_owned(),
            read["path"].as_str()?.to_owned(),
        ))
    };
    let mut judged = Vec::new();
    if let Some(leads) = value.get("next").and_then(Value::as_object) {
        for lead in leads.values() {
            if lead["tool"] != ToolId::GhGetFileContent.as_str() {
                continue;
            }
            for read in lead
                .pointer("/input/queries")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if read.get("matchString").is_some()
                    && read_identity(read).is_some_and(|id| hidden.contains(&id))
                {
                    let mut read = read.clone();
                    if let Some(fields) = read.as_object_mut() {
                        fields.remove("ranges");
                        fields.remove("contextLines");
                        fields.insert("classification".into(), query["classification"].clone());
                    }
                    if !judged.contains(&read) {
                        judged.push(read);
                    }
                }
            }
        }
    }
    if let Some(leads) = value.get_mut("next").and_then(Value::as_object_mut) {
        leads.retain(|_, lead| {
            if lead["tool"] != ToolId::GhGetFileContent.as_str() {
                return true;
            }
            let Some(rows) = lead
                .pointer_mut("/input/queries")
                .and_then(Value::as_array_mut)
            else {
                return false;
            };
            rows.retain(|read| read_identity(read).is_some_and(|id| selected.contains(&id)));
            !rows.is_empty()
        });
    }

    for batch in judged.chunks(crate::response::result::rows_per_call(
        ToolId::GhGetFileContent,
    )) {
        let call = crate::response::result::Continuation::related_input(
            ToolId::GhGetFileContent,
            serde_json::json!({ "queries": batch }),
        )
        .build();
        crate::response::result::attach_related(value, call);
    }
}
