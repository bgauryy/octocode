use super::{all_paths, prune_row};
use crate::id::ToolId;
use serde_json::{Map, Value, json};

/// A minimal valid data row per tool whose arrays hold one element, so
/// every extra path has somewhere to land.
fn base_data(tool: ToolId) -> Value {
    match tool {
        // An owner listing's page facts carry the `providerPagesRead` stat.
        ToolId::GhSearchRepo => json!({
            "repositories":[{"owner":"o","repo":"r"}],
            "pagination":{"currentPage":1,"hasMore":false}
        }),
        ToolId::GhSearchCode => {
            json!({"files":[{"path":"a.rs","matches":[{"value":"x"}]}]})
        }
        ToolId::GhStructure => json!({"files":[]}),
        ToolId::GhGetFileContent => json!({"path":"a.rs","content":"x"}),
        ToolId::GhSearchHistory => json!({"commits":[]}),
        ToolId::GhGetHistoryItem => json!({"commitSha":"abc"}),
        ToolId::ArtifactSearch => json!({"artifacts":[{"name":"x"}]}),
        ToolId::GhCloneRepo => json!({"error":"x"}),
        ToolId::LocalSearch => json!({
            "matchCount":1,
            "files":[{"path":"a.rs","matches":[{"value":"x"}]}]
        }),
        ToolId::StructureSearch => json!({"files":[]}),
        ToolId::AstSearch => json!({
            "files":[{"path":"a.rs","matches":[{"value":"x"}]}],
            "nodes":["1 k 1:1-1:2"]
        }),
        ToolId::LocalFetch => json!({"path":"a.rs","content":"x"}),
        ToolId::LspSearch => {
            json!({"kind":"references","files":[{"path":"a.ts","matches":[{"line":1}]}],"lsp":{},"coverage":{"state":"complete"}})
        }
        ToolId::Clasify => Value::Null,
    }
}

fn resolve<'a>(schema: &'a Value, root: &'a Value) -> &'a Value {
    crate::contracts::resolve_ref(schema, &root["$defs"])
}

/// The first concrete value a schema accepts: required properties filled,
/// the first enum value, the first non-null type.
fn sample(schema: &Value, root: &Value) -> Value {
    let schema = resolve(schema, root);
    if let Some(first) = schema["enum"].as_array().and_then(|values| values.first()) {
        return first.clone();
    }
    if let Some(constant) = schema.get("const") {
        return constant.clone();
    }
    for key in ["anyOf", "oneOf"] {
        if let Some(branch) = schema[key]
            .as_array()
            .and_then(|branches| branches.iter().find(|b| b["type"] != "null"))
        {
            return sample(branch, root);
        }
    }
    match schema["type"].as_str() {
        Some("string") => json!("x"),
        Some("integer" | "number") => json!(1),
        Some("boolean") => json!(true),
        Some("array") => json!([]),
        Some("object") => {
            let mut object = Map::new();
            for name in schema["required"].as_array().into_iter().flatten() {
                if let Some(name) = name.as_str() {
                    object.insert(name.into(), sample(&schema["properties"][name], root));
                }
            }
            Value::Object(object)
        }
        _ => json!("x"),
    }
}

/// Places a sample value at `path` (contract form below `results[]`).
fn inject(row: &mut Value, path: &str, root: &Value) {
    let mut schemas = crate::contracts::property_schemas(root, &root["$defs"], "results")
        .into_iter()
        .map(|results| &resolve(results, root)["items"])
        .collect::<Vec<_>>();
    let mut targets = vec![&mut *row];
    let parts = path.split('.').collect::<Vec<_>>();
    for (index, part) in parts.iter().enumerate() {
        let (name, each) = part
            .strip_suffix("[]")
            .map_or((*part, false), |name| (name, true));
        let mut found = Vec::new();
        for schema in &schemas {
            found.extend(crate::contracts::property_schemas(
                schema,
                &root["$defs"],
                name,
            ));
        }
        assert!(!found.is_empty(), "{path}: no schema for {name}");
        let last = index + 1 == parts.len();
        let mut next_targets = Vec::new();
        for target in targets {
            let object = target.as_object_mut().expect("object on the path");
            if last && !each {
                object.insert(name.into(), sample(found[0], root));
                continue;
            }
            let child = object.entry(name.to_owned()).or_insert_with(|| {
                if each {
                    json!([sample(&resolve(found[0], root)["items"], root)])
                } else {
                    sample(found[0], root)
                }
            });
            if each {
                next_targets.extend(child.as_array_mut().expect("array").iter_mut());
            } else {
                next_targets.push(child);
            }
        }
        targets = next_targets;
        schemas = found
            .iter()
            .map(|schema| {
                let schema = resolve(schema, root);
                if each { &schema["items"] } else { schema }
            })
            .collect();
    }
}

fn holds(row: &Value, path: &str) -> bool {
    let mut values = vec![row];
    for part in path.split('.') {
        let (name, each) = part
            .strip_suffix("[]")
            .map_or((part, false), |name| (name, true));
        values = values
            .into_iter()
            .filter_map(|value| value.get(name))
            .flat_map(|value| {
                if each {
                    value.as_array().into_iter().flatten().collect::<Vec<_>>()
                } else {
                    vec![value]
                }
            })
            .collect();
    }
    !values.is_empty()
}

fn every_kind() -> Value {
    json!([
        "hints",
        "details",
        "stats",
        "receipts",
        "echo",
        "diagnostics"
    ])
}

#[test]
fn default_and_all_extras_fixture_pairs_validate_for_every_tool() {
    for tool in ToolId::ALL
        .into_iter()
        .filter(|tool| *tool != ToolId::Clasify)
    {
        let contract = crate::contracts::tool_contract(tool).expect("contract");
        let root = &contract["outputSchema"];
        let mut extras_row = json!({"index":0,"data":base_data(tool)});
        for path in all_paths(tool) {
            let rest = path.strip_prefix("results[].").expect("row path");
            inject(&mut extras_row, rest, root);
        }
        let mut default_row = extras_row.clone();
        prune_row(&mut extras_row, tool, &json!({"extras": every_kind()}));
        prune_row(&mut default_row, tool, &json!({}));
        for path in all_paths(tool) {
            let rest = path.strip_prefix("results[].").expect("row path");
            assert!(
                holds(&extras_row, rest),
                "{}: extras keeps {path}",
                tool.as_str()
            );
            assert!(
                !holds(&default_row, rest),
                "{}: default drops {path}",
                tool.as_str()
            );
        }
        for (label, row) in [("extras", extras_row), ("default", default_row)] {
            crate::contracts::validate_output(tool.as_str(), &json!({"results":[row]}))
                .unwrap_or_else(|error| panic!("{} {label}: {error:?}", tool.as_str()));
        }
    }
}

/// Each kind keeps only its own paths: asking `stats` leaves receipts
/// and diagnostics out.
#[test]
fn a_kind_keeps_only_its_own_paths() {
    let mut row = json!({"index":0,"cache":1,"data":{
        "path":"a.rs","content":"x","returnedLines":1,"lastModified":"t","httpStatus":500
    }});
    prune_row(
        &mut row,
        ToolId::GhGetFileContent,
        &json!({"extras":["stats"]}),
    );
    assert_eq!(row["data"]["returnedLines"], 1, "{row}");
    assert!(row.get("cache").is_none(), "{row}");
    assert!(row["data"].get("lastModified").is_none(), "{row}");
    assert!(row["data"].get("httpStatus").is_none(), "{row}");
}

/// A block emptied by pruning (a summary of counts) goes
/// with its fields; a block that keeps a field stays.
#[test]
fn a_block_of_only_extra_fields_goes_with_them() {
    let mut row = json!({"index":0,"data":{"summary":{"x":1},"kept":{"a":1}}});
    super::remove(&mut row, &super::segments("data.summary.x"));
    assert_eq!(row, json!({"index":0,"data":{"kept":{"a":1}}}));
    let mut row = json!({"index":0,"data":{"summary":{"x":1,"y":2}}});
    super::remove(&mut row, &super::segments("data.summary.x"));
    assert_eq!(row, json!({"index":0,"data":{"summary":{"y":2}}}));
}

#[test]
fn pruning_never_touches_continuations() {
    for tool in ToolId::ALL {
        for path in all_paths(tool) {
            assert!(
                !path
                    .split(['.', '['])
                    .any(|part| part == "next" || part == "hints"),
                "{}: {path}",
                tool.as_str()
            );
        }
    }
    let page = json!({"tool":"lspSearch","query":{"queries":[{"path":"/r/a.ts","operation":"references","symbolName":"a","line":1,"page":2}]}});
    let lead =
        json!({"tool":"localFetch","query":{"queries":[{"path":"/r/a.ts","ranges":["1-2"]}]}});
    let mut row = json!({"index":0,"meta":{"evidence":{"kind":"semantic","confidence":"high"}},"cache":1,"data":{
        "kind":"empty","category":"noResults",
        "workspaceRoot":"/r",
        "lsp":{"receipt":{"command":"tsserver"}},
        "next":{"nextPage":page},
        "hints":{"readDefinition":lead}
    }});
    prune_row(&mut row, ToolId::LspSearch, &json!({}));
    assert!(
        row.get("meta").is_none() && row.get("cache").is_none(),
        "{row}"
    );
    assert!(row["data"].get("workspaceRoot").is_none(), "{row}");
    assert!(
        row["data"].get("lsp").is_none(),
        "an emptied block goes: {row}"
    );
    assert_eq!(row["data"]["next"]["nextPage"], page);
    assert_eq!(row["data"]["hints"]["readDefinition"], lead);
    let mut envelope = json!({"results":[row]});
    crate::response::channels::publish_hints(&mut envelope, ToolId::LspSearch);
    crate::contracts::validate_output("lspSearch", &envelope)
        .expect("pruned row keeps a valid continuation");
    for continuation in [page, lead] {
        let tool = continuation["tool"].as_str().expect("tool");
        crate::contracts::prepare_and_validate(tool, continuation["query"].clone())
            .unwrap_or_else(|error| panic!("{tool}: {error:?}"));
    }
}

/// Output fields gated on `extras` belong in core's extra classes, not in
/// a tool branch. Each listed module keeps a branch that does real work only
/// for a kind (a provider call) or is a value-conditional dedupe; the
/// counts may only shrink. A module's allowance covers its file and its
/// submodules, so splitting a module into files moves no allowance.
#[test]
fn no_new_ad_hoc_extras_output_branches() {
    const ALLOWED: &[(&str, usize)] = &[
        // Shared value-conditional dedupe pass (echo keeps request
        // echoes; stats keeps the whole statistics block the default
        // trims), and the git HEAD read made only for receipts.
        ("response/rows", 3),
        // Details list every path behind a grouped skip entry.
        ("tools/leads/skipped", 1),
        // The language-server receipt is built only for receipts.
        ("tools/lsp_search", 1),
        // Snippet `matches` go only when resolved `lines` replace them.
        ("tools/gh_search_code/code_output", 1),
        // A provider call made only for receipts (last-modified lookup).
        ("tools/gh_get_file_content", 1),
        // Provider calls made only for details: a tree page's
        // last-commit dates (GraphQL), and the branches listing of refs.
        ("tools/gh_structure", 1),
        ("tools/gh_structure/refs", 1),
        // Extras-gated output not yet moved into core's extra classes.
        ("tools/artifact_search", 1),
        // Line counts and modified times: reading them costs I/O per entry.
        ("tools/structure_search/listing", 1),
        ("tools/gh_get_history_item/patch_hop", 1),
        ("tools/gh_get_history_item/pr_finish", 3),
        ("tools/gh_get_history_item/pull_request", 1),
    ];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read src").flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Test modules split into `tests/` or `*_tests/` directories are not production.
                if path.file_name().is_some_and(|name| {
                    name != "tests" && !name.to_string_lossy().ends_with("_tests")
                }) {
                    stack.push(path);
                }
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "rs")
                || path.file_name().is_some_and(|name| {
                    let name = name.to_string_lossy();
                    name == "tests.rs" || name.ends_with("_tests.rs")
                })
            {
                continue;
            }
            let production = crate::tools::test_support::production_source(&path);
            let count = production
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                // A branch names one kind: `extras::wants(q, ExtraKind::X)`,
                // a typed `query.wants(ExtraKind::X)`, or
                // `query.extras.contains(&ExtraKind::X)`, whose
                // `.contains(` rustfmt may wrap onto its own line.
                // Accessors that forward a `kind` argument are not branches.
                .filter(|line| {
                    line.contains("ExtraKind::")
                        && (line.contains("wants(") || line.contains(".contains(&"))
                })
                .count();
            if count > 0 {
                let relative = path
                    .strip_prefix(&root)
                    .expect("under src")
                    .to_string_lossy()
                    .replace('\\', "/");
                found.push((relative, count));
            }
        }
    }
    // Each file counts against the allowance of the nearest listed module
    // that is the file's own module or one of its ancestors.
    let owner = |file: &str| {
        let module = file.trim_end_matches(".rs").trim_end_matches("/mod");
        ALLOWED
            .iter()
            .filter(|(name, _)| {
                module == *name
                    || module
                        .strip_prefix(*name)
                        .is_some_and(|rest| rest.starts_with('/'))
            })
            .max_by_key(|(name, _)| name.len())
            .map(|(name, _)| *name)
    };
    let mut per_module = std::collections::BTreeMap::<&str, usize>::new();
    for (file, count) in &found {
        let Some(module) = owner(file) else {
            panic!(
                "{file}: {count} extras branch line(s), 0 allowed. Declare the field extra in core (fieldClass) instead."
            );
        };
        *per_module.entry(module).or_default() += count;
    }
    let stale: Vec<&str> = ALLOWED
        .iter()
        .map(|(module, _)| *module)
        .filter(|module| !per_module.contains_key(module))
        .collect();
    assert!(
        stale.is_empty(),
        "no extras branch left, drop the allowance: {stale:?}"
    );
    for (module, count) in &per_module {
        let allowed = ALLOWED
            .iter()
            .find(|(name, _)| name == module)
            .map_or(0, |(_, allowed)| *allowed);
        assert!(
            *count <= allowed,
            "{module}: {count} extras branch line(s), {allowed} allowed. Declare the field extra in core (fieldClass) instead."
        );
    }
}
