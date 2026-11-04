//! What a union's branches declare, for choosing the branch a failed value
//! meant: which fields select among them, and how many of the value's
//! fields a branch does not describe.
use super::super::literals::equivalent;
use super::branch_object;
use super::selectors::selector_literals;
use serde_json::{Map, Value};

/// The object forms a branch stands for: a nested `anyOf`/`oneOf` (through
/// `$ref`) contributes each of its own branches.
pub(super) fn alternatives<'a>(root: &'a Value, branch: &'a Value) -> Vec<&'a Value> {
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

/// The fields one object form requires, its `allOf` parts included.
fn required<'a>(root: &'a Value, form: &'a Value, out: &mut Vec<&'a str>) {
    let form = branch_object(root, form);
    out.extend(
        form.get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str),
    );
    for part in form
        .get("allOf")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        required(root, part, out);
    }
}

/// Whether a form's literal fields accept the value's: each sent literal
/// field must hold one of the form's literals, and one it omits must not be
/// required without a default (omitting it selects another form).
fn selectors_fit(root: &Value, form: &Value, value: &Value) -> bool {
    let mut needed = Vec::new();
    required(root, form, &mut needed);
    declared(root, form)
        .into_iter()
        .flatten()
        .all(|(name, schema)| {
            let Some(literals) = selector_literals(root, schema) else {
                return true;
            };
            match value.get(name) {
                Some(actual) => literals.iter().any(|literal| equivalent(literal, actual)),
                None => {
                    crate::contracts::schema_facts::schema_keyword(
                        schema,
                        &root["$defs"],
                        "default",
                    )
                    .is_some()
                        || !needed.contains(&name.as_str())
                }
            }
        })
}

/// The fields a caller may author in the forms `value` still fits: forms
/// whose literal fields accept the value's and that declare every other
/// field it sends that some form declares; failing that, the forms whose
/// literal fields accept it. `None` when no form's literal fields do. An
/// unknown field's allowed set is then the row's form, not the one form
/// branch scoring picked (localFetch `view:"content"` reads a whole file,
/// ranges or matches).
pub(super) fn compatible_fields(
    root: &Value,
    branches: &[Value],
    value: &Value,
) -> Option<Vec<String>> {
    let object = value.as_object()?;
    let forms = branches
        .iter()
        .flat_map(|branch| alternatives(root, branch))
        .map(|form| (selectors_fit(root, form, value), declared(root, form)))
        .collect::<Vec<_>>();
    let declares = |maps: &[&Map<String, Value>], name: &str| {
        maps.iter().any(|declared| declared.contains_key(name))
    };
    let sent = object
        .keys()
        .filter(|name| forms.iter().any(|(_, maps)| declares(maps, name)))
        .collect::<Vec<_>>();
    let fit = forms.iter().filter(|(fits, _)| *fits).collect::<Vec<_>>();
    let fitting = fit
        .iter()
        .copied()
        .filter(|(_, maps)| sent.iter().all(|name| declares(maps, name)))
        .collect::<Vec<_>>();
    let chosen = if fitting.is_empty() { fit } else { fitting };
    if chosen.is_empty() {
        return None;
    }
    let mut fields = chosen
        .iter()
        .flat_map(|(_, maps)| maps.iter().copied().flatten())
        .filter(|(_, property)| super::super::schema::authored_property(root, property))
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    fields.sort();
    fields.dedup();
    Some(fields)
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
                let Some(literals) = selector_literals(root, schema) else {
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
