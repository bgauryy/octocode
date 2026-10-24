use super::{ContractValidationError, ValidationIssue};
use crate::contracts::validate::union;
use serde_json::Value;

/// Where an unknown field sits: `queries[N]` (the row's 0-based output
/// `index`) for a bulk row, else the object path (`questions.0`), so a
/// root-level clasify matrix is not called a query row.
pub(super) fn query_label(path: &[String]) -> String {
    match path.first().map(String::as_str) {
        Some("queries") => {
            let index = path.get(1).map_or("0", String::as_str);
            if path.len() > 3 {
                format!("queries[{index}].{}", path[2..path.len() - 1].join("."))
            } else {
                format!("queries[{index}]")
            }
        }
        _ if path.len() > 1 => path[..path.len() - 1].join("."),
        _ => "the request".into(),
    }
}

/// `siblingRequires` is set by union validation when another branch declares
/// the unknown field; it names the fields that select that branch.
pub(super) fn sibling_requirement(issue: &ValidationIssue) -> Option<String> {
    let fields = issue
        .schema
        .as_ref()?
        .get("siblingRequires")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    (!fields.is_empty()).then(|| fields.join(" and "))
}

pub(super) fn issue(
    rule_id: impl Into<String>,
    path: Vec<String>,
    message: impl Into<String>,
) -> ContractValidationError {
    ContractValidationError {
        issues: vec![ValidationIssue {
            rule_id: rule_id.into(),
            path,
            message: message.into(),
            schema: None,
            received: None,
        }],
    }
}

pub(super) fn schema_issue(
    rule_id: impl Into<String>,
    path: Vec<String>,
    message: impl Into<String>,
    schema: &Value,
    received: &Value,
) -> ContractValidationError {
    let mut error = issue(rule_id, path, message);
    // A speculative probe reads only pass/fail: skip the copies.
    if !union::speculative() {
        error.issues[0].schema = Some(schema.clone());
        error.issues[0].received = Some(received.clone());
    }
    error
}

pub(super) fn internal(message: String) -> ContractValidationError {
    issue("contract.generated-json", vec![], message)
}

/// Classic Wagner-Fischer Levenshtein distance, O(m*n) time, O(min(m,n)) space.
pub(crate) fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let (a, b) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let mut prev: Vec<usize> = (0..=a.len()).collect();
    let mut curr = vec![0usize; a.len() + 1];
    for (j, cb) in b.iter().enumerate() {
        curr[0] = j + 1;
        for (i, ca) in a.iter().enumerate() {
            let cost = usize::from(ca != cb);
            curr[i + 1] = (prev[i + 1] + 1).min(curr[i] + 1).min(prev[i] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[a.len()]
}
