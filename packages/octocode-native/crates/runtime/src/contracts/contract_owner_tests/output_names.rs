//! A tool's own output field names are structure, not data: the structured
//! sanitizer must never read one as a credential member and redact its value
//! (`matchKeys[].key` once hid Cargo key paths as `[REDACTED-ENVVARSECRETS]`).
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Whether `schema`, through `$ref`s and compositions, admits a string: a
/// number or an array never holds a value the sanitizer could hide.
fn admits_string(schema: &Value, defs: &Value) -> bool {
    if let Some(target) = schema.get("$ref").and_then(Value::as_str) {
        let name = target.rsplit('/').next().unwrap_or(target);
        return defs.get(name).is_some_and(|d| admits_string(d, defs));
    }
    let typed = match schema.get("type") {
        Some(Value::String(kind)) => kind == "string",
        Some(Value::Array(kinds)) => kinds.iter().any(|kind| kind == "string"),
        _ => false,
    };
    typed
        || ["anyOf", "oneOf", "allOf"].iter().any(|keyword| {
            schema
                .get(keyword)
                .and_then(Value::as_array)
                .is_some_and(|branches| branches.iter().any(|b| admits_string(b, defs)))
        })
}

/// Every string-valued property name an output schema declares, following
/// `$ref`s into the contract's definitions.
fn output_property_names(schema: &Value, defs: &Value) -> BTreeSet<String> {
    fn visit<'a>(
        schema: &'a Value,
        defs: &'a Value,
        seen: &mut BTreeSet<&'a str>,
        names: &mut BTreeSet<String>,
    ) {
        match schema {
            Value::Object(map) => {
                for (key, child) in map {
                    match (key.as_str(), child) {
                        ("$ref", Value::String(target)) => {
                            let name = target.rsplit('/').next().unwrap_or(target);
                            if let Some(definition) = defs.get(name)
                                && seen.insert(name)
                            {
                                visit(definition, defs, seen, names);
                            }
                        }
                        ("properties", Value::Object(properties)) => {
                            for (name, property) in properties {
                                if admits_string(property, defs) {
                                    names.insert(name.clone());
                                }
                                visit(property, defs, seen, names);
                            }
                        }
                        _ => visit(child, defs, seen, names),
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    visit(item, defs, seen, names);
                }
            }
            _ => {}
        }
    }
    let mut names = BTreeSet::new();
    visit(schema, defs, &mut BTreeSet::new(), &mut names);
    names
}

/// Values a string field of a result row may hold: a word, an identifier, a
/// dotted key path, an opaque run and a mixed-case chosen password.
const VALUES: [&str; 5] = [
    "serde",
    "serde_json",
    "profile.dev.package.serde_json",
    "abcdefghijklmnopqrstuvwxyz012345",
    "Fake0Value9Not4Real",
];

#[test]
fn no_output_field_name_reads_as_a_credential_member() {
    let contract = crate::contracts::parsed_contract().expect("generated contract");
    let defs = &contract["$defs"];
    let mut names = BTreeSet::new();
    for tool in contract["tools"].as_array().expect("tools") {
        names.extend(output_property_names(&tool["outputSchema"], defs));
    }
    assert!(
        names.contains("path") && names.contains("keyPath"),
        "{names:?}"
    );
    let mut hidden = Vec::new();
    for name in &names {
        for value in VALUES {
            let mut row = json!({ name: value });
            let removed = octocode_engine::security::sanitizer::sanitize_structure(&mut row)
                .expect("scanned");
            if !removed.is_empty() || row[name] != value {
                hidden.push(format!("{name}: {value}"));
            }
        }
    }
    assert_eq!(
        hidden,
        Vec::<String>::new(),
        "an output field named like a credential has its value hidden; rename it in core"
    );
}
