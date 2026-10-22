//! The opt-in output stage. A field core classes `extra` belongs to an
//! `extras` kind (stats, receipts, echoes, diagnostics, secondary details);
//! it is not evidence, not next-call input, and not a disclosure. Default
//! rows drop it; a row whose `extras` names its kind keeps it. The paths are
//! the contract's `extraPaths` ([`ToolId::extra_paths`]), so emitters build
//! these fields unconditionally and branch on a kind only for real work.
use crate::id::ToolId;
use octocode_contract_types::ExtraKind;
use serde_json::{Map, Value};

/// Whether a raw query's `extras` names `kind`.
#[must_use]
pub(crate) fn wants(query: &Value, kind: ExtraKind) -> bool {
    query
        .get("extras")
        .and_then(Value::as_array)
        .is_some_and(|kinds| kinds.iter().any(|value| names(value, kind)))
}

/// Whether one `extras` entry is `kind`'s contract name.
pub(crate) fn names(value: &Value, kind: ExtraKind) -> bool {
    value
        .as_str()
        .and_then(|name| name.parse::<ExtraKind>().ok())
        == Some(kind)
}

/// [`wants`] for a contract `extraPaths` kind name.
fn wants_kind(query: &Value, kind: &str) -> bool {
    kind.parse::<ExtraKind>()
        .is_ok_and(|kind| wants(query, kind))
}

/// Every extra path of `tool`, whatever its kind.
#[cfg(test)]
pub(crate) fn all_paths(tool: ToolId) -> Vec<&'static str> {
    tool.extra_paths()
        .iter()
        .flat_map(|(_, paths)| paths.iter().copied())
        .collect()
}

/// Whether [`prune_row`] leaves the extra at `path` on a row of `query`:
/// its kind is wanted, its key is picked, the rows are sorted by it, or no
/// kind of `tool` lists it.
#[must_use]
pub fn keeps(tool: ToolId, query: &Value, path: &str) -> bool {
    tool.extra_paths()
        .iter()
        .filter(|(_, paths)| paths.contains(&path))
        .all(|(kind, _)| {
            wants_kind(query, kind) || picked(query, path) || sorted_by(tool, query, path)
        })
}

/// Drops `tool`'s extra fields from one result row unless its query names
/// their kind. Error rows are pruned too: an extra field explains the call,
/// not the failure.
/// A key a query's `fields` pick names is evidence the caller asked for,
/// and the field the query's `sort` orders rows by is what the order
/// shows (contract `sortKeyPaths`), so both stay whatever their kind.
pub(crate) fn prune_row(row: &mut Value, tool: ToolId, query: &Value) {
    for (kind, paths) in tool.extra_paths() {
        if wants_kind(query, kind) {
            continue;
        }
        for path in *paths {
            if picked(query, path) || sorted_by(tool, query, path) {
                continue;
            }
            if let Some(rest) = path.strip_prefix("results[].") {
                remove(row, &segments(rest));
            }
        }
    }
}

/// Whether the query's `sort` orders `tool`'s rows by the extra at `path`.
fn sorted_by(tool: ToolId, query: &Value, path: &str) -> bool {
    let Some(sort) = query.get("sort").and_then(Value::as_str) else {
        return false;
    };
    tool.sort_key_paths()
        .iter()
        .any(|(value, paths)| *value == sort && paths.contains(&path))
}

/// Whether `path` ends in a key the query's `fields` pick names.
fn picked(query: &Value, path: &str) -> bool {
    let key = path.rsplit('.').next().unwrap_or(path);
    query
        .get("fields")
        .and_then(Value::as_array)
        .is_some_and(|fields| fields.iter().any(|field| field.as_str() == Some(key)))
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
    /// a tool branch. Each listed file keeps a branch that does real work only
    /// for a kind (a provider call) or is a value-conditional dedupe; the
    /// counts may only shrink.
    #[test]
    fn no_new_ad_hoc_extras_output_branches() {
        const ALLOWED: &[(&str, usize)] = &[
            // Shared value-conditional dedupe pass (echo keeps request
            // echoes; stats keeps the whole statistics block the default
            // trims), and the git HEAD read made only for receipts.
            ("response/rows.rs", 3),
            // Details list every path behind a grouped skip entry.
            ("tools/local_search/leads.rs", 1),
            // The language-server receipt is built only for receipts.
            ("tools/lsp_search/mod.rs", 1),
            // Snippet `matches` go only when resolved `lines` replace them.
            ("tools/gh_search_code/code_output.rs", 1),
            // A provider call made only for receipts (last-modified lookup).
            ("tools/gh_get_file_content/mod.rs", 1),
            // Provider calls made only for details: a tree page's
            // last-commit dates (GraphQL), and the branches listing of refs.
            ("tools/gh_structure/mod.rs", 1),
            ("tools/gh_structure/refs.rs", 1),
            // Extras-gated output not yet moved into core's extra classes.
            ("tools/artifact_search/mod.rs", 1),
            // Line counts and modified times: reading them costs I/O per entry.
            ("tools/structure_search/listing.rs", 1),
            ("tools/gh_get_history_item/patch_hop.rs", 1),
            ("tools/gh_get_history_item/pr_finish.rs", 3),
            ("tools/gh_get_history_item/pull_request.rs", 1),
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
        let stale: Vec<&str> = ALLOWED
            .iter()
            .map(|(file, _)| *file)
            .filter(|file| !found.iter().any(|(name, _)| name == file))
            .collect();
        assert!(
            stale.is_empty(),
            "no extras branch left, drop the allowance: {stale:?}"
        );
        for (file, count) in &found {
            let allowed = ALLOWED
                .iter()
                .find(|(name, _)| name == file)
                .map_or(0, |(_, allowed)| *allowed);
            assert!(
                *count <= allowed,
                "{file}: {count} extras branch line(s), {allowed} allowed. Declare the field extra in core (fieldClass) instead."
            );
        }
    }
}

#[cfg(test)]
mod sorted_tests {
    use super::*;
    use serde_json::json;

    /// Rows sorted by an extra field show it by default (L6-8): the
    /// comment count of a `comments:desc` search, the forks and update date
    /// of a repository search sorted by them. Another sort drops them.
    #[test]
    fn the_field_rows_are_sorted_by_survives_pruning() {
        let issue = || json!({"data":{"issues":[{"number":1,"commentsCount":40}]}});
        let mut row = issue();
        prune_row(
            &mut row,
            ToolId::GhSearchHistory,
            &json!({"sort":"comments:desc"}),
        );
        assert_eq!(row["data"]["issues"][0]["commentsCount"], 40, "{row}");
        let mut row = issue();
        prune_row(
            &mut row,
            ToolId::GhSearchHistory,
            &json!({"sort":"created:desc"}),
        );
        assert!(
            row["data"]["issues"][0].get("commentsCount").is_none(),
            "{row}"
        );

        let repo = || json!({"data":{"repositories":[{"owner":"o","repo":"r","forks":9,"updatedAt":"t"}]}});
        let mut row = repo();
        prune_row(
            &mut row,
            ToolId::GhSearchRepo,
            &json!({"sort":"forks:desc"}),
        );
        assert_eq!(row["data"]["repositories"][0]["forks"], 9, "{row}");
        assert!(
            row["data"]["repositories"][0].get("updatedAt").is_none(),
            "{row}"
        );
        let mut row = repo();
        prune_row(
            &mut row,
            ToolId::GhSearchRepo,
            &json!({"sort":"updated:desc"}),
        );
        assert_eq!(row["data"]["repositories"][0]["updatedAt"], "t", "{row}");
        assert!(keeps(
            ToolId::GhSearchRepo,
            &json!({"sort":"updated:desc"}),
            "results[].data.repositories[].updatedAt"
        ));
    }
}

#[cfg(test)]
mod picked_tests {
    use super::*;
    use serde_json::json;

    /// `fields:["license"]` keeps the details-class `license` a default row
    /// drops: the pick is the caller's request, not an opt-in extra.
    #[test]
    fn a_picked_extra_key_survives_pruning() {
        let path = ToolId::GhSearchRepo
            .extra_paths()
            .iter()
            .flat_map(|(_, paths)| paths.iter().copied())
            .find(|path| path.ends_with(".license"))
            .expect("license is an extra path");
        let picked_query = json!({"keywords":["x"],"fields":["license"]});
        assert!(picked(&picked_query, path));
        assert!(!picked(&json!({"keywords":["x"]}), path));
        let mut row = json!({"data":{"repositories":[{"owner":"o","repo":"r","license":"mit"}]}});
        prune_row(&mut row, ToolId::GhSearchRepo, &picked_query);
        assert_eq!(row["data"]["repositories"][0]["license"], "mit", "{row}");
        prune_row(&mut row, ToolId::GhSearchRepo, &json!({"keywords":["x"]}));
        assert!(
            row["data"]["repositories"][0].get("license").is_none(),
            "{row}"
        );
    }
}
