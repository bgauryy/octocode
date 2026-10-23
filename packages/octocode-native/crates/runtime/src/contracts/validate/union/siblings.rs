use super::super::ValidationIssue;
use super::{
    render_selector, selector_literals, selectors_accept, sibling_selector_values, single_literal,
};
use serde_json::Value;

pub(super) fn branch_object<'a>(root: &'a Value, branch: &'a Value) -> &'a Value {
    branch
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| root.pointer(reference.strip_prefix('#').unwrap_or(reference)))
        .unwrap_or(branch)
}

/// An unknown field that a sibling branch declares is a mode mismatch, not a
/// typo. Record the sibling's missing required fields so the error projector
/// can say which mode the field belongs to (e.g. discovery-only `pageSize`).
/// When the query omits the selectors of the one branch in its mode, and
/// each is a single literal, the fix sets them (`setFields`:
/// `nameRegex` → `operation:"files"`). A selector the query spells at its
/// default value says what omitting it says, so the fix is the same switch
/// (`operation:"tree"` → `"files"`); when the query's own
/// selector picks another branch, the field is that mode's
/// (`siblingConflict`).
pub(super) fn annotate_sibling_branch_fields(
    issues: &mut [ValidationIssue],
    root: &Value,
    branches: &[Value],
    value: &Value,
) {
    for item in issues
        .iter_mut()
        .filter(|item| item.rule_id == "schema.unknown-field")
    {
        let Some(field) = item.path.last() else {
            continue;
        };
        // The fields any branch lists the unknown field's value for: the
        // selector a misnamed `type`/`analysis` meant; for a `true` field,
        // the fields that list its name (`merged:true` → `state`).
        let listing = |listed: &Value| {
            let mut fields = branches
                .iter()
                .filter_map(|branch| branch_object(root, branch).get("properties")?.as_object())
                .flat_map(|properties| {
                    super::super::schema::fields_listing(root, properties, listed)
                })
                .collect::<Vec<_>>();
            fields.sort();
            fields.dedup();
            fields
        };
        // The fields whose declared value kinds name the unknown field
        // (`branch` → `ref`).
        let mut of_kind = branches
            .iter()
            .filter_map(|branch| branch_object(root, branch).get("properties")?.as_object())
            .flat_map(|properties| super::super::schema::fields_of_kind(root, properties, field))
            .collect::<Vec<_>>();
        of_kind.sort();
        of_kind.dedup();
        for (key, fields) in [
            ("kindFields", of_kind),
            ("valueFields", listing(&value[field])),
            (
                "nameFields",
                if value[field] == Value::Bool(true) {
                    listing(&Value::from(field.as_str()))
                } else {
                    Vec::new()
                },
            ),
        ] {
            if !fields.is_empty()
                && let Some(schema) = item.schema.as_mut().and_then(Value::as_object_mut)
            {
                schema.insert(key.into(), serde_json::json!(fields));
            }
        }
        let declaring = branches
            .iter()
            .map(|branch| branch_object(root, branch))
            .filter(|branch| {
                branch
                    .get("properties")
                    .and_then(|properties| properties.get(field))
                    .is_some()
            })
            .collect::<Vec<_>>();
        // Missing fields only repair a branch in the query's own mode; when no
        // declaring branch shares its selectors, name the selectors instead
        // (`matchString` on an issue → the operations that accept it).
        let same_mode = declaring
            .iter()
            .copied()
            .filter(|branch| selectors_accept(branch, value))
            .collect::<Vec<_>>();
        let Some(schema) = item.schema.as_mut().and_then(Value::as_object_mut) else {
            continue;
        };
        if let Some(requires) = sibling_missing_fields(root, &same_mode, value, field) {
            schema.insert("siblingRequires".into(), Value::Array(requires));
            if let [branch] = same_mode.as_slice()
                && let Some(selectors) = omitted_selectors(root, branch, value, field)
                && !selectors.is_empty()
            {
                schema.insert("setFields".into(), Value::Object(selectors));
            }
        } else if let Some(requires) = sibling_selector_values(&declaring, value) {
            schema.insert("siblingRequires".into(), Value::Array(requires));
            match declaring.as_slice() {
                [branch] => match defaulted_selector_fix(root, branches, branch, value, field) {
                    Some(selectors) => {
                        schema.insert("setFields".into(), Value::Object(selectors));
                    }
                    None => {
                        schema.insert("siblingConflict".into(), Value::Bool(true));
                    }
                },
                _ => {
                    schema.insert("siblingConflict".into(), Value::Bool(true));
                }
            }
        }
    }
}

/// The selectors that move a query into `branch` when every selector it
/// sets that `branch` rejects is spelled at a default value (one some branch
/// declares as that selector's `default`): with those spelled defaults
/// omitted, `branch`'s omitted selectors ([`omitted_selectors`]), each
/// spelled selector among them.
fn defaulted_selector_fix(
    root: &Value,
    branches: &[Value],
    branch: &Value,
    value: &Value,
    field: &str,
) -> Option<serde_json::Map<String, Value>> {
    let properties = branch.get("properties").and_then(Value::as_object)?;
    let spelled = properties
        .iter()
        .filter_map(|(name, schema)| {
            let literals = selector_literals(schema)?;
            let set = value.get(name)?;
            (!literals.contains(&set)).then_some((name, set))
        })
        .collect::<Vec<_>>();
    let defaulted = |name: &str, set: &Value| {
        branches.iter().any(|other| {
            let schema = &branch_object(root, other)["properties"][name];
            super::super::super::resolve_ref(schema, &root["$defs"]).get("default") == Some(set)
        })
    };
    if spelled.is_empty() || !spelled.iter().all(|(name, set)| defaulted(name, set)) {
        return None;
    }
    let mut omitted = value.clone();
    let object = omitted.as_object_mut()?;
    for (name, _) in &spelled {
        object.remove(name.as_str());
    }
    let selectors = omitted_selectors(root, branch, &omitted, field)?;
    spelled
        .iter()
        .all(|(name, _)| selectors.contains_key(name.as_str()))
        .then_some(selectors)
}

/// The required fields `branch` still lacks (besides `field`), each with
/// its one literal (`operation:"files"`); `None` when one is no single
/// literal, so only the caller can fill it, or when the query carries a
/// field `branch` does not declare (it mixes forms: setting the selector
/// would only move the conflict).
fn omitted_selectors(
    root: &Value,
    branch: &Value,
    value: &Value,
    field: &str,
) -> Option<serde_json::Map<String, Value>> {
    let declared = branch.get("properties").and_then(Value::as_object)?;
    if !value
        .as_object()?
        .keys()
        .all(|key| declared.contains_key(key))
    {
        return None;
    }
    branch
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|required| *required != field && value.get(*required).is_none())
        .map(|required| {
            let schema =
                super::super::super::resolve_ref(&branch["properties"][required], &root["$defs"]);
            Some((required.to_owned(), single_literal(schema)?.clone()))
        })
        .collect()
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
                selector_literals(schema).map_or_else(
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
