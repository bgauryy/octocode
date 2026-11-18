//! The published MCP input (core's `publishedInputSchema`, embedded through
//! the presentation) and native admission agree on every key: no field native
//! admits on a fresh call is hidden from agents, and no published field is one
//! native rejects. Private progress (`x-cursor`) is neither published nor
//! admitted outside an issued cursor.
use crate::contracts::{cli_presentation, state_fields, tool_contract};
use crate::id::ToolId;
use octocode_test_support::json_fixture;
use serde_json::Value;
use std::collections::BTreeSet;

/// Nested levels compared below the envelope: deep enough for every field a
/// caller writes, and a bound on recursive schemas (AST rules) on both sides.
const DEPTH: usize = 5;

/// Every key path `schema` admits (`queries[].contextLines.before`,
/// `queries[].rule.*`), skipping fields a form forbids (`not: {}`) and, when
/// `private` is set, private progress.
fn key_paths(root: &Value, schema: &Value, at: &str, depth: usize, out: &mut BTreeSet<String>) {
    if depth > DEPTH {
        return;
    }
    let resolved = crate::contracts::resolve_ref(schema, &root["$defs"]);
    if !std::ptr::eq(schema, resolved) {
        key_paths(root, resolved, at, depth, out);
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        for branch in schema[keyword].as_array().into_iter().flatten() {
            key_paths(root, branch, at, depth, out);
        }
    }
    for (name, field) in schema["properties"].as_object().into_iter().flatten() {
        let forbidden = field["not"]
            .as_object()
            .is_some_and(serde_json::Map::is_empty);
        if forbidden
            || crate::contracts::schema_facts::schema_keyword(field, &root["$defs"], "x-cursor")
                == Some(&Value::Bool(true))
        {
            continue;
        }
        let path = if at.is_empty() {
            name.clone()
        } else {
            format!("{at}.{name}")
        };
        out.insert(path.clone());
        key_paths(root, field, &path, depth + 1, out);
    }
    if let Some(items) = schema.get("items").filter(|items| items.is_object()) {
        key_paths(root, items, &format!("{at}[]"), depth, out);
    }
    if let Some(values) = schema
        .get("additionalProperties")
        .filter(|values| values.is_object())
    {
        let path = format!("{at}.*");
        out.insert(path.clone());
        key_paths(root, values, &path, depth + 1, out);
    }
}

fn paths_of(schema: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    key_paths(schema, schema, "", 0, &mut out);
    out
}

/// The `x-cursor` fields still present after the walk skipped them: none
/// may be published.
fn mentions_private_state(schema: &Value) -> bool {
    match schema {
        Value::Object(map) => {
            map.contains_key("x-cursor") || map.values().any(mentions_private_state)
        }
        Value::Array(items) => items.iter().any(mentions_private_state),
        _ => false,
    }
}

fn presentation_tool(name: &str) -> &'static Value {
    cli_presentation().expect("embedded presentation")["tools"]
        .as_array()
        .expect("presentation tools")
        .iter()
        .find(|tool| tool["name"] == name)
        .unwrap_or_else(|| panic!("{name} has a presentation"))
}

#[test]
fn every_tool_publishes_exactly_the_keys_native_admits() {
    let mut failures = Vec::new();
    for tool in ToolId::ALL {
        let name = tool.as_str();
        let admitted = paths_of(&tool_contract(tool).expect("contract")["inputSchema"]);
        let presented = presentation_tool(name);
        let full = &presented.get("selection").unwrap_or(presented)["mcp"]["inputSchema"];
        let plain = &presented["mcp"]["inputSchema"];
        let published = paths_of(full);
        let hidden: Vec<_> = admitted.difference(&published).collect();
        let rejected: Vec<_> = published.difference(&admitted).collect();
        if !hidden.is_empty() {
            failures.push(format!("{name}: admitted but not published: {hidden:?}"));
        }
        if !rejected.is_empty() {
            failures.push(format!("{name}: published but not admitted: {rejected:?}"));
        }
        let beyond: Vec<_> = paths_of(plain).difference(&published).cloned().collect();
        if !beyond.is_empty() {
            failures.push(format!("{name}: the plain listing adds {beyond:?}"));
        }
        for schema in [full, plain] {
            if mentions_private_state(schema) {
                failures.push(format!("{name}: a published schema carries x-cursor"));
            }
        }
        for field in state_fields(tool) {
            if published.contains(&format!("queries[].{field}")) {
                failures.push(format!("{name}: private progress {field} is published"));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn help_and_examples_never_show_private_progress() {
    let mut failures = Vec::new();
    for tool in ToolId::ALL {
        let name = tool.as_str();
        let help = crate::contracts::cli_tool_help(name).unwrap_or_default();
        let presented = presentation_tool(name);
        let variants = presented["views"]["variants"]["variants"]
            .as_array()
            .into_iter()
            .flatten();
        let mut shown: BTreeSet<&str> = BTreeSet::new();
        for variant in variants {
            shown.extend(
                variant["requires"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str),
            );
            shown.extend(
                variant["example"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(key, _)| key.as_str()),
            );
        }
        for field in state_fields(tool) {
            if shown.contains(field.as_str()) || help.contains(&format!("\"{field}\":")) {
                failures.push(format!("{name}: help or an example shows private {field}"));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn the_walk_sees_nested_fields_and_skips_private_and_forbidden_ones() {
    let schema = json_fixture(
        r##"{
            "$defs": {"Range": {"properties": {"from": {}, "to": {}}}},
            "anyOf": [
                {
                    "properties": {
                        "queries": {
                            "items": {
                                "properties": {
                                    "range": {"$ref": "#/$defs/Range"},
                                    "page": {"x-cursor": true},
                                    "gone": {"not": {}},
                                    "tags": {"additionalProperties": {"type": "string"}}
                                }
                            }
                        }
                    }
                },
                {"properties": {"cursor": {}}}
            ]
        }"##,
    );
    let paths: Vec<_> = paths_of(&schema).into_iter().collect();
    assert_eq!(
        paths,
        [
            "cursor",
            "queries",
            "queries[].range",
            "queries[].range.from",
            "queries[].range.to",
            "queries[].tags",
            "queries[].tags.*"
        ]
    );
}

/// The object forms a union nests, flattened: a form split by a field of its
/// own (clasify `content` or `source`) is several leaves.
fn leaf_forms<'a>(root: &'a Value, schema: &'a Value, out: &mut Vec<&'a Value>) {
    let schema = crate::contracts::resolve_ref(schema, &root["$defs"]);
    let branches: Vec<&Value> = ["anyOf", "oneOf"]
        .iter()
        .flat_map(|keyword| schema[*keyword].as_array().into_iter().flatten())
        .collect();
    if branches.is_empty() {
        out.push(schema);
    }
    for branch in branches {
        leaf_forms(root, branch, out);
    }
}

/// Whether an object form lets `field` be the string `value`: the field's
/// literals name it, or the field takes any value. A form without the field
/// does not.
fn form_takes(root: &Value, form: &Value, field: &str, value: &str) -> bool {
    let form = crate::contracts::resolve_ref(form, &root["$defs"]);
    let Some(property) = form["properties"].get(field) else {
        return false;
    };
    let property = crate::contracts::resolve_ref(property, &root["$defs"]);
    let literals: Vec<&Value> = property
        .get("const")
        .into_iter()
        .chain(property["enum"].as_array().into_iter().flatten())
        .collect();
    literals.is_empty()
        || literals
            .iter()
            .any(|literal| literal.as_str() == Some(value))
}

/// The `FIELD=VALUE` pairs of a published selection key
/// (`operation=match,view=content`).
fn pairs(selection: &str) -> Vec<(&str, &str)> {
    selection
        .split(',')
        .map(|pair| {
            pair.split_once('=')
                .unwrap_or_else(|| panic!("{selection} is FIELD=VALUE pairs"))
        })
        .collect()
}

/// Every selected view a tool publishes (one operation, or an operation
/// narrowed by a second field's value) publishes exactly the keys the
/// validator admits for the forms that selection names, in the view's mode:
/// the plain views are a runtime's without a classification provider, which
/// refuses [`PROVIDER_FIELDS`](crate::contracts::PROVIDER_FIELDS); the views
/// under `selection` are a runtime's with one.
#[test]
fn every_operation_publishes_exactly_the_keys_its_forms_admit() {
    let mut failures = Vec::new();
    for tool in ToolId::ALL {
        let name = tool.as_str();
        let query = &tool_contract(tool).expect("contract")["querySchema"];
        let mut forms = Vec::new();
        leaf_forms(query, query, &mut forms);
        let presented = presentation_tool(name);
        let modes = [
            ("plain", Some(presented), false),
            ("selection", presented.get("selection"), true),
        ];
        for (mode, presented, provider) in modes {
            let selections = presented.and_then(|presented| presented["selections"].as_object());
            for (selection, view) in selections.into_iter().flatten() {
                let pairs = pairs(selection);
                if pairs.first().map(|(field, _)| *field) != Some("operation") || pairs.len() > 2 {
                    failures.push(format!(
                        "{name} {mode} {selection}: not operation=VALUE[,FIELD=VALUE]"
                    ));
                    continue;
                }
                let takes = |root: &Value, form: &Value| {
                    pairs
                        .iter()
                        .all(|(field, value)| form_takes(root, form, field, value))
                };
                let mut admitted = BTreeSet::new();
                for form in forms.iter().filter(|form| takes(query, form)) {
                    key_paths(query, form, "", 0, &mut admitted);
                }
                if !provider {
                    admitted.retain(|path| {
                        let field = path.split(['.', '[']).next().unwrap_or(path);
                        !crate::contracts::PROVIDER_FIELDS.contains(&field)
                    });
                }
                let schema = &view["querySchema"];
                let published = paths_of(schema);
                if admitted != published {
                    let hidden: Vec<_> = admitted.difference(&published).collect();
                    let rejected: Vec<_> = published.difference(&admitted).collect();
                    failures.push(format!(
                        "{name} {mode} {selection}: admitted only {hidden:?}; published only {rejected:?}"
                    ));
                }
                // The view is the selection's own: each form it shows takes
                // the pairs, and a narrowing pair admits only its value.
                let mut shown = Vec::new();
                leaf_forms(schema, schema, &mut shown);
                if !shown.iter().all(|form| takes(schema, form)) {
                    failures.push(format!(
                        "{name} {mode} {selection}: a published form does not take the selection"
                    ));
                }
                if let Some((field, value)) = pairs.get(1) {
                    let only = shown.iter().all(|form| {
                        let property = crate::contracts::resolve_ref(
                            &form["properties"][*field],
                            &schema["$defs"],
                        );
                        property["const"] == *value
                            || property["enum"] == serde_json::json!([value])
                    });
                    if !only {
                        failures.push(format!(
                            "{name} {mode} {selection}: {field} admits more than {value}"
                        ));
                    }
                }
                if mentions_private_state(schema) {
                    failures.push(format!("{name} {mode} {selection}: carries x-cursor"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// The guard above compares something: operations are published, a provider
/// adds classification views, and second pairs narrow an operation.
#[test]
fn selected_views_cover_both_modes_and_second_pairs() {
    let tools = || {
        ToolId::ALL
            .into_iter()
            .map(|tool| presentation_tool(tool.as_str()))
    };
    let keys =
        |presented: &'static Value| presented["selections"].as_object().into_iter().flatten();
    assert!(tools().flat_map(keys).any(|(key, _)| !key.contains(',')));
    assert!(tools().flat_map(keys).any(|(key, _)| key.contains(',')));
    let provider: Vec<_> = tools()
        .filter_map(|presented| presented.get("selection"))
        .flat_map(keys)
        .collect();
    assert!(provider.iter().any(|(_, view)| {
        view["querySchema"]["properties"]
            .get(crate::contracts::PROVIDER_FIELDS[0])
            .is_some()
    }));
}
