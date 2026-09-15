//! Canonical union branch selection, matching validation/unionIssues.ts.
use super::{ContractValidationError, ValidationIssue, issue, validate_schema};
use serde_json::Value;

const SELECTORS: &[&str] = &[
    "operation",
    "analysis",
    "treeKind",
    "type",
    "resultView",
    "fullContent",
    "matchString",
    "startLine",
    "endLine",
    "contextBytes",
    "matchStringIsRegex",
    "matchStringCaseSensitive",
    "packageName",
    "keywords",
];

pub(super) fn validate(
    root: &Value,
    branches: &[Value],
    exclusive: bool,
    value: &mut Value,
    path: &[String],
) -> Result<(), ContractValidationError> {
    let mut success = None;
    let mut successes = 0;
    let mut failures = Vec::new();
    for branch in branches {
        let mut candidate = value.clone();
        match validate_schema(root, branch, &mut candidate, &mut path.to_vec()) {
            Ok(()) => {
                successes += 1;
                if success.is_none() {
                    success = Some(candidate);
                }
            }
            Err(error) => failures.push(error.issues),
        }
    }
    if let Some(candidate) = success
        && (!exclusive || successes == 1)
    {
        *value = candidate;
        return Ok(());
    }
    // Zod 4.6.2 returns the sole non-aborted branch before constructing an
    // invalid_union issue. Shape-only key errors and ordinary checks continue;
    // missing values, invalid types and selectors abort a branch.
    let non_aborted = failures
        .iter()
        .filter(|issues| {
            issues.iter().all(|issue| {
                matches!(
                    issue.rule_id.as_str(),
                    "schema.unknown-field"
                        | "schema.union-keys"
                        | "schema.size"
                        | "schema.range"
                        | "schema.pattern"
                        | "schema.uri"
                )
            })
        })
        .collect::<Vec<_>>();
    if non_aborted.len() == 1 {
        return Err(ContractValidationError {
            issues: group_unknown_fields(non_aborted[0].clone()),
        });
    }
    let Some(selected) = failures
        .into_iter()
        .min_by_key(|issues| score(issues, path.len()))
    else {
        return Err(issue(
            "schema.union",
            path.to_vec(),
            "Input matches multiple exclusive schema branches",
        ));
    };
    let mut issues = group_unknown_fields(selected);
    for issue in &mut issues {
        if issue.rule_id == "schema.union-keys" {
            issue.rule_id = "schema.union-selected-keys".into();
        }
    }
    Err(ContractValidationError { issues })
}

fn score(issues: &[ValidationIssue], depth: usize) -> [usize; 4] {
    let invalid = |names: &[&str]| {
        issues
            .iter()
            .filter(|issue| {
                matches!(issue.rule_id.as_str(), "schema.const" | "schema.enum")
                    && issue
                        .path
                        .get(depth)
                        .is_some_and(|name| names.contains(&name.as_str()))
            })
            .count()
    };
    let rejected = issues
        .iter()
        .filter(|issue| {
            matches!(
                issue.rule_id.as_str(),
                "schema.const" | "schema.enum" | "schema.unknown-field"
            ) && issue
                .path
                .get(depth)
                .is_some_and(|name| SELECTORS.contains(&name.as_str()))
        })
        .count();
    let count = group_unknown_fields(issues.to_vec()).len();
    [
        invalid(&["operation", "type"]),
        invalid(&["analysis", "treeKind", "resultView"]),
        rejected,
        count,
    ]
}

fn group_unknown_fields(issues: Vec<ValidationIssue>) -> Vec<ValidationIssue> {
    let mut result = Vec::new();
    let mut consumed = vec![false; issues.len()];
    for (index, item) in issues.iter().enumerate() {
        if consumed[index] {
            continue;
        }
        if item.rule_id != "schema.unknown-field" {
            result.push(item.clone());
            continue;
        }
        let parent = &item.path[..item.path.len().saturating_sub(1)];
        let mut fields = Vec::new();
        for (other_index, other) in issues.iter().enumerate().skip(index) {
            if other.rule_id == "schema.unknown-field"
                && other.path[..other.path.len().saturating_sub(1)] == *parent
            {
                consumed[other_index] = true;
                if let Some(field) = other.path.last() {
                    fields.push(format!("\"{field}\""));
                }
            }
        }
        result.extend(
            issue(
                "schema.union-keys",
                parent.to_vec(),
                format!(
                    "Unrecognized key{}: {}",
                    if fields.len() == 1 { "" } else { "s" },
                    fields.join(", ")
                ),
            )
            .issues,
        );
    }
    result
}
