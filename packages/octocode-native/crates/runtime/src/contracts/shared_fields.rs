//! Leaf fields a response hoists into `shared` when every row entry holds
//! the same value, and the canonical view that restores them before the
//! output contract validates a response.
//!
//! `shared` holds two kinds of keys, each restored only where it came from:
//! entry keys (hoisted from every object entry of every array under `data`)
//! go back onto those entries; [`ROW_FIELDS`] (stated once for every row,
//! such as the checked-out head) go back onto each non-error row's `data`,
//! never onto nested entries whose strict schemas do not declare them.
use serde_json::{Map, Value};

/// Row-level fields a response may state once in `shared`. They are never
/// hoisted from entries, so a shared key has exactly one origin.
pub(crate) const ROW_FIELDS: &[&str] = &["commitSha"];

fn can_share_field(key: &str, value: &Value) -> bool {
    const EXCLUDED: &[&str] = &[
        "path",
        "dir",
        "uri",
        "absolutePath",
        "owner",
        "repo",
        "name",
        // A declaration's name is its lspSearch anchor: symbols containers
        // (whose member-less siblings are entry strings) keep it on the row.
        "symbolName",
        "id",
        "type",
        "kind",
        "reason",
        "isPartial",
        "number",
        "title",
        "state",
        "author",
        "labels",
        "createdAt",
        "mergedAt",
        "commentsCount",
        "startLine",
        "endLine",
        "start",
        "end",
        "startColumn",
        "endColumn",
        "startByte",
        "endByte",
        "line",
        "column",
        "character",
        "parentId",
        "parent",
        "named",
        "exported",
        // Per-entry match-row accounting must stay on each entry: hoisting it
        // whenever the values happen to coincide (typical on page 1) makes the
        // row shape depend on the data, so identical queries drift between
        // pages and between CLI and MCP consumers.
        "totalMatchRows",
        "returnedMatchRows",
    ];
    !EXCLUDED.contains(&key)
        && !ROW_FIELDS.contains(&key)
        && (value.is_number()
            || value.is_boolean()
            || value.as_str().is_some_and(|s| !s.is_empty()))
}

fn shared_leaves_mut(rows: &mut [Value]) -> impl Iterator<Item = &mut Map<String, Value>> {
    rows.iter_mut()
        .filter_map(|row| row["data"].as_object_mut())
        .flat_map(|data| data.values_mut().filter_map(Value::as_array_mut))
        .flatten()
        .filter_map(Value::as_object_mut)
}

/// Restore the canonical evidence view for validation without changing the
/// compact response returned to the caller. Explicit leaf values take priority.
pub(crate) fn restore(output: &mut Value) {
    let shared = match output.get("shared").and_then(Value::as_object) {
        Some(shared) => shared.clone(),
        None => return,
    };
    let Some(rows) = output.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    for row in rows.iter_mut().filter(|row| row["status"] != "error") {
        let Some(data) = row["data"].as_object_mut() else {
            continue;
        };
        for (key, value) in shared
            .iter()
            .filter(|(key, _)| ROW_FIELDS.contains(&key.as_str()))
        {
            data.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    for leaf in shared_leaves_mut(rows) {
        for (key, value) in &shared {
            if can_share_field(key, value) {
                leaf.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }
    }
}

/// Hoist leaf fields every row entry shares into one `shared` map.
pub(crate) fn hoist(rows: &mut [Value]) -> Option<Map<String, Value>> {
    let leaves: Vec<&Map<String, Value>> = rows
        .iter()
        .filter_map(|row| row["data"].as_object())
        .flat_map(|data| data.values().filter_map(Value::as_array))
        .flatten()
        .filter_map(Value::as_object)
        .collect();
    if leaves.len() < 2 {
        return None;
    }
    let shared: Map<String, Value> = leaves[0]
        .iter()
        .filter(|(key, value)| {
            can_share_field(key, value) && leaves.iter().all(|leaf| leaf.get(*key) == Some(*value))
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if shared.is_empty() {
        return None;
    }
    for leaf in shared_leaves_mut(rows) {
        for key in shared.keys() {
            leaf.remove(key);
        }
    }
    Some(shared)
}

#[cfg(test)]
mod tests {
    use crate::contracts::validate_output;
    use serde_json::json;

    /// The checked-out head is a row field stated once in `shared`; it never
    /// belongs on nested entries such as localFetch `blocks`.
    #[test]
    fn local_fetch_blocks_validate_with_a_shared_head() {
        let output = json!({"shared":{"commitSha":"abc123"},"results":[{"index":0,"data":{
            "path":"src/a.ts","content":"1\tfunction a() {}\n","totalLines":1,
            "blocks":[{"symbolName":"a","line":1,"endLine":1}]
        }}]});
        validate_output("localFetch", &output).expect("shared head validates on row data");
    }

    /// One representative response per tool, with the nested arrays each
    /// tool emits (localFetch/ghGetFileContent `blocks`, lspSearch
    /// `builtinLib`, ghStructure ref maps, gh rows `lines`, history rows),
    /// compacted as the response stage does (entry hoist, plus the shared
    /// checked-out head on local reads), validates after restore.
    #[test]
    fn compacted_representative_responses_validate_for_every_tool() {
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let block = json!([{"symbolName":"a","line":1,"endLine":3},{"symbolName":"b","line":5,"endLine":9}]);
        let cases = [
            (
                "localSearch",
                true,
                json!({"files":[
                    {"path":"a.rs","matches":[{"line":3,"value":"fn restore()"}]},
                    {"path":"b.rs","matches":[{"line":4,"value":"fn restore()"}]}
                ]}),
            ),
            (
                "localFetch",
                true,
                json!({"path":"a.ts","content":"1\tx\n","totalLines":9,"blocks":block}),
            ),
            (
                "structureSearch",
                true,
                json!({"path":"src","files":[{"dir":"src","files":["a.rs (1)"]},{"dir":"src/d","files":["b.rs (2)"]}]}),
            ),
            (
                "lspSearch",
                false,
                json!({"path":"a.ts","payload":{"kind":"callees","files":[
                {"path":"a.ts","matches":[{"symbolName":"b","kind":"function","line":4,"endLine":6,"sites":[{"line":2,"column":1}]}]}
            ],"builtinLib":["map","filter"]},
                "hints":{"didYouMean":{"tool":"lspSearch","query":{"queries":[{"path":"a.ts","symbolName":"a","lineHint":2,"operation":"callees"}]}}}}),
            ),
            (
                "astSearch",
                false,
                json!({"files":[
                    {"path":"a.ts","matches":[{"line":1,"column":1,"endLine":2,"value":"f(x)"}]},{"path":"b.ts","matches":[{"line":3,"column":1,"endLine":4,"value":"f(y)"}]}
                ]}),
            ),
            (
                "ghSearchCode",
                false,
                json!({"files":[
                    {"path":"a.js","lines":["5\tvar a = 1;"],"commitSha":sha},
                    {"path":"b.js","lines":["6\tvar b = 2;"],"commitSha":sha}
                ]}),
            ),
            (
                "ghGetFileContent",
                false,
                json!({"path":"lib/a.js","content":"1\tx\n","totalLines":9,"commitSha":sha,"blocks":block}),
            ),
            (
                "ghStructure",
                false,
                json!({"defaultBranch":"main","branches":{"main":sha,"dev":sha},"tags":{"v1":sha}}),
            ),
            (
                "ghStructure",
                false,
                json!({"entries":[{"dir":"lib","files":["a.js","b.js"]},{"dir":"lib/x","files":["c.js"]}],"commitSha":sha}),
            ),
            (
                "ghSearchHistory",
                false,
                json!({"commits":[
                    {"sha":sha,"date":"2026-10-06T21:16:14Z","messageHeadline":"a (#1)","author":"x","prNumber":1},
                    {"sha":"abc","date":"2026-10-06T21:16:14Z","messageHeadline":"b (#2)","author":"x","prNumber":2}
                ]}),
            ),
            (
                "ghGetHistoryItem",
                false,
                json!({"pullRequests":[{"number":1,"title":"t","state":"merged","author":"x","createdAt":"2026-10-06T21:16:14Z",
                "targetBranch":"main","sourceBranch":"f","sourceSha":sha,"targetSha":sha,"mergeCommitSha":sha,
                "commitsCount":1,"changedFilesCount":2,"additions":2,"deletions":2}]}),
            ),
            (
                "ghSearchRepo",
                false,
                json!({"repositories":[
                    {"owner":"o","repo":"a","stars":1,"language":"JavaScript","topics":["x"]},
                    {"owner":"o","repo":"b","stars":2,"language":"JavaScript","topics":["y"]}
                ]}),
            ),
        ];
        for (tool, local, data) in cases {
            let rows = vec![
                json!({"index":0,"data":data.clone()}),
                json!({"index":1,"data":data}),
            ];
            let mut output = crate::response::rows::envelope(rows);
            if local {
                output["shared"]["commitSha"] = json!(sha);
            }
            if let Err(error) = validate_output(tool, &output) {
                panic!("{tool}: {:?}\n{output}", error.issues);
            }
        }
    }

    /// Outline containers keep their names even when every one shares it
    /// (two `impl Foo` blocks); member-less declarations are entry strings,
    /// never hoisted.
    #[test]
    fn outline_container_names_stay_on_their_rows() {
        let mut rows = vec![json!({"index":0,"data":{"symbols":[
            "Foo (1, struct)",
            {"symbolName":"Foo","kind":"impl","line":3,"endLine":5,"members":["a (4, function)"]},
            {"symbolName":"Foo","kind":"impl","line":7,"endLine":9,"members":["b (8, function)"]}
        ]}})];
        assert!(super::hoist(&mut rows).is_none(), "{rows:?}");
        assert_eq!(rows[0]["data"]["symbols"][1]["symbolName"], "Foo");
    }
}
