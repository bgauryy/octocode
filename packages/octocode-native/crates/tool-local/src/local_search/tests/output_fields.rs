//! Every field a localSearch row states is one its output reference
//! declares: the reference lists localSearch's own fields only, so a field
//! the tool starts to state must be declared with it.
use super::*;
use serde_json::Value;

/// `node` with each `$ref` followed, in the tool's own definitions or the
/// contract's shared ones.
fn resolved<'a>(node: &'a Value, defs: &[&'a Value]) -> &'a Value {
    let mut at = node;
    for _ in 0..16 {
        let Some(name) = at["$ref"]
            .as_str()
            .and_then(|reference| reference.strip_prefix("#/$defs/"))
        else {
            break;
        };
        match defs.iter().find_map(|defs| defs.get(name)) {
            Some(next) => at = next,
            None => break,
        }
    }
    at
}

/// The object schemas `node` is or combines.
fn objects<'a>(node: &'a Value, defs: &[&'a Value]) -> Vec<&'a Value> {
    let at = resolved(node, defs);
    let mut found = Vec::new();
    if at.get("properties").is_some() {
        found.push(at);
    }
    for key in ["anyOf", "allOf", "oneOf"] {
        for part in at[key].as_array().into_iter().flatten() {
            found.extend(objects(part, defs));
        }
    }
    found
}

/// The property names declared where `path` reaches (`[]` steps into items).
fn declared(path: &[&str]) -> std::collections::BTreeSet<String> {
    let contract = octocode_runtime_base::contracts::tool_contract(
        octocode_runtime_base::id::ToolId::LocalSearch,
    )
    .expect("contract");
    let shared = octocode_runtime_base::contracts::parsed_contract().expect("contract");
    let output = &contract["outputSchema"];
    let defs = [&output["$defs"], &shared["$defs"]];
    let mut nodes = vec![output];
    for step in path {
        nodes = nodes
            .into_iter()
            .flat_map(|node| {
                if *step == "[]" {
                    vec![resolved(&resolved(node, &defs)["items"], &defs)]
                } else {
                    objects(node, &defs)
                        .into_iter()
                        .map(|object| resolved(&object["properties"][*step], &defs))
                        .collect()
                }
            })
            .collect();
    }
    nodes
        .into_iter()
        .flat_map(|node| objects(node, &defs))
        .flat_map(|object| object["properties"].as_object().into_iter().flatten())
        .map(|(name, _)| name.clone())
        .collect()
}

/// The keys of every object in `rows`' lists at `field`.
fn stated(rows: &[&Value], field: &str) -> std::collections::BTreeSet<String> {
    rows.iter()
        .flat_map(|row| row[field].as_array().into_iter().flatten())
        .flat_map(|item| item.as_object().into_iter().flatten())
        .map(|(key, _)| key.clone())
        .collect()
}

#[test]
fn every_emitted_local_search_field_is_declared() {
    let files = [
        (
            "lib.rs",
            "pub fn beacon() -> u32 {\n    let beacon = 1;\n    beacon + 1\n}\n",
        ),
        (
            "pkg.json",
            "{\n  \"name\": \"beacon-pkg\",\n  \"version\": \"1\"\n}\n",
        ),
        ("notes.txt", "beacon beacon\nnone\nbeacon here\n"),
    ];
    let queries = [
        serde_json::json!({"matchString": ["beacon"]}),
        serde_json::json!({"matchString": ["beacon"], "contextLines": {"before": 1, "after": 1}}),
        serde_json::json!({"matchString": ["beacon"], "view": "matchOnly", "unique": "count"}),
        serde_json::json!({"matchString": ["beacon", "here"], "view": "countLines"}),
        serde_json::json!({"matchString": ["beacon", "here"], "view": "countMatches"}),
        serde_json::json!({"matchString": ["beacon"], "view": "files"}),
        serde_json::json!({"matchString": ["beacon"], "keyPath": "name"}),
        serde_json::json!({"matchString": ["beacon"], "limit": 1}),
    ];
    let bodies = queries
        .iter()
        .map(|fields| search_fixture(&files, ls_query(fields.clone(), None)))
        .collect::<Vec<_>>();
    let file_rows = bodies
        .iter()
        .flat_map(|body| body["files"].as_array().into_iter().flatten())
        .collect::<Vec<_>>();
    assert!(!file_rows.is_empty(), "{bodies:?}");
    let file = ["results", "[]", "data", "files", "[]"];
    let declared_file = declared(&file);
    let file_keys = file_rows
        .iter()
        .flat_map(|row| row.as_object().into_iter().flatten())
        .map(|(key, _)| key.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let undeclared = file_keys.difference(&declared_file).collect::<Vec<_>>();
    assert!(
        undeclared.is_empty(),
        "file fields {undeclared:?} of {declared_file:?}"
    );
    for (list, emitted) in [
        ("matches", stated(&file_rows, "matches")),
        ("declarations", stated(&file_rows, "declarations")),
    ] {
        assert!(!emitted.is_empty(), "no {list} stated: {bodies:?}");
        let at = [&file[..], &[list, "[]"]].concat();
        let known = declared(&at);
        let undeclared = emitted.difference(&known).collect::<Vec<_>>();
        assert!(
            undeclared.is_empty(),
            "{list} fields {undeclared:?} of {known:?}"
        );
    }
}
