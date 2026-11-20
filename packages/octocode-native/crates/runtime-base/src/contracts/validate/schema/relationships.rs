use super::super::{ContractValidationError, ValidationIssue, internal, issue};
use serde_json::Value;
use std::cmp::Ordering;

pub(super) fn strings(value: &Value) -> Result<Vec<&str>, ContractValidationError> {
    value
        .as_array()
        .ok_or_else(|| internal("Expected an array of field names".into()))?
        .iter()
        .map(|part| {
            part.as_str()
                .ok_or_else(|| internal("Expected a field name".into()))
        })
        .collect()
}

fn diagnostic(
    annotation: &Value,
    path: &[String],
    default_path: &[&str],
) -> Result<ContractValidationError, ContractValidationError> {
    let mut at = path.to_vec();
    at.extend(
        match annotation.get("path") {
            Some(parts) => strings(parts)?,
            None => default_path.to_vec(),
        }
        .into_iter()
        .map(str::to_owned),
    );
    Ok(issue(
        annotation["id"].as_str().unwrap_or("schema.order"),
        at,
        annotation["message"]
            .as_str()
            .unwrap_or("Fields do not satisfy the declared relationship"),
    ))
}

pub(super) fn validate(
    schema: &Value,
    value: &Value,
    path: &[String],
) -> Result<(), ContractValidationError> {
    if let Some(order) = schema.get("x-order") {
        validate_order(order, value, path)?;
    }
    if let Some(sum) = schema.get("x-sum") {
        let total = sum["total"]
            .as_str()
            .ok_or_else(|| internal("x-sum requires total".into()))?;
        let expected = strings(&sum["terms"])?
            .into_iter()
            .try_fold(0.0, |sum, name| {
                value
                    .get(name)
                    .map_or(Some(0.0), Value::as_f64)
                    .map(|term| sum + term)
            });
        if expected.is_none() || expected != value.get(total).and_then(Value::as_f64) {
            return Err(diagnostic(sum, path, &[total])?);
        }
    }
    if let Some(tokens) = schema.get("x-numericTokens") {
        validate_tokens(tokens, value, path)?;
    }
    if let Some(tree) = schema.get("x-tree") {
        validate_tree(tree, value, path)?;
    }
    if let Some(rule) = schema.get("x-toolQuery") {
        validate_tool_query(rule, value, path)?;
    }
    Ok(())
}

/// Another tool's query row, named by a sibling field (a clasify `source:
/// {tool, query}`): checked against that tool's own contract, as the source
/// reader checks it when it runs, so admission refuses what capture would.
/// It is checked as a fresh row, whoever sends it: the tool's private
/// progress (its x-cursor fields) is never part of it, since only an issued
/// cursor carries that progress, beside the row (clasify `sourceProgress`).
/// Each issue keeps the tool's own path and message, then names the fix.
fn validate_tool_query(
    rule: &Value,
    value: &Value,
    path: &[String],
) -> Result<(), ContractValidationError> {
    let field = |name: &str| {
        rule[name]
            .as_str()
            .ok_or_else(|| internal(format!("x-toolQuery requires {name}")))
    };
    let (tool_field, query_field) = (field("tool")?, field("query")?);
    let (Some(tool), Some(query)) = (
        value.get(tool_field).and_then(Value::as_str),
        value.get(query_field),
    ) else {
        return Ok(());
    };
    let Err(error) = super::super::validate_query(tool, query.clone()) else {
        return Ok(());
    };
    if error.is_contract_error() {
        return Err(error);
    }
    let id = field("id")?;
    Err(ContractValidationError {
        issues: error
            .issues
            .into_iter()
            .flat_map(|inner| {
                let mut at = path.to_vec();
                at.push(query_field.to_owned());
                let message = nearest_known_field(&inner)
                    .unwrap_or_else(|| inner.message.trim_end_matches('.').to_owned());
                at.extend(inner.path);
                issue(
                    id,
                    at,
                    format!("{message}. query is the row a direct {tool} call takes."),
                )
                .issues
            })
            .collect(),
    })
}

/// An unknown field's message with the known field it most likely means
/// (`Unknown field: contextLine; use 'contextLines'`), as a direct call to
/// the tool words it; `None` for any other issue or nothing near.
fn nearest_known_field(inner: &ValidationIssue) -> Option<String> {
    if inner.rule_id != "schema.unknown-field" {
        return None;
    }
    let field = inner.path.last()?;
    let names = |key: &str| {
        inner
            .schema
            .as_ref()
            .and_then(|schema| schema[key].as_array())
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
    };
    let sent = names("sentFields");
    let known = names("knownFields");
    let near = super::super::repair::nearest(
        field,
        known.iter().copied().filter(|name| !sent.contains(name)),
    );
    let (last, rest) = near.split_last()?;
    let rest = rest
        .iter()
        .map(|name| format!("'{name}'"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(if rest.is_empty() {
        format!("Unknown field: {field}; use '{last}'")
    } else {
        format!("Unknown field: {field}; use {rest} or '{last}'")
    })
}

/// The `(tool, query)` field names of the embedded contract's `x-toolQuery`
/// rule `id` (a clasify `source: {tool, query}`): the repair owner reads a
/// nested query's issues by them ([`crate::contracts::normalize`]).
pub(crate) fn tool_query_fields(id: &str) -> Option<(&'static str, &'static str)> {
    static RULES: std::sync::OnceLock<Vec<(&'static str, &'static str, &'static str)>> =
        std::sync::OnceLock::new();
    fn collect(value: &'static Value, rules: &mut Vec<(&'static str, &'static str, &'static str)>) {
        match value {
            Value::Object(fields) => {
                if let Some(rule) = fields.get("x-toolQuery")
                    && let (Some(id), Some(tool), Some(query)) = (
                        rule["id"].as_str(),
                        rule["tool"].as_str(),
                        rule["query"].as_str(),
                    )
                    && !rules.iter().any(|(known, ..)| *known == id)
                {
                    rules.push((id, tool, query));
                }
                for value in fields.values() {
                    collect(value, rules);
                }
            }
            Value::Array(items) => {
                for value in items {
                    collect(value, rules);
                }
            }
            _ => {}
        }
    }
    RULES
        .get_or_init(|| {
            let mut rules = Vec::new();
            if let Ok(contract) = crate::contracts::parsed_contract() {
                collect(contract, &mut rules);
            }
            rules
        })
        .iter()
        .find(|(known, ..)| *known == id)
        .map(|(_, tool, query)| (*tool, *query))
}

fn validate_order(
    order: &Value,
    value: &Value,
    path: &[String],
) -> Result<(), ContractValidationError> {
    let lower = order["lower"]
        .as_str()
        .ok_or_else(|| internal("x-order requires lower".into()))?;
    let upper = order["upper"]
        .as_str()
        .ok_or_else(|| internal("x-order requires upper".into()))?;
    let (Some(left), Some(right)) = (value.get(lower), value.get(upper)) else {
        return Ok(());
    };
    let comparison = match order["kind"].as_str() {
        Some("number") => left
            .as_f64()
            .zip(right.as_f64())
            .and_then(|(a, b)| a.partial_cmp(&b)),
        Some("timestamp") => left
            .as_str()
            .and_then(crate::civil_date::rfc3339_millis)
            .zip(right.as_str().and_then(crate::civil_date::rfc3339_millis))
            .map(|(a, b)| a.cmp(&b)),
        Some("lexicographic") => {
            let mut result = Some(Ordering::Equal);
            for key in strings(&order["keys"])? {
                if result != Some(Ordering::Equal) {
                    break;
                }
                if let (Some(a), Some(b)) = (left.get(key), right.get(key)) {
                    result = a
                        .as_f64()
                        .zip(b.as_f64())
                        .and_then(|(a, b)| a.partial_cmp(&b));
                }
            }
            result
        }
        _ => return Err(internal("Unsupported x-order kind".into())),
    };
    if comparison == Some(Ordering::Less)
        || (comparison == Some(Ordering::Equal) && order["exclusive"] != true)
    {
        Ok(())
    } else {
        Err(diagnostic(order, path, &[upper])?)
    }
}

fn descendants<'a>(
    value: &'a Value,
    segments: &[&str],
    path: Vec<String>,
    found: &mut Vec<(&'a Value, Vec<String>)>,
) {
    let Some((head, tail)) = segments.split_first() else {
        if value.is_object() {
            found.push((value, path));
        }
        return;
    };
    if *head == "*" {
        if let Some(items) = value.as_array() {
            for (index, item) in items.iter().enumerate() {
                let mut at = path.clone();
                at.push(index.to_string());
                descendants(item, tail, at, found);
            }
        }
    } else if let Some(child) = value.get(*head) {
        let mut at = path;
        at.push((*head).to_owned());
        descendants(child, tail, at, found);
    }
}

fn validate_tree(
    tree: &Value,
    value: &Value,
    path: &[String],
) -> Result<(), ContractValidationError> {
    let maximum = tree["maxDepth"]
        .as_u64()
        .ok_or_else(|| internal("x-tree requires maxDepth".into()))?;
    let children: Vec<_> = tree["children"]
        .as_array()
        .ok_or_else(|| internal("x-tree requires children".into()))?
        .iter()
        .map(strings)
        .collect::<Result<_, _>>()?;
    let mut pending = vec![(value, path.to_vec(), 0)];
    while let Some((value, at, depth)) = pending.pop() {
        if depth > maximum {
            return Err(diagnostic(tree, &at, &[])?);
        }
        for child_path in &children {
            let mut found = Vec::new();
            descendants(value, child_path, at.clone(), &mut found);
            pending.extend(found.into_iter().map(|(child, at)| (child, at, depth + 1)));
        }
    }
    Ok(())
}

fn validate_tokens(
    tokens: &Value,
    value: &Value,
    path: &[String],
) -> Result<(), ContractValidationError> {
    let Some(text) = value.as_str() else {
        return Ok(());
    };
    let ignore = super::compiled_pattern(
        tokens["ignore"]
            .as_str()
            .ok_or_else(|| internal("x-numericTokens requires ignore".into()))?,
    )
    .map_err(internal)?;
    let pattern = super::compiled_pattern(
        tokens["pattern"]
            .as_str()
            .ok_or_else(|| internal("x-numericTokens requires pattern".into()))?,
    )
    .map_err(internal)?;
    let mut compact = String::new();
    let mut end = 0;
    for matched in ignore.find_iter(text) {
        compact.push_str(&text[end..matched.start()]);
        end = matched.end();
    }
    compact.push_str(&text[end..]);
    let minimum = tokens["minimum"]
        .as_f64()
        .ok_or_else(|| internal("x-numericTokens requires minimum".into()))?;
    let maximum = tokens["maximum"]
        .as_f64()
        .ok_or_else(|| internal("x-numericTokens requires maximum".into()))?;
    for matched in pattern.find_iter(&compact) {
        let valid = compact[matched.range]
            .parse::<f64>()
            .is_ok_and(|number| number.is_finite() && number >= minimum && number <= maximum);
        if !valid {
            return Err(diagnostic(tokens, path, &[])?);
        }
    }
    Ok(())
}
