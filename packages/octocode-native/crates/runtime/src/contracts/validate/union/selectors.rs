use super::super::ValidationIssue;
use super::super::literals::equivalent;
use super::branch_object;
use serde_json::Value;

/// Branches chosen by a literal selector (e.g. `operation`) need no missing
/// field: name the selector values of every branch declaring the field, so
/// `review` on a commit query points at `operation:"pullRequest"` and
/// `keywords` on a pypi query at every `type` that accepts it. The field's
/// own value is what the caller sends, never what admits it: a branch that
/// also pins `field` (`view:"content"` beside an `operation`) is named by
/// its other selectors, so `view` on a callers query points at the
/// operations that take a view, once.
pub(super) fn sibling_selector_values(
    root: &Value,
    declaring: &[&Value],
    value: &Value,
    field: &str,
) -> Option<Vec<Value>> {
    let mut alternatives = Vec::new();
    for branch in declaring {
        let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
            continue;
        };
        let selectors = properties
            .iter()
            .filter(|(name, _)| name.as_str() != field)
            .filter_map(|(name, schema)| {
                let literals = selector_literals(root, schema)?;
                let actual = value.get(name)?;
                (!literals.iter().any(|literal| equivalent(literal, actual)))
                    .then_some((name.as_str(), literals))
            })
            .collect::<Vec<_>>();
        alternatives.push(selectors);
    }
    merge_alternatives(alternatives).map(|text| vec![Value::String(text)])
}

/// Alternatives, each the selectors one form needs: a form needing one
/// selector merges with the others needing that selector (`view one of
/// "matchOnly", "count"`); a form needing several stays its own
/// alternative (`a:1 and b:2`). `None` when no form needs any.
fn merge_alternatives<'a>(alternatives: Vec<Vec<(&'a str, Vec<&'a Value>)>>) -> Option<String> {
    let mut merged: Vec<(&str, Vec<&Value>)> = Vec::new();
    let mut options = Vec::new();
    for selectors in alternatives {
        match selectors.as_slice() {
            [] => {}
            [(name, literals)] => {
                let index = merged
                    .iter()
                    .position(|(known, _)| known == name)
                    .unwrap_or_else(|| {
                        merged.push((name, Vec::new()));
                        merged.len() - 1
                    });
                for literal in literals {
                    if !merged[index]
                        .1
                        .iter()
                        .any(|known| equivalent(known, literal))
                    {
                        merged[index].1.push(literal);
                    }
                }
            }
            several => {
                let option = several
                    .iter()
                    .map(|(name, literals)| render_selector(name, literals))
                    .collect::<Vec<_>>()
                    .join(" and ");
                if !options.contains(&option) {
                    options.push(option);
                }
            }
        }
    }
    let mut rendered = merged
        .iter()
        .map(|(name, literals)| render_selector(name, literals))
        .collect::<Vec<_>>();
    rendered.extend(options);
    (!rendered.is_empty()).then(|| rendered.join(" or "))
}

/// Whether every literal selector of `branch` accepts the query's value.
pub(super) fn selectors_accept(root: &Value, branch: &Value, value: &Value) -> bool {
    branch
        .get("properties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .all(
            |(name, schema)| match (selector_literals(root, schema), value.get(name)) {
                (Some(literals), Some(actual)) => {
                    literals.iter().any(|literal| equivalent(literal, actual))
                }
                _ => true,
            },
        )
}

/// The literal values a `const` or `enum` selector accepts.
pub(super) fn selector_literals<'a>(root: &'a Value, schema: &'a Value) -> Option<Vec<&'a Value>> {
    let schema = branch_object(root, schema);
    if let Some(literal) = schema.get("const") {
        return Some(vec![literal]);
    }
    schema
        .get("enum")
        .and_then(Value::as_array)
        .map(|values| values.iter().collect())
}

pub(super) fn render_selector<V: std::borrow::Borrow<Value>>(name: &str, literals: &[V]) -> String {
    match literals {
        [literal] => format!("{name}:{}", render_literal(literal.borrow())),
        _ => format!(
            "{name} one of {}",
            literals
                .iter()
                .map(|literal| render_literal(literal.borrow()))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn literal_accepts(root: &Value, schema: &Value, value: &Value) -> bool {
    let schema = branch_object(root, schema);
    schema
        .get("const")
        .is_some_and(|constant| equivalent(constant, value))
        || schema
            .get("enum")
            .and_then(Value::as_array)
            .is_some_and(|values| values.iter().any(|literal| equivalent(literal, value)))
}

pub(super) fn single_literal<'a>(root: &'a Value, schema: &'a Value) -> Option<&'a Value> {
    let schema = branch_object(root, schema);
    schema.get("const").or_else(|| {
        schema
            .get("enum")
            .and_then(Value::as_array)
            .filter(|values| values.len() == 1)
            .and_then(|values| values.first())
    })
}

fn render_literal(value: &Value) -> String {
    value.to_string()
}

/// A literal rejected by the selected branch but accepted by a sibling is a
/// cross-field rule (e.g. `unique:"list"` needs `view:"matchOnly"`).
/// Name the sibling's selector values instead of only the local constant.
pub(super) fn annotate_sibling_selectors(
    issues: &mut [ValidationIssue],
    root: &Value,
    branches: &[Value],
    value: &Value,
    path: &[String],
) {
    for item in issues.iter_mut().filter(|item| {
        matches!(item.rule_id.as_str(), "schema.const" | "schema.enum")
            && item.path.len() == path.len() + 1
    }) {
        let Some(field) = item.path.last() else {
            continue;
        };
        let Some(received) = value.get(field) else {
            continue;
        };
        if let Some(requirement) = sibling_requirement(root, branches, value, (field, received)) {
            item.message = format!(
                "{} ({field}:{} requires {requirement})",
                item.message,
                render_literal(received)
            );
        }
    }
}

/// The single-literal selectors that let a sibling accept `field:received`,
/// merged across every sibling that needs any. `None` when a sibling accepting
/// the value differs from the row by a multi-value selector (a
/// `direction:"both"` form whose `operation` enum lacks `hubs`): no
/// `name:literal` names that form, so any requirement would be partial.
fn sibling_requirement(
    root: &Value,
    branches: &[Value],
    value: &Value,
    (field, received): (&str, &Value),
) -> Option<String> {
    let mut alternatives = Vec::new();
    for branch in branches.iter().map(|branch| branch_object(root, branch)) {
        let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
            continue;
        };
        if !properties
            .get(field)
            .is_some_and(|schema| literal_accepts(root, schema, received))
        {
            continue;
        }
        let mut selectors = Vec::new();
        for (name, schema) in properties.iter().filter(|(name, _)| name.as_str() != field) {
            match selector_change(root, schema, value, name) {
                SelectorChange::None => {}
                SelectorChange::Name(literal) => selectors.push((name.as_str(), vec![literal])),
                SelectorChange::Unnameable => return None,
            }
        }
        alternatives.push(selectors);
    }
    merge_alternatives(alternatives)
}

/// The change a sibling's property asks of the row.
enum SelectorChange<'a> {
    /// It accepts the row as sent, or it is no selector.
    None,
    /// It pins one literal the row lacks: `name:literal`.
    Name(&'a Value),
    /// A multi-value selector rejects the row's value.
    Unnameable,
}

fn selector_change<'a>(
    root: &'a Value,
    schema: &'a Value,
    value: &Value,
    name: &str,
) -> SelectorChange<'a> {
    let actual = value.get(name);
    if let Some(literal) = single_literal(root, schema) {
        return if actual.is_some_and(|actual| equivalent(literal, actual)) {
            SelectorChange::None
        } else {
            SelectorChange::Name(literal)
        };
    }
    match actual {
        Some(actual)
            if selector_literals(root, schema).is_some()
                && !literal_accepts(root, schema, actual) =>
        {
            SelectorChange::Unnameable
        }
        _ => SelectorChange::None,
    }
}

/// A `{"not":{}}` property forbids the field in its branch. When a sibling
/// branch allows that field but forbids fields the caller also sent, the
/// fields are mutually exclusive: name them instead of the bare `not` rule.
pub(super) fn annotate_forbidden_fields(
    issues: &mut [ValidationIssue],
    root: &Value,
    branches: &[Value],
    value: &Value,
    path: &[String],
) {
    let forbids = |schema: &Value| {
        branch_object(root, schema)
            .get("not")
            .is_some_and(|not| not == &Value::Object(Default::default()))
    };
    for item in issues
        .iter_mut()
        .filter(|item| item.rule_id == "schema.not" && item.path.len() == path.len() + 1)
    {
        let Some(field) = item.path.last() else {
            continue;
        };
        let mut conflicts: Vec<String> = Vec::new();
        for branch in branches.iter().map(|branch| branch_object(root, branch)) {
            let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
                continue;
            };
            if properties.get(field).is_none_or(forbids) {
                continue;
            }
            for (name, schema) in properties {
                if forbids(schema) && value.get(name).is_some() && !conflicts.contains(name) {
                    conflicts.push(name.clone());
                }
            }
        }
        if conflicts.is_empty() {
            continue;
        }
        item.message = format!(
            "`{field}` cannot be combined with {}: they are mutually exclusive, send one or the other",
            conflicts
                .iter()
                .map(|name| format!("`{name}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

/// A missing selector (`operation`) when the fields the caller sent fit
/// only some of its values: name the fields that rule the others out and
/// the selector values that accept them all (`nameRegex requires
/// operation:"files"`), so the repair is one step, not a guess. Read from
/// the branches alone: a branch admits the row when it declares, and does
/// not forbid, every field the row carries.
pub(super) fn annotate_missing_selector(
    issues: &mut [ValidationIssue],
    root: &Value,
    branches: &[Value],
    value: &Value,
    path: &[String],
) {
    let Some(sent) = value.as_object() else {
        return;
    };
    let declares = |properties: &serde_json::Map<String, Value>, name: &str| {
        properties.get(name).is_some_and(|schema| {
            branch_object(root, schema).get("not")
                != Some(&Value::Object(serde_json::Map::default()))
        })
    };
    for item in issues
        .iter_mut()
        .filter(|item| super::missing_literal(item, path.len()))
    {
        let Some(field) = item.path.last() else {
            continue;
        };
        let mut admitting: Vec<&Value> = Vec::new();
        let mut ruling_out: Vec<&str> = Vec::new();
        let mut rejecting = false;
        for branch in branches.iter().map(|branch| branch_object(root, branch)) {
            let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
                continue;
            };
            let Some(literals) = properties
                .get(field)
                .and_then(|schema| selector_literals(root, schema))
            else {
                continue;
            };
            let undeclared = sent
                .keys()
                .filter(|name| !declares(properties, name))
                .collect::<Vec<_>>();
            if undeclared.is_empty() {
                for literal in literals {
                    if !admitting.iter().any(|known| equivalent(known, literal)) {
                        admitting.push(literal);
                    }
                }
            } else {
                rejecting = true;
                for name in undeclared {
                    if !ruling_out.contains(&name.as_str()) {
                        ruling_out.push(name);
                    }
                }
            }
        }
        if admitting.is_empty() || !rejecting {
            continue;
        }
        let verb = if ruling_out.len() == 1 {
            "requires"
        } else {
            "require"
        };
        item.message = format!(
            "{} ({} {verb} {})",
            item.message,
            ruling_out.join(", "),
            render_selector(field, &admitting)
        );
    }
}
