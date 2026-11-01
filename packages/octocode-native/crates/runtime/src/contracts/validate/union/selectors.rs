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
    declaring: &[&Value],
    value: &Value,
    field: &str,
) -> Option<Vec<Value>> {
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
            .filter(|(name, _)| name.as_str() != field)
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
/// A sibling whose selector rejects a value the row sent (a
/// `direction:"both"` form whose `operation` lacks `hubs`) is another form
/// of query, not a companion field this one is missing: it is skipped.
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
                    .map(|(name, schema)| sibling_requirement(name, schema, value))
                    .collect::<Option<Vec<_>>>()?
                    .into_iter()
                    .flatten()
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

/// What a sibling's property `name` asks of the row: `None` when it is a
/// selector that rejects the value the row sent, which rules the sibling
/// out; `Some(Some("name:literal"))` when the row omits a field the sibling
/// pins to one literal; `Some(None)` otherwise.
fn sibling_requirement(name: &str, schema: &Value, value: &Value) -> Option<Option<String>> {
    match value.get(name) {
        Some(actual) => {
            (selector_literals(schema).is_none() || literal_accepts(schema, actual)).then_some(None)
        }
        None => Some(
            single_literal(schema).map(|literal| format!("{name}:{}", render_literal(literal))),
        ),
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
            schema.get("not") != Some(&Value::Object(serde_json::Map::default()))
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
            let Some(literals) = properties.get(field).and_then(selector_literals) else {
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
