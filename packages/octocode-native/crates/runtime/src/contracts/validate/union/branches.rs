//! What a union's branches declare, for choosing the branch a failed value
//! meant: which fields select among them, and how many of the value's
//! fields a branch does not describe.
use super::super::literals::equivalent;
use super::branch_object;
use super::selectors::selector_literals;
use serde_json::{Map, Value};

/// The object forms a branch stands for: a nested `anyOf`/`oneOf` (through
/// `$ref`) contributes each of its own branches.
fn alternatives<'a>(root: &'a Value, branch: &'a Value) -> Vec<&'a Value> {
    let target = branch_object(root, branch);
    match ["anyOf", "oneOf"]
        .iter()
        .find_map(|keyword| target.get(*keyword).and_then(Value::as_array))
    {
        Some(nested) => nested
            .iter()
            .flat_map(|branch| alternatives(root, branch))
            .collect(),
        None => vec![target],
    }
}

/// The properties one object form declares, its `allOf` parts included.
fn properties<'a>(root: &'a Value, form: &'a Value, out: &mut Vec<&'a Map<String, Value>>) {
    let form = branch_object(root, form);
    if let Some(declared) = form.get("properties").and_then(Value::as_object) {
        out.push(declared);
    }
    for part in form
        .get("allOf")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        properties(root, part, out);
    }
}

fn declared<'a>(root: &'a Value, form: &'a Value) -> Vec<&'a Map<String, Value>> {
    let mut out = Vec::new();
    properties(root, form, &mut out);
    out
}

/// The fields that select among `branches`: forms pin each to literals
/// (`const`/`enum`), and not all to the same set. A literal on any other
/// field (a `role` only one form declares, or every form pins alike) is a
/// value of the form, not a choice between forms.
pub(super) fn selector_fields(root: &Value, branches: &[Value]) -> Vec<String> {
    let mut sets: Vec<(&str, Vec<Vec<&Value>>)> = Vec::new();
    for form in branches
        .iter()
        .flat_map(|branch| alternatives(root, branch))
    {
        for declared in declared(root, form) {
            for (name, schema) in declared {
                let schema = super::super::super::resolve_ref(schema, &root["$defs"]);
                let Some(literals) = selector_literals(schema) else {
                    continue;
                };
                let index = sets
                    .iter()
                    .position(|(field, _)| field == name)
                    .unwrap_or_else(|| {
                        sets.push((name, Vec::new()));
                        sets.len() - 1
                    });
                let known = &mut sets[index].1;
                if !known.iter().any(|set| same_literals(set, &literals)) {
                    known.push(literals);
                }
            }
        }
    }
    sets.into_iter()
        .filter(|(_, distinct)| distinct.len() > 1)
        .map(|(field, _)| field.to_owned())
        .collect()
}

fn same_literals(left: &[&Value], right: &[&Value]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .all(|literal| right.iter().any(|other| equivalent(literal, other)))
}

/// How many of the value's fields no form of `branch` declares: the fewer,
/// the more specifically the branch describes the value (an open branch
/// reports no unknown field for them).
pub(super) fn undeclared_fields(root: &Value, branch: &Value, value: &Value) -> usize {
    let Some(object) = value.as_object() else {
        return 0;
    };
    let forms = alternatives(root, branch)
        .into_iter()
        .flat_map(|form| declared(root, form))
        .collect::<Vec<_>>();
    object
        .keys()
        .filter(|key| !forms.iter().any(|declared| declared.contains_key(*key)))
        .count()
}
