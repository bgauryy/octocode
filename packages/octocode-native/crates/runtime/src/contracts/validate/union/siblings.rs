use super::super::ValidationIssue;
use super::{render_selector, selector_literals, selectors_accept, sibling_selector_values};
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
        } else if let Some(requires) = sibling_selector_values(&declaring, value, field) {
            schema.insert("siblingRequires".into(), Value::Array(requires));
        }
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
