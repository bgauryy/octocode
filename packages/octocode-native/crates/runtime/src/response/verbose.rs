//! The verbose-field stage. A field core classes `verbose` explains how an
//! answer was produced (scan stats, receipts, echoes, diagnostics); it is
//! not evidence, not next-call input, and not a disclosure. Default rows
//! drop it; a row that asked for `debug: true` keeps it. The paths are the
//! contract's `verbosePaths` ([`ToolId::verbose_paths`]), so emitters build
//! these fields unconditionally and never branch on `debug` for them.
use crate::tools::id::ToolId;
use serde_json::{Map, Value};

/// Drops `tool`'s verbose fields from one result row unless its query asked
/// for `debug: true`. Error rows are pruned too: a verbose field explains
/// the call, not the failure.
pub fn prune_row(row: &mut Value, tool: ToolId, query: &Value) {
    if query.get("debug").and_then(Value::as_bool) == Some(true) {
        return;
    }
    for path in tool.verbose_paths() {
        if let Some(rest) = path.strip_prefix("results[].") {
            remove(row, &segments(rest));
        }
    }
}

/// `a.b[].c` → `[("a", false), ("b", true), ("c", false)]`: each field name
/// and whether the path continues into every element of its array.
fn segments(path: &str) -> Vec<(&str, bool)> {
    path.split('.')
        .map(|segment| match segment.strip_suffix("[]") {
            Some(name) => (name, true),
            None => (segment, false),
        })
        .collect()
}

fn remove(value: &mut Value, path: &[(&str, bool)]) {
    let Some((&(name, each), rest)) = path.split_first() else {
        return;
    };
    let Some(object) = value.as_object_mut() else {
        return;
    };
    if rest.is_empty() && !each {
        object.shift_remove(name);
        return;
    }
    match (each, object.get_mut(name)) {
        (true, Some(Value::Array(items))) => {
            for item in items {
                remove(item, rest);
            }
        }
        (false, Some(child)) => {
            let held = child.as_object().is_some_and(|fields| !fields.is_empty());
            remove(child, rest);
            // A block that held only verbose fields says nothing once they
            // go; the row's `data` itself always stays.
            if held && name != "data" && child.as_object().is_some_and(Map::is_empty) {
                object.shift_remove(name);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::prune_row;
    use crate::tools::id::ToolId;
    use serde_json::{Map, Value, json};

    /// A minimal valid data row per tool whose arrays hold one element, so
    /// every verbose path has somewhere to land.
    fn base_data(tool: ToolId) -> Value {
        match tool {
            ToolId::GhSearchRepo => json!({"repositories":[{"owner":"o","repo":"r"}]}),
            ToolId::GhSearchCode => {
                json!({"files":[{"path":"a.rs","matches":[{"value":"x"}]}]})
            }
            ToolId::GhStructure => json!({"entries":[]}),
            ToolId::GhGetFileContent => json!({"path":"a.rs","content":"x"}),
            ToolId::GhSearchHistory => json!({"commits":[]}),
            ToolId::GhGetHistoryItem => json!({"sha":"abc"}),
            ToolId::ArtifactSearch => json!({"artifacts":[{"name":"x"}]}),
            ToolId::GhCloneRepo => json!({"error":"x"}),
            ToolId::LocalSearch => json!({
                "stats":{"matchCount":1},
                "files":[{"path":"a.rs","matches":[{"value":"x"}]}]
            }),
            ToolId::StructureSearch => json!({"files":[]}),
            ToolId::AstSearch => json!({
                "files":[{"path":"a.rs","matches":[{"value":"x"}]}],
                "nodes":["1 k 1:1-1:2"]
            }),
            ToolId::AstTopology => json!({"results":[{"file":"a.rs"}],"summary":{},"coverage":{}}),
            ToolId::AstRewrite => json!({"matches":[]}),
            ToolId::LocalFetch => json!({"path":"a.rs","content":"x"}),
            ToolId::LspSearch => {
                json!({"payload":{"kind":"references","files":[{"path":"a.ts","matches":[{"line":1}]}]},"lsp":{}})
            }
            ToolId::Clasify => Value::Null,
        }
    }

    fn resolve<'a>(schema: &'a Value, root: &'a Value) -> &'a Value {
        crate::contracts::resolve_ref(schema, &root["$defs"])
    }

    /// Every schema a field name may resolve to below `schema`.
    fn property<'a>(schema: &'a Value, root: &'a Value, name: &str, out: &mut Vec<&'a Value>) {
        let schema = resolve(schema, root);
        if let Some(found) = schema["properties"].get(name) {
            out.push(found);
        }
        for key in ["anyOf", "oneOf", "allOf"] {
            for branch in schema[key].as_array().into_iter().flatten() {
                property(branch, root, name, out);
            }
        }
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
        let rows = &root["properties"]["results"]["items"];
        let mut schemas = vec![rows];
        let mut targets = vec![&mut *row];
        let parts = path.split('.').collect::<Vec<_>>();
        for (index, part) in parts.iter().enumerate() {
            let (name, each) = part
                .strip_suffix("[]")
                .map_or((*part, false), |name| (name, true));
            let mut found = Vec::new();
            for schema in &schemas {
                property(schema, root, name, &mut found);
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
                let child = object
                    .entry(name.to_owned())
                    .or_insert_with(|| if each { json!([{}]) } else { json!({}) });
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

    #[test]
    fn debug_false_and_true_fixture_pairs_validate_for_every_tool() {
        for tool in ToolId::ALL
            .into_iter()
            .filter(|tool| *tool != ToolId::Clasify)
        {
            let contract = crate::contracts::tool_contract(tool).expect("contract");
            let root = &contract["outputSchema"];
            let mut debug_row = json!({"index":0,"data":base_data(tool)});
            for path in tool.verbose_paths() {
                let rest = path.strip_prefix("results[].").expect("row path");
                inject(&mut debug_row, rest, root);
            }
            let mut default_row = debug_row.clone();
            prune_row(&mut debug_row, tool, &json!({"debug":true}));
            prune_row(&mut default_row, tool, &json!({"debug":false}));
            for path in tool.verbose_paths() {
                let rest = path.strip_prefix("results[].").expect("row path");
                assert!(
                    holds(&debug_row, rest),
                    "{}: debug keeps {path}",
                    tool.as_str()
                );
                assert!(
                    !holds(&default_row, rest),
                    "{}: default drops {path}",
                    tool.as_str()
                );
            }
            for (label, row) in [("debug", debug_row), ("default", default_row)] {
                crate::contracts::validate_output(tool.as_str(), &json!({"results":[row]}))
                    .unwrap_or_else(|error| panic!("{} {label}: {error:?}", tool.as_str()));
            }
        }
    }

    /// A block emptied by pruning (astTopology's summary of counts) goes
    /// with its fields; a block that keeps a field stays.
    #[test]
    fn a_block_of_only_verbose_fields_goes_with_them() {
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
            for path in tool.verbose_paths() {
                assert!(
                    !path
                        .split(['.', '['])
                        .any(|part| part == "next" || part == "hints"),
                    "{}: {path}",
                    tool.as_str()
                );
            }
        }
        let page = json!({"tool":"lspSearch","query":{"queries":[{"path":"/r/a.ts","operation":"references","symbolName":"a","lineHint":1,"page":2,"debug":false}]}});
        let lead =
            json!({"tool":"localFetch","query":{"queries":[{"path":"/r/a.ts","ranges":["1-2"]}]}});
        let mut row = json!({"index":0,"meta":{"evidence":{"kind":"semantic","confidence":"high"}},"cache":1,"data":{
            "payload":{"kind":"empty"},
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
        crate::contracts::validate_output("lspSearch", &json!({"results":[row]}))
            .expect("pruned row keeps a valid continuation");
        for continuation in [page, lead] {
            let tool = continuation["tool"].as_str().expect("tool");
            crate::contracts::prepare_and_validate(tool, continuation["query"].clone())
                .unwrap_or_else(|error| panic!("{tool}: {error:?}"));
        }
    }

    /// Output fields gated on `debug` belong in core's verbose classes, not in
    /// a tool branch. Each listed file keeps a branch that does real work only
    /// under `debug` (a provider call) or is a value-conditional dedupe; the
    /// counts may only shrink.
    #[test]
    fn no_new_ad_hoc_debug_output_branches() {
        const ALLOWED: &[(&str, usize)] = &[
            // The verbose stage itself.
            ("response/verbose.rs", 1),
            // Shared value-conditional dedupe passes ("debug keeps everything").
            ("response/rows.rs", 1),
            // A healthy language-server receipt is dropped by value; the flag
            // is read once per row.
            ("tools/lsp_search/receipt.rs", 1),
            ("tools/lsp_search/mod.rs", 1),
            // Snippet `matches` go only when resolved `lines` replace them.
            ("tools/gh_search_code/code_output.rs", 1),
            // A provider call made only under debug (last-modified lookup).
            ("tools/gh_get_file_content/mod.rs", 1),
            // A replayed lead row drops its `debug:false` echo; no output branch.
            ("tools/clasify/context.rs", 1),
            // The `debug` accessor of each artifact query form.
            ("providers/artifact/types.rs", 4),
            // Debug-gated output not yet moved into core's verbose classes.
            ("tools/ast_rewrite/output.rs", 8),
            ("tools/artifact_search/mod.rs", 1),
            ("tools/gh_get_history_item/patch_hop.rs", 2),
            ("tools/gh_get_history_item/pr_sections.rs", 1),
            ("tools/gh_get_history_item/pull_request.rs", 3),
            // Debug receipts that an early `#[cfg(test)]` item used to hide from
            // this scan; the split exposed them, the count is unchanged.
            ("tools/clasify/run/mod.rs", 2),
            ("tools/clasify/run/render.rs", 2),
        ];
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read src").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
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
                let text = std::fs::read_to_string(&path).expect("read source");
                // Only an inline test module ends production code; a lone
                // `#[cfg(test)]` item must not hide the rest of the file.
                let production = text
                    .match_indices("#[cfg(test)]")
                    .find(|(at, marker)| text[at + marker.len()..].trim_start().starts_with("mod "))
                    .map_or(text.as_str(), |(at, _)| &text[..at]);
                let count = production
                    .lines()
                    .filter(|line| !line.trim_start().starts_with("//"))
                    .filter(|line| {
                        line.contains("query.debug")
                            || line.contains("q.debug")
                            || line.contains(".debug()")
                            || line.contains("get(\"debug\")")
                            || line.contains("if debug")
                            || line.contains("!debug")
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
        for (file, count) in &found {
            let allowed = ALLOWED
                .iter()
                .find(|(name, _)| name == file)
                .map_or(0, |(_, allowed)| *allowed);
            assert!(
                *count <= allowed,
                "{file}: {count} debug branch line(s), {allowed} allowed. Declare the field verbose in core (fieldClass) instead."
            );
        }
    }
}
