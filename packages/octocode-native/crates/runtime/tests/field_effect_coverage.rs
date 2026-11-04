//! `field-effect-coverage.json` stays aligned with the published query schema.
//!
//! A field path is spelled as in `skills-dev/octocode-dev/scripts/tool-inventory.mjs`:
//! `.name` for an object property, `[]` for array items and `{}` for
//! `additionalProperties`, so `ranges[].startLine` and `resources{}.query.ranges[].endLine`.
//! A path with no entry inherits the effects of its nearest declared ancestor;
//! a top-level path falls back to the file's `common` fields. Private
//! `x-cursor` state fields are not published query fields and are skipped.
use serde_json::Value;
use std::collections::BTreeSet;

const CONTRACT: &str = include_str!("../../../../octocode-config/contract/tool-contract.json");
const COVERAGE: &str = include_str!("../src/contracts/field-effect-coverage.json");

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("valid JSON")
}

/// `schema` with `$ref` chains followed through the contract's `$defs`.
fn deref<'a>(contract: &'a Value, mut schema: &'a Value) -> &'a Value {
    let mut hops = 0;
    while let Some(reference) = schema["$ref"].as_str() {
        let pointer = reference.strip_prefix('#').expect("local $ref");
        schema = contract
            .pointer(pointer)
            .unwrap_or_else(|| panic!("$ref {reference} resolves"));
        hops += 1;
        assert!(hops < 64, "$ref chain does not end: {reference}");
    }
    schema
}

/// Every published property path under `schema`, appended to `out`.
fn walk(
    contract: &Value,
    schema: &Value,
    prefix: &str,
    active: &mut Vec<*const Value>,
    out: &mut BTreeSet<String>,
) {
    let schema = deref(contract, schema);
    let id: *const Value = schema;
    if active.contains(&id) {
        return;
    }
    active.push(id);
    if let Some(properties) = schema["properties"].as_object() {
        for (name, child) in properties {
            if deref(contract, child)["x-cursor"] == true {
                continue;
            }
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}.{name}")
            };
            out.insert(path.clone());
            walk(contract, child, &path, active, out);
        }
    }
    if schema.get("items").is_some() {
        walk(
            contract,
            &schema["items"],
            &format!("{prefix}[]"),
            active,
            out,
        );
    }
    if schema["additionalProperties"].is_object() {
        walk(
            contract,
            &schema["additionalProperties"],
            &format!("{prefix}{{}}"),
            active,
            out,
        );
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        for branch in schema[keyword].as_array().into_iter().flatten() {
            walk(contract, branch, prefix, active, out);
        }
    }
    active.pop();
}

/// `path` without its last segment (`.name`, `[]` or `{}`); empty at the top.
fn parent(path: &str) -> &str {
    if let Some(rest) = path.strip_suffix("[]").or_else(|| path.strip_suffix("{}")) {
        return rest;
    }
    path.rfind(['.', '[', '{']).map_or("", |at| &path[..at])
}

/// The effects `path` resolves to: its own entry, else its nearest ancestor's,
/// else the file's `common` entry for the top-level name.
fn resolve<'a>(fields: &'a Value, common: &'a Value, path: &str) -> Option<&'a Value> {
    let mut at = path;
    while !at.is_empty() {
        if let Some(effects) = fields.get(at) {
            return Some(effects);
        }
        at = parent(at);
    }
    let top = path.split(['.', '[', '{']).next().unwrap_or(path);
    common.get(top)
}

fn published_paths(contract: &Value, tool: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    walk(
        contract,
        &tool["querySchema"],
        "",
        &mut Vec::new(),
        &mut out,
    );
    out
}

fn tool_name(tool: &Value) -> &str {
    tool["name"].as_str().expect("tool name")
}

fn declared<'a>(coverage: &'a Value, name: &str) -> &'a Value {
    coverage["tools"]
        .get(name)
        .unwrap_or_else(|| panic!("{name} has no entry in field-effect-coverage.json"))
}

#[test]
fn every_published_query_path_resolves_to_an_effect() {
    let (contract, coverage) = (json(CONTRACT), json(COVERAGE));
    let common = &coverage["common"]["fields"];
    let mut unresolved = Vec::new();
    for tool in contract["tools"].as_array().expect("tools") {
        let name = tool_name(tool);
        let fields = &declared(&coverage, name)["fields"];
        for path in published_paths(&contract, tool) {
            if resolve(fields, common, &path).is_none() {
                unresolved.push(format!("{name}: {path}"));
            }
        }
    }
    assert!(
        unresolved.is_empty(),
        "declare these fields in field-effect-coverage.json, or add an ancestor whose effects they inherit:\n{}",
        unresolved.join("\n")
    );
}

#[test]
fn every_declared_key_names_a_published_query_path() {
    let (contract, coverage) = (json(CONTRACT), json(COVERAGE));
    let tools = contract["tools"].as_array().expect("tools");
    let names = tools.iter().map(tool_name).collect::<BTreeSet<_>>();
    for declared_tool in coverage["tools"].as_object().expect("tools").keys() {
        assert!(
            names.contains(declared_tool.as_str()),
            "{declared_tool} is not a published tool"
        );
    }
    let mut stale = Vec::new();
    for tool in tools {
        let name = tool_name(tool);
        let paths = published_paths(&contract, tool);
        for key in declared(&coverage, name)["fields"]
            .as_object()
            .expect("fields")
            .keys()
        {
            if !paths.contains(key) {
                stale.push(format!("{name}: {key}"));
            }
        }
    }
    let common = coverage["common"]["fields"].as_object().expect("common");
    for key in common.keys() {
        let everywhere = tools
            .iter()
            .all(|tool| published_paths(&contract, tool).contains(key));
        if !everywhere {
            stale.push(format!("common: {key}"));
        }
    }
    assert!(
        stale.is_empty(),
        "these keys name no published query path; remove or rename them:\n{}",
        stale.join("\n")
    );
}

#[test]
fn declared_effects_come_from_the_listed_classes() {
    let coverage = json(COVERAGE);
    let classes = coverage["effects"]
        .as_array()
        .expect("effects")
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    let mut groups = vec![
        &coverage["envelope"]["fields"],
        &coverage["common"]["fields"],
    ];
    groups.extend(
        coverage["tools"]
            .as_object()
            .expect("tools")
            .values()
            .map(|tool| &tool["fields"]),
    );
    for fields in groups {
        for (key, effects) in fields.as_object().expect("fields") {
            for effect in effects.as_array().expect("effects array") {
                let effect = effect.as_str().expect("effect name");
                assert!(classes.contains(effect), "{key}: unknown effect {effect}");
            }
        }
    }
}

#[test]
fn parent_strips_one_segment() {
    assert_eq!(parent("ranges[].startLine"), "ranges[]");
    assert_eq!(parent("ranges[]"), "ranges");
    assert_eq!(parent("resources{}.query"), "resources{}");
    assert_eq!(parent("resources{}"), "resources");
    assert_eq!(parent("limit"), "");
}
