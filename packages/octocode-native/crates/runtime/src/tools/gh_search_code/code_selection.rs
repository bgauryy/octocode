//! A classified code page finished after shared selection: its reads, judge leads and field pick.
use super::{CODE_ROW_ID, add_reads, code_output};
use crate::id::ToolId;
use crate::tools::gh_shared::pick_rows;

/// Finish a classified provider page after shared sanitization and selection.
/// Read leads may only name selected evidence; `unfiltered` remains the explicit
/// route to the original page. Output fields never change the judged evidence.
/// A `hidden` file (judged related or unknown on its listed lines) whose hits
/// were not all listed gets `next.judgeHits` instead of its `readHits`: the
/// same keyword read with this query's `clasifyContext`, so its unlisted hits
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
        for (name, lead) in leads {
            if !name.starts_with("readHits") {
                continue;
            }
            for read in lead
                .pointer("/query/queries")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if read_identity(read).is_some_and(|id| hidden.contains(&id)) {
                    let mut read = read.clone();
                    if let Some(fields) = read.as_object_mut() {
                        fields.remove("ranges");
                        fields.remove("contextLines");
                        fields.insert("clasifyContext".into(), query["clasifyContext"].clone());
                    }
                    judged.push(read);
                }
            }
        }
    }
    for group in ["next", "hints"] {
        if let Some(leads) = value.get_mut(group).and_then(Value::as_object_mut) {
            leads.retain(|_, lead| {
                if lead["tool"] != ToolId::GhGetFileContent.as_str() {
                    return true;
                }
                let Some(rows) = lead
                    .pointer_mut("/query/queries")
                    .and_then(Value::as_array_mut)
                else {
                    return false;
                };
                rows.retain(|read| read_identity(read).is_some_and(|id| selected.contains(&id)));
                !rows.is_empty()
            });
        }
    }
    // The surviving `readHits*` reads are numbered from the first label, so
    // no gap suggests missing leads.
    if let Some(leads) = value.get_mut("next").and_then(Value::as_object_mut) {
        let names = leads
            .keys()
            .filter(|name| {
                name.strip_prefix("readHits")
                    .is_some_and(|n| n.chars().all(|c| c.is_ascii_digit()))
            })
            .cloned()
            .collect::<Vec<_>>();
        let reads = names
            .iter()
            .filter_map(|name| leads.remove(name))
            .collect::<Vec<_>>();
        add_reads(
            value,
            code_output::PageReads {
                top: None,
                hits: reads,
            },
        );
    }
    for (index, batch) in judged
        .chunks(crate::response::result::rows_per_call(
            ToolId::GhGetFileContent,
        ))
        .enumerate()
    {
        value["next"][crate::id::numbered("judgeHits", index)] =
            crate::response::result::Continuation::input(
                ToolId::GhGetFileContent,
                serde_json::json!({ "queries": batch }),
            )
            .build();
    }
    let fields = query["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    pick_rows(value, "files", CODE_ROW_ID, &fields);
}
