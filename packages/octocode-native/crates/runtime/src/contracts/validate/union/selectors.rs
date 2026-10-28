use super::super::ValidationIssue;
use super::super::literals::equivalent;
use super::branch_object;
use serde_json::Value;

/// Branches chosen by a literal selector (e.g. `operation`) need no missing
/// field: name the selector values of every branch declaring the field, so
/// `review` on a commit query points at `operation:"pullRequest"` and
/// `keywords` on a pypi query at every `type` that accepts it.
pub(super) fn sibling_selector_values(declaring: &[&Value], value: &Value) -> Option<Vec<Value>> {
    // One mismatched selector per branch merges by name across branches;
    // a branch needing several selectors stays its own alternative.
    let mut merged: Vec<(String, Vec<Value>)> = Vec::new();
    let mut options = Vec::new();
    for branch in declaring {
        let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
            continue;
        };
        let selectors = properties
            .iter()
            .filter_map(|(name, schema)| {
                let literals = selector_literals(schema)?;
                let actual = value.get(name)?;
                (!literals.iter().any(|literal| equivalent(literal, actual)))
                    .then_some((name, literals))
            })
            .collect::<Vec<_>>();
        match selectors.as_slice() {
            [] => {}
            [(name, literals)] => {
                let index = merged
                    .iter()
                    .position(|(merged_name, _)| merged_name == *name)
                    .unwrap_or_else(|| {
                        merged.push(((*name).clone(), Vec::new()));
                        merged.len() - 1
                    });
                for literal in literals {
                    if !merged[index]
                        .1
                        .iter()
                        .any(|known| equivalent(known, literal))
                    {
                        merged[index].1.push((*literal).clone());
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
    (!rendered.is_empty()).then(|| vec![Value::String(rendered.join(" or "))])
}

/// Whether every literal selector of `branch` accepts the query's value.
pub(super) fn selectors_accept(branch: &Value, value: &Value) -> bool {
    branch
        .get("properties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .all(
            |(name, schema)| match (selector_literals(schema), value.get(name)) {
                (Some(literals), Some(actual)) => {
                    literals.iter().any(|literal| equivalent(literal, actual))
                }
                _ => true,
            },
        )
}

/// The literal values a `const` or `enum` selector accepts.
pub(super) fn selector_literals(schema: &Value) -> Option<Vec<&Value>> {
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

fn literal_accepts(schema: &Value, value: &Value) -> bool {
    schema
        .get("const")
        .is_some_and(|constant| equivalent(constant, value))
        || schema
            .get("enum")
            .and_then(Value::as_array)
            .is_some_and(|values| values.iter().any(|literal| equivalent(literal, value)))
}

pub(super) fn single_literal(schema: &Value) -> Option<&Value> {
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
        let requirement = branches
            .iter()
            .map(|branch| branch_object(root, branch))
            .find_map(|branch| {
                let properties = branch.get("properties")?.as_object()?;
                if !literal_accepts(properties.get(field)?, received) {
                    return None;
                }
                let selectors = properties
                    .iter()
                    .filter(|(name, _)| *name != field)
                    .filter_map(|(name, schema)| {
                        let literal = single_literal(schema)?;
                        (value.get(name) != Some(literal))
                            .then(|| format!("{name}:{}", render_literal(literal)))
                    })
                    .collect::<Vec<_>>();
                (!selectors.is_empty()).then(|| selectors.join(" and "))
            });
        if let Some(requirement) = requirement {
            item.message = format!(
                "{} ({field}:{} requires {requirement})",
                item.message,
                render_literal(received)
            );
        }
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
        schema
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
