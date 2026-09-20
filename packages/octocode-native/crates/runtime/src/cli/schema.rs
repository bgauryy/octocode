//! CLI-only projections of the core-owned tool contract.
use clap::ValueEnum;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub(super) enum SchemeView {
    #[default]
    Full,
    Query,
}

pub(super) fn project(tool: Value, view: SchemeView) -> Value {
    match view {
        SchemeView::Full => tool,
        // Keep the complete schema subtree: its local refs resolve against its
        // own root, including all core-owned $defs and validation constraints.
        SchemeView::Query => {
            let mut query = json!({"name": tool["name"], "querySchema": tool["querySchema"]});
            if let Some(description) = tool.get("description") {
                query["description"] = description.clone();
            }
            if let Some(queries) = tool.pointer("/inputSchema/properties/queries") {
                let bounds: Map<String, Value> = ["minItems", "maxItems"]
                    .into_iter()
                    .filter_map(|key| {
                        queries
                            .get(key)
                            .map(|value| (key.to_owned(), value.clone()))
                    })
                    .collect();
                if !bounds.is_empty() {
                    query["queryEnvelope"] = json!({"queries": bounds});
                }
            }
            query
        }
    }
}

fn parse_selection(selection: &str) -> Result<(&str, Value), String> {
    let (field, value) = selection
        .split_once('=')
        .filter(|(field, value)| !field.trim().is_empty() && !value.trim().is_empty())
        .ok_or("--select expects FIELD=VALUE, e.g. operation=code")?;
    Ok((
        field,
        serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.into())),
    ))
}

pub(super) fn project_selected(
    tool: Value,
    view: SchemeView,
    selection: Option<&str>,
) -> Result<Value, String> {
    let Some(selection) = selection else {
        return Ok(project(tool, view));
    };
    if view != SchemeView::Query || !tool["name"].is_string() {
        return Err("--select requires --view query and a tool name".into());
    }
    let (field, value) = parse_selection(selection)?;
    let mut projected = project(tool, view);
    let schema = &mut projected["querySchema"];
    let candidates = ["oneOf", "anyOf"]
        .into_iter()
        .flat_map(|union| {
            schema[union]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .filter(|(_, branch)| {
                    branch["properties"]
                        .get(field)
                        .and_then(|property| property.get("const"))
                        == Some(&value)
                })
                .map(move |(index, _)| (union, index))
        })
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        return Err(format!(
            "--select {selection:?} matched {} top-level oneOf/anyOf branches; choose a const field/value identifying exactly one branch in --view query.",
            candidates.len()
        ));
    }
    let (union, index) = candidates[0];
    let branches = schema[union]
        .as_array()
        .ok_or("Selected union must be an array")?;
    let selected = branches[index].clone();
    // Removing other oneOf branches must not admit instances that previously
    // matched multiple branches. Const discriminators usually prove disjointness;
    // retain exclusion constraints for siblings whose overlap cannot be ruled out.
    let requires_field = |branch: &Value| {
        branch["required"]
            .as_array()
            .is_some_and(|required| required.iter().any(|name| name.as_str() == Some(field)))
    };
    let overlaps = if union == "oneOf" {
        branches
            .iter()
            .enumerate()
            .filter(|(i, branch)| {
                *i != index
                    && !(branch["properties"]
                        .get(field)
                        .and_then(|property| property.get("const"))
                        .is_some_and(|other| consts_disjoint(other, &value))
                        && (requires_field(&selected) || requires_field(branch)))
            })
            .map(|(_, branch)| branch.clone())
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    schema[union] = json!([selected]);
    if !overlaps.is_empty() {
        let object = schema
            .as_object_mut()
            .ok_or("Query schema must be an object")?;
        object
            .entry("allOf")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("Query schema allOf must be an array")?
            .push(json!({"not": {"anyOf": overlaps}}));
    }
    prune_unreachable_defs(schema);
    Ok(projected)
}

fn consts_disjoint(left: &Value, right: &Value) -> bool {
    // JSON Schema equates numeric spellings such as 1 and 1.0, including in
    // compound values. Only discard siblings when scalar disjointness is certain.
    match (left, right) {
        (Value::String(left), Value::String(right)) => left != right,
        (Value::Bool(left), Value::Bool(right)) => left != right,
        _ => std::mem::discriminant(left) != std::mem::discriminant(right),
    }
}

fn collect_refs(value: &Value, refs: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if matches!(key.as_str(), "$ref" | "$dynamicRef" | "$recursiveRef") {
                    if let Some(reference) = value.as_str() {
                        refs.push(reference.into());
                    }
                } else {
                    collect_refs(value, refs);
                }
            }
        }
        Value::Array(values) => values.iter().for_each(|value| collect_refs(value, refs)),
        _ => {}
    }
}

fn prune_unreachable_defs(schema: &mut Value) {
    let Some(defs) = schema.get("$defs").and_then(Value::as_object).cloned() else {
        return;
    };
    let mut roots = schema.clone();
    if let Some(root) = roots.as_object_mut() {
        root.remove("$defs");
    }
    let mut pending = vec![];
    collect_refs(&roots, &mut pending);
    let mut needed = BTreeSet::new();
    while let Some(reference) = pending.pop() {
        // Anchor/external reference scopes can depend on definitions without a
        // JSON pointer. Keep all definitions when reachability is not provable.
        let Some(pointer) = reference.strip_prefix("#/$defs/") else {
            return;
        };
        let Some(token) = pointer.split('/').next() else {
            return;
        };
        let name = token.replace("~1", "/").replace("~0", "~");
        if !needed.insert(name.clone()) {
            continue;
        }
        let Some(definition) = defs.get(&name) else {
            return;
        };
        collect_refs(definition, &mut pending);
    }
    if needed.is_empty() {
        if let Some(root) = schema.as_object_mut() {
            root.remove("$defs");
        }
    } else if let Some(definitions) = schema.get_mut("$defs").and_then(Value::as_object_mut) {
        definitions.retain(|name, _| needed.contains(name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Args, commands::Command};
    use clap::Parser;

    #[test]
    fn scheme_select_parsing() {
        let args = Args::try_parse_from([
            "octocode",
            "scheme",
            "ghSearch",
            "--view",
            "query",
            "--select",
            "operation=code",
        ])
        .unwrap();
        assert!(
            matches!(args.command, Command::Scheme { select: Some(selection), .. } if selection == "operation=code")
        );
        // --select and --view require a tool name.
        for args in [
            vec!["octocode", "scheme", "--select", "operation=x"],
            vec!["octocode", "scheme", "--view", "query"],
        ] {
            assert!(Args::try_parse_from(args).is_err());
        }
        // --select without --view query parses but fails projection at runtime.
        let args =
            Args::try_parse_from(["octocode", "scheme", "ghSearch", "--select", "operation=x"])
                .unwrap();
        assert!(matches!(
            args.command,
            Command::Scheme {
                view: None,
                select: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn scheme_select_preserves_constraints_and_prunes_only_unreachable_defs() {
        let schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema", "maxProperties": 3,
            "$defs": {"kept": {"$ref": "#/$defs/nested"}, "nested": {"type":"string"}, "unused": {"type":"number"}},
            "oneOf": [
                {"type":"object", "required":["kind","payload"], "additionalProperties":false, "properties":{"kind":{"const":"a"}, "payload":{"$ref":"#/$defs/kept"}}},
                {"type":"object", "required":["kind"], "properties":{"kind":{"const":"b"},"payload":{"$ref":"#/$defs/unused"}}}
            ]
        });
        let tool = json!({"name":"fixture", "querySchema":schema, "inputSchema":{"properties":{"queries":{"minItems":1,"maxItems":5}}}});
        let result = project_selected(tool.clone(), SchemeView::Query, Some("kind=a")).unwrap();
        assert_eq!(result["querySchema"]["oneOf"], json!([schema["oneOf"][0]]));
        assert_eq!(result["querySchema"]["maxProperties"], 3);
        assert_eq!(result["queryEnvelope"]["queries"]["maxItems"], 5);
        assert_eq!(result["querySchema"]["$defs"].as_object().unwrap().len(), 2);
        assert_local_refs_resolve(&result["querySchema"], &result["querySchema"]);
        assert_eq!(
            project_selected(tool.clone(), SchemeView::Full, None).unwrap(),
            tool
        );
        for selection in ["kind=unknown", "missing=a", "bad", "=a", "kind="] {
            assert!(project_selected(tool.clone(), SchemeView::Query, Some(selection)).is_err());
        }
        assert!(project_selected(tool.clone(), SchemeView::Full, Some("kind=a")).is_err());
        let mut ambiguous = tool.clone();
        ambiguous["querySchema"]["oneOf"][1]["properties"]["kind"]["const"] = json!("a");
        assert!(project_selected(ambiguous, SchemeView::Query, Some("kind=a")).is_err());
        let mut overlapping = tool;
        overlapping["querySchema"]["oneOf"][1] = json!({"type":"object"});
        let result = project_selected(overlapping, SchemeView::Query, Some("kind=a")).unwrap();
        assert_eq!(
            result["querySchema"]["allOf"][0]["not"]["anyOf"],
            json!([{"type":"object"}])
        );
    }

    #[test]
    fn scheme_select_anyof_json_values_and_anchor_refs_are_safe() {
        for value in [json!(true), json!(3), json!(null), json!({"nested":1})] {
            let tool = json!({"name":"fixture", "querySchema": {"anyOf":[{"properties":{"kind":{"const":value}}}]}});
            let selection = format!("kind={value}");
            assert!(project_selected(tool, SchemeView::Query, Some(&selection)).is_ok());
        }
        let tool = json!({"name":"fixture", "querySchema": {
            "$defs":{"anchor":{"$anchor":"payload", "type":"string"},"other":{"type":"integer"}},
            "anyOf":[{"properties":{"kind":{"const":"a"},"payload":{"$ref":"#payload"}}}]
        }});
        let result = project_selected(tool.clone(), SchemeView::Query, Some("kind=a")).unwrap();
        assert_eq!(result["querySchema"]["$defs"], tool["querySchema"]["$defs"]);
    }

    #[test]
    fn scheme_select_real_core_keeps_the_exact_branch_and_local_refs() {
        let contract = octocode_native::contracts::parsed_contract().unwrap();
        let tool = contract["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == "ghSearch")
            .unwrap();
        let result =
            project_selected(tool.clone(), SchemeView::Query, Some("operation=code")).unwrap();
        let original = tool["querySchema"]["oneOf"]
            .as_array()
            .unwrap()
            .iter()
            .find(|branch| branch["properties"]["operation"]["const"] == "code")
            .unwrap();
        assert_eq!(result["querySchema"]["oneOf"], json!([original]));
        assert_local_refs_resolve(&result["querySchema"], &result["querySchema"]);
        assert!(
            result.to_string().len() < project(tool.clone(), SchemeView::Query).to_string().len()
        );
        for (key, value) in tool["querySchema"].as_object().unwrap() {
            if !matches!(key.as_str(), "oneOf" | "$defs") {
                assert_eq!(&result["querySchema"][key], value);
            }
        }
    }

    #[test]
    fn scheme_view_parsing() {
        for (flag, expected) in [("full", SchemeView::Full), ("query", SchemeView::Query)] {
            let args =
                Args::try_parse_from(["octocode", "scheme", "ghSearch", "--view", flag]).unwrap();
            assert!(
                matches!(args.command, Command::Scheme { view: Some(view), .. } if view == expected)
            );
        }
        for args in [
            vec!["octocode", "scheme", "ghSearch", "--view", "bad"],
            vec!["octocode", "scheme", "ghSearch", "--view"],
            vec!["octocode", "--view", "query"],
        ] {
            assert!(Args::try_parse_from(args).is_err());
        }
        let args = Args::try_parse_from(["octocode", "scheme"]).unwrap();
        assert!(matches!(
            args.command,
            Command::Scheme {
                tool: None,
                view: None,
                ..
            }
        ));
    }

    fn assert_local_refs_resolve(value: &Value, schema: &Value) {
        match value {
            Value::Object(object) => {
                if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                    let pointer = reference
                        .strip_prefix('#')
                        .expect("self-contained local reference");
                    assert!(schema.pointer(pointer).is_some(), "unresolved {reference}");
                }
                for value in object.values() {
                    assert_local_refs_resolve(value, schema);
                }
            }
            Value::Array(values) => {
                for value in values {
                    assert_local_refs_resolve(value, schema);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn scheme_view_bulk_bounds_match_runtime_admission() {
        let contract = octocode_native::contracts::parsed_contract().unwrap();
        for tool in contract["tools"].as_array().unwrap() {
            let query = project(tool.clone(), SchemeView::Query);
            assert_eq!(query["description"], tool["description"]);
            assert!(query.get("outputSchema").is_none());
            for bound in ["minItems", "maxItems"] {
                assert_eq!(
                    query["queryEnvelope"]["queries"][bound],
                    tool["inputSchema"]["properties"]["queries"][bound],
                    "{} must expose {bound}",
                    tool["name"]
                );
            }
        }
        let tool = contract["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == "localFetch")
            .unwrap();
        let query = project(tool.clone(), SchemeView::Query);
        let limits = &query["queryEnvelope"]["queries"];
        let minimum = limits["minItems"].as_u64().unwrap() as usize;
        let maximum = limits["maxItems"].as_u64().unwrap() as usize;
        let submit = |count: usize| {
            octocode_native::contracts::validate(
                "localFetch",
                json!({"queries": vec![json!({
                    "path": "/tmp/schema-admission.txt", "reasoning": "Check envelope admission."
                }); count]}),
            )
        };
        assert!(submit(minimum).is_ok());
        assert!(submit(maximum).is_ok());
        assert!(submit(minimum - 1).is_err());
        assert!(submit(maximum + 1).is_err());
    }

    #[test]
    fn scheme_view_preserves_real_core_schema_and_references() {
        let contract = octocode_native::contracts::parsed_contract().unwrap();
        for tool in contract["tools"].as_array().unwrap() {
            assert_eq!(project(tool.clone(), SchemeView::Full), *tool);
            let query = project(tool.clone(), SchemeView::Query);
            assert_eq!(query.as_object().unwrap().len(), 4);
            assert_eq!(query["name"], tool["name"]);
            assert_eq!(query["querySchema"], tool["querySchema"]);
            assert_local_refs_resolve(&query["querySchema"], &query["querySchema"]);
        }
        let search = contract["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == "astRewrite")
            .unwrap();
        assert!(search["querySchema"]["$defs"].is_object());
        assert!(search["querySchema"]["oneOf"].is_array());
    }
}
