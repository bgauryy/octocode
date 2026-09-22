// Integration test crate — malformed audit fixtures should fail loudly.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use octocode_native::contracts::contract_json;
use serde_json::Value;

const COVERAGE_JSON: &str = include_str!("../src/contracts/field-effect-coverage.json");
const DISCRIMINATOR_FIELDS: &[&str] = &[
    "operation",
    "analysis",
    "ruleKind",
    "regex",
    "treeKind",
    "type",
];

fn collect_fields(schema: &Value, prefix: &str, fields: &mut BTreeSet<String>) {
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (name, child) in properties {
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}.{name}")
            };
            fields.insert(path.clone());
            collect_fields(child, &path, fields);
        }
    }
    if let Some(items) = schema.get("items") {
        let path = if prefix.is_empty() {
            "[]".to_owned()
        } else {
            format!("{prefix}[]")
        };
        collect_fields(items, &path, fields);
    }
    for union in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = schema.get(union).and_then(Value::as_array) {
            for branch in branches {
                collect_fields(branch, prefix, fields);
            }
        }
    }
}

fn collect_values(schema: &Value, target: &str, prefix: &str, values: &mut BTreeSet<String>) {
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (name, child) in properties {
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}.{name}")
            };
            if path == target {
                if let Some(value) = child.get("const").and_then(Value::as_str) {
                    values.insert(value.to_owned());
                }
                if let Some(entries) = child.get("enum").and_then(Value::as_array) {
                    for entry in entries.iter().filter_map(Value::as_str) {
                        values.insert(entry.to_owned());
                    }
                }
            }
            collect_values(child, target, &path, values);
        }
    }
    if let Some(items) = schema.get("items") {
        let path = if prefix.is_empty() {
            "[]".to_owned()
        } else {
            format!("{prefix}[]")
        };
        collect_values(items, target, &path, values);
    }
    for union in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = schema.get(union).and_then(Value::as_array) {
            for branch in branches {
                collect_values(branch, target, prefix, values);
            }
        }
    }
}

fn discriminator_paths(schema: &Value) -> BTreeSet<String> {
    let mut fields = BTreeSet::new();
    collect_fields(schema, "", &mut fields);
    fields
        .into_iter()
        .filter(|path| {
            let leaf = path.rsplit('.').next().unwrap_or(path);
            DISCRIMINATOR_FIELDS.contains(&leaf)
        })
        .filter(|path| {
            let mut values = BTreeSet::new();
            collect_values(schema, path, "", &mut values);
            !values.is_empty()
        })
        .collect()
}

fn string_set(values: &Value) -> BTreeSet<String> {
    values
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_public_field_and_discriminator_has_declared_engine_effect_coverage() {
    let contract: Value = serde_json::from_str(contract_json()).expect("generated contract JSON");
    let coverage: Value = serde_json::from_str(COVERAGE_JSON).expect("field coverage JSON");

    assert_eq!(
        coverage["contractFingerprint"], contract["fingerprint"],
        "field-effect coverage must be reviewed whenever the canonical contract changes"
    );

    let allowed_classes = string_set(&coverage["coverageClasses"]);
    let coverage_tools = coverage["tools"].as_object().expect("coverage tools");
    let contract_tools = contract["tools"].as_array().expect("contract tools");
    let contract_names = contract_tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let coverage_names = coverage_tools.keys().cloned().collect::<BTreeSet<_>>();
    assert_eq!(
        coverage_names, contract_names,
        "coverage must name exactly the 13 public tools"
    );

    for tool in contract_tools {
        let name = tool["name"].as_str().expect("tool name");
        let tool_coverage = &coverage_tools[name];
        let evidence = tool_coverage["evidence"]
            .as_array()
            .expect("tool evidence array");
        assert!(
            !evidence.is_empty()
                && evidence
                    .iter()
                    .all(|entry| entry.as_str().is_some_and(|s| !s.is_empty())),
            "{name}: coverage must cite deterministic native tests"
        );
        for relative in evidence.iter().filter_map(Value::as_str) {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("{name}: cannot read {}: {error}", path.display()));
            assert!(
                source.contains("#[test]") || source.contains("#[tokio::test]"),
                "{name}: evidence {} must contain executable tests",
                path.display()
            );
        }

        let mut actual_fields = BTreeSet::new();
        collect_fields(&tool["querySchema"], "", &mut actual_fields);
        let declared_fields = tool_coverage["fields"]
            .as_object()
            .expect("covered fields")
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            declared_fields, actual_fields,
            "{name}: schema fields and reviewed engine-effect coverage drifted"
        );

        for (path, classes) in tool_coverage["fields"]
            .as_object()
            .expect("covered field map")
        {
            let declared = string_set(classes);
            assert!(
                !declared.is_empty(),
                "{name}.{path}: missing coverage class"
            );
            assert!(
                declared.is_subset(&allowed_classes),
                "{name}.{path}: unknown coverage class in {declared:?}"
            );
            assert_ne!(
                declared,
                BTreeSet::from(["validated".to_owned()]),
                "{name}.{path}: schema validation alone cannot satisfy an advertised engine effect"
            );
        }

        let actual_variants = tool["variants"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|variant| variant["name"].as_str())
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        let covered_variants = string_set(&tool_coverage["variants"]);
        assert_eq!(
            covered_variants, actual_variants,
            "{name}: every canonical variant must remain tied to deterministic native evidence"
        );

        let expected_discriminator_paths = discriminator_paths(&tool["querySchema"]);
        let discriminators = tool_coverage["discriminators"]
            .as_object()
            .expect("discriminator map");
        let covered_discriminator_paths = discriminators.keys().cloned().collect::<BTreeSet<_>>();
        assert_eq!(
            covered_discriminator_paths, expected_discriminator_paths,
            "{name}: operation discriminator inventory drifted"
        );
        for (path, expected) in discriminators {
            let mut actual = BTreeSet::new();
            collect_values(&tool["querySchema"], path, "", &mut actual);
            assert_eq!(
                string_set(expected),
                actual,
                "{name}.{path}: every advertised branch must have deterministic native coverage"
            );
        }
    }
}

#[test]
fn field_effect_registry_records_all_supported_coverage_classes() {
    let coverage: Value = serde_json::from_str(COVERAGE_JSON).expect("field coverage JSON");
    let mut observed: BTreeMap<String, usize> = BTreeMap::new();
    for tool in coverage["tools"].as_object().expect("tools").values() {
        for classes in tool["fields"].as_object().expect("fields").values() {
            for class in string_set(classes) {
                *observed.entry(class).or_default() += 1;
            }
        }
    }
    for required in [
        "consumed",
        "forwarded",
        "output-control",
        "continuation",
        "workflow-metadata",
        "security-policy",
    ] {
        assert!(
            observed.get(required).is_some_and(|count| *count > 0),
            "coverage class {required} is declared but unused"
        );
    }
}
