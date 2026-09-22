//! Canonical union branch selection, matching validation/unionIssues.ts.
use super::schema::validate_schema;
use super::{ContractValidationError, ValidationIssue, issue};
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
            // Preserve the original unknown-field path and knownFields schema.
            // The stable error projector uses both to produce an actionable
            // spelling suggestion; grouping them here discards that context.
            issues: non_aborted[0].clone(),
        });
    }
    // Each branch pins its selector fields to distinct literals, so any single
    // branch's const/enum issue names only that branch's value(s). Collect the
    // allowed literals for every field across all branches before one branch is
    // chosen, so the surfaced error can list the full set instead of one
    // arbitrary literal (parity: validation/unionIssues.ts).
    let allowed = aggregate_allowed_literals(&failures);
    let Some(mut selected) = failures
        .into_iter()
        .min_by_key(|issues| score(issues, path.len()))
    else {
        return Err(issue(
            "schema.union",
            path.to_vec(),
            "Input matches multiple exclusive schema branches",
        ));
    };
    widen_literal_issues(&mut selected, &allowed);
    // Branch scoring may group key errors for parity, but the selected branch
    // must retain individual paths and schemas for precise diagnostics.
    Err(ContractValidationError { issues: selected })
}

/// Collects the allowed literal values for each field path across every failed
/// branch. A branch reports one `schema.const` (a single literal) or
/// `schema.enum` (a set) per selector; their union is the field's true allowed
/// set, which no single branch's issue can name on its own.
fn aggregate_allowed_literals(failures: &[Vec<ValidationIssue>]) -> Vec<(Vec<String>, Vec<Value>)> {
    let mut allowed: Vec<(Vec<String>, Vec<Value>)> = Vec::new();
    for issues in failures {
        for item in issues {
            let literals: Vec<Value> = match item.rule_id.as_str() {
                "schema.const" => item
                    .schema
                    .as_ref()
                    .and_then(|schema| schema.get("const"))
                    .cloned()
                    .into_iter()
                    .collect(),
                "schema.enum" => item
                    .schema
                    .as_ref()
                    .and_then(|schema| schema.get("enum"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
                _ => continue,
            };
            if literals.is_empty() {
                continue;
            }
            let entry =
                if let Some(existing) = allowed.iter_mut().find(|(field, _)| field == &item.path) {
                    existing
                } else {
                    allowed.push((item.path.clone(), Vec::new()));
                    allowed.last_mut().expect("entry just pushed")
                };
            for literal in literals {
                if !entry.1.contains(&literal) {
                    entry.1.push(literal);
                }
            }
        }
    }
    allowed
}

/// Rewrites the selected branch's single-literal `schema.const` (or partial
/// `schema.enum`) issues into an enum-shaped issue listing every allowed value
/// for that field, so a wrong selector is self-correcting instead of naming one
/// arbitrary branch's literal. Only widens when the aggregated set has more than
/// one value; a genuinely single-valued field keeps its precise const message.
fn widen_literal_issues(issues: &mut [ValidationIssue], allowed: &[(Vec<String>, Vec<Value>)]) {
    for item in issues {
        if !matches!(item.rule_id.as_str(), "schema.const" | "schema.enum") {
            continue;
        }
        let Some((_, literals)) = allowed.iter().find(|(field, _)| field == &item.path) else {
            continue;
        };
        if literals.len() < 2 {
            continue;
        }
        let rendered = literals
            .iter()
            .map(|literal| {
                literal
                    .as_str()
                    .map_or_else(|| literal.to_string(), str::to_owned)
            })
            .collect::<Vec<_>>()
            .join(", ");
        item.rule_id = "schema.enum".into();
        item.message = format!("Value is outside the allowed enum; allowed: {rendered}");
        item.schema = Some(serde_json::json!({ "enum": literals }));
    }
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
