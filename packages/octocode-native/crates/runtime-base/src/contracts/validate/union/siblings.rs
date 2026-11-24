use super::super::ValidationIssue;
use super::{
    render_selector, selector_literals, selectors_accept, sibling_selector_values, sibling_switch,
};
use serde_json::Value;

pub(super) fn branch_object<'a>(root: &'a Value, branch: &'a Value) -> &'a Value {
    crate::contracts::resolve_ref(branch, &root["$defs"])
}

/// An unknown field that a sibling branch declares is a mode mismatch, not a
/// typo. Record the sibling's missing required fields so the error projector
/// can say which mode the field belongs to (e.g. discovery-only `pageSize`).
/// When the query's own selector picks another branch, record the selector
/// values that admit the field (`siblingRequires`), and, when one value
/// does in every declaring branch, that switch (`siblingSwitch`:
/// `{"operation":"search"}`), which an example applies. A nested key counts
/// only where a sibling declares it at the same path (`filters.language` of
/// a search, refused by a list): a nested one that merely shares a branch
/// field's name (`filters.owner`) is not that field.
pub(super) fn annotate_sibling_branch_fields(
    issues: &mut [ValidationIssue],
    root: &Value,
    branches: &[Value],
    value: &Value,
    path: &[String],
) {
    for item in issues.iter_mut().filter(|item| {
        item.rule_id == "schema.unknown-field"
            && item.path.len() > path.len()
            && item.path.starts_with(path)
    }) {
        let within = &item.path[path.len()..];
        let Some(field) = within.first() else {
            continue;
        };
        let declaring = branches
            .iter()
            .flat_map(|branch| super::branches::alternatives(root, branch))
            .filter(|branch| match within {
                [_] => branch
                    .get("properties")
                    .and_then(|properties| properties.get(field))
                    .is_some(),
                _ => names_nested_key(root, branch, within),
            })
            .collect::<Vec<_>>();
        // Missing fields only repair a branch in the query's own mode; when no
        // declaring branch shares its selectors, name the selectors instead
        // (`matchString` on an issue → the operations that accept it).
        let same_mode = declaring
            .iter()
            .copied()
            .filter(|branch| selectors_accept(root, branch, value))
            .collect::<Vec<_>>();
        let Some(schema) = item.schema.as_mut().and_then(Value::as_object_mut) else {
            continue;
        };
        if let Some(requires) = sibling_missing_fields(root, &same_mode, value, field) {
            schema.insert("siblingRequires".into(), Value::Array(requires));
        } else if let Some(requires) = sibling_selector_values(root, &declaring, value, field) {
            schema.insert("siblingRequires".into(), Value::Array(requires));
            if let Some(switch) = sibling_switch(root, &declaring, value, field) {
                schema.insert("siblingSwitch".into(), switch);
            }
        }
    }
}

/// Whether `branch` names the key `within` ends with among the
/// `properties` of the object at the path before it (any form of that
/// object): a value that takes any key (`additionalProperties`) names none.
fn names_nested_key(root: &Value, branch: &Value, within: &[String]) -> bool {
    fn names(root: &Value, schema: &Value, key: &str, depth: usize) -> bool {
        let shape = crate::contracts::resolve_ref(schema, &root["$defs"]);
        shape["properties"].get(key).is_some()
            || (depth < 8
                && ["allOf", "anyOf", "oneOf"].iter().any(|keyword| {
                    shape[keyword]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|form| names(root, form, key, depth + 1))
                }))
    }
    let Some((key, parents)) = within.split_last() else {
        return false;
    };
    super::super::sample::schemas_at(branch, &root["$defs"], parents)
        .into_iter()
        .any(|parent| names(root, parent, key, 0))
}

/// An unknown field directly in the union's value lists as allowed every
/// field of the forms the value still fits (`compatible_fields`), replacing
/// the one scored branch's own fields; kept when no form fits.
pub(super) fn annotate_allowed_fields(
    issues: &mut [ValidationIssue],
    root: &Value,
    branches: &[Value],
    value: &Value,
    path: &[String],
) {
    let mut allowed = None;
    for item in issues.iter_mut().filter(|item| {
        item.rule_id == "schema.unknown-field"
            && item.path.len() == path.len() + 1
            && item.path.starts_with(path)
    }) {
        let Some(schema) = item.schema.as_mut().and_then(Value::as_object_mut) else {
            continue;
        };
        let fields = allowed
            .get_or_insert_with(|| super::branches::compatible_fields(root, branches, value));
        if let Some(fields) = fields {
            schema.insert("knownFields".into(), serde_json::json!(fields));
        }
    }
}

/// A value that chooses none of the forms `forms` names (each its required
/// fields, sharing none): the fields the scored form lacks are a guess, so
/// its missing-field issues give way to the forms themselves, stated on each
/// unknown field directly in the value, or once on the value when it sent
/// none (`{}`).
pub(super) fn name_forms(
    issues: &mut Vec<ValidationIssue>,
    forms: &[Vec<String>],
    path: &[String],
) {
    let depth = path.len();
    let direct =
        |item: &ValidationIssue| item.path.len() == depth + 1 && item.path.starts_with(path);
    issues.retain(|item| !(item.rule_id == "schema.required" && direct(item)));
    let forms = serde_json::json!(forms);
    let mut named = false;
    for item in issues
        .iter_mut()
        .filter(|item| item.rule_id == "schema.unknown-field" && direct(item))
    {
        if let Some(schema) = item.schema.as_mut().and_then(Value::as_object_mut) {
            schema.insert("forms".into(), forms.clone());
            named = true;
        }
    }
    if !named {
        let mut missing =
            super::super::issue("schema.union", path.to_vec(), "Missing required fields").issues;
        for item in &mut missing {
            item.schema = Some(serde_json::json!({ "forms": forms }));
        }
        issues.splice(0..0, missing);
    }
}

/// The required fields each declaring branch still lacks, one alternative per
/// branch (`pattern or rule` for an astSearch match field), so the projector
/// names every form that accepts the field, not only the first.
pub(super) fn sibling_missing_fields(
    root: &Value,
    declaring: &[&Value],
    value: &Value,
    field: &str,
) -> Option<Vec<Value>> {
    let mut options: Vec<String> = Vec::new();
    for branch in declaring {
        let missing = branch
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|required| *required != field && value.get(*required).is_none())
            .map(|required| {
                let schema = super::super::super::resolve_ref(
                    &branch["properties"][required],
                    &root["$defs"],
                );
                selector_literals(root, schema).map_or_else(
                    || required.to_owned(),
                    |literals| render_selector(required, &literals),
                )
            })
            .collect::<Vec<_>>();
        if missing.is_empty() {
            continue;
        }
        let option = missing.join(" and ");
        if !options.contains(&option) {
            options.push(option);
        }
    }
    (!options.is_empty()).then(|| vec![Value::String(options.join(" or "))])
}
