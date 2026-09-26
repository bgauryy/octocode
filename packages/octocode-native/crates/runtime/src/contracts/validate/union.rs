//! Canonical union branch selection, matching validation/unionIssues.ts.
use super::schema::validate_schema;
use super::{ContractValidationError, ValidationIssue, issue};
use serde_json::Value;

const SELECTORS: &[&str] = &[
    "operation",
    "analysis",
    "treeKind",
    "type",
    "questionType",
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
        let mut issues = non_aborted[0].clone();
        annotate_sibling_branch_fields(&mut issues, root, branches, value);
        return Err(ContractValidationError {
            // Preserve the original unknown-field path and knownFields schema.
            // The stable error projector uses both to produce an actionable
            // spelling suggestion; grouping them here discards that context.
            issues,
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
    annotate_sibling_selectors(&mut selected, root, branches, value, path);
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
            let index = allowed
                .iter()
                .position(|(field, _)| field == &item.path)
                .unwrap_or_else(|| {
                    allowed.push((item.path.clone(), Vec::new()));
                    allowed.len() - 1
                });
            let entry = &mut allowed[index];
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
        let hint = item
            .received
            .as_ref()
            .and_then(|received| super::schema::boolean_enum_hint(literals, received))
            .unwrap_or_default();
        item.message = format!("Value is outside the allowed enum; allowed: {rendered}{hint}");
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
        invalid(&["operation", "type", "questionType"]),
        invalid(&["analysis", "treeKind", "resultView"]),
        rejected,
        count,
    ]
}

fn branch_object<'a>(root: &'a Value, branch: &'a Value) -> &'a Value {
    branch
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| root.pointer(reference.strip_prefix('#').unwrap_or(reference)))
        .unwrap_or(branch)
}

/// An unknown field that a sibling branch declares is a mode mismatch, not a
/// typo. Record the sibling's missing required fields so the error projector
/// can say which mode the field belongs to (e.g. discovery-only `pageSize`).
fn annotate_sibling_branch_fields(
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
        let requires = branches
            .iter()
            .map(|branch| branch_object(root, branch))
            .find_map(|branch| {
                branch.get("properties")?.get(field)?;
                let missing = branch
                    .get("required")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .filter(|required| value.get(*required).is_none())
                    .map(|required| Value::String(required.to_owned()))
                    .collect::<Vec<_>>();
                (!missing.is_empty()).then_some(missing)
            });
        if let (Some(requires), Some(schema)) = (
            requires,
            item.schema.as_mut().and_then(Value::as_object_mut),
        ) {
            schema.insert("siblingRequires".into(), Value::Array(requires));
        }
    }
}

fn literal_accepts(schema: &Value, value: &Value) -> bool {
    schema.get("const") == Some(value)
        || schema
            .get("enum")
            .and_then(Value::as_array)
            .is_some_and(|values| values.contains(value))
}

fn single_literal(schema: &Value) -> Option<&Value> {
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
/// cross-field rule (e.g. `unique:"list"` needs `resultView:"matchOnly"`).
/// Name the sibling's selector values instead of only the local constant.
fn annotate_sibling_selectors(
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

#[cfg(test)]
mod tests {
    use crate::contracts::{PrepareOptions, prepare_many_and_validate};
    use serde_json::json;

    #[test]
    fn clasify_question_selectors_report_the_selected_forms_missing_field() {
        for (question, field) in [
            (
                json!({"questionType":"addsEvidence","target":"Retry safety"}),
                "knownEvidence",
            ),
            (json!({"questionType":"contribution"}), "target"),
            (
                json!({"type":"choice","instructions":"Choose a label"}),
                "criteria",
            ),
        ] {
            let error = prepare_many_and_validate(
                "clasify",
                json!({
                    "id":"missing-question-field",
                    "reasoning":"Check the selected question.",
                    "resources":[{"id":"held","context":{"value":"Observed evidence"}}],
                    "questions":[question]
                }),
                PrepareOptions::default(),
            )
            .expect_err("the selected question lacks a required field");
            assert_eq!(error.issues.len(), 1, "{error:?}");
            assert_eq!(error.issues[0].path, ["questions", "0", field]);
            assert_eq!(error.issues[0].rule_id, "schema.required");
        }
    }
}
