//! Canonical union branch selection, matching validation/unionIssues.ts.
use super::schema::validate_schema;
use super::{ContractValidationError, ValidationIssue, issue};
use serde_json::Value;

const SELECTORS: &[&str] = &[
    "operation",
    "analysis",
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
    if let Some(mixed) = mixed_forms(&failures, path) {
        return Err(mixed);
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
        annotate_forbidden_fields(&mut issues, root, branches, value, path);
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
    annotate_forbidden_fields(&mut selected, root, branches, value, path);
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

/// Two branches that each reject only the other's fields (e.g. a clasify
/// preset `questionType`+`target` sent together with a custom `type`+
/// `instructions`, or `context.value` with `tool`+`query`): the input mixes
/// forms. Name both field sets instead of calling one set "unknown".
fn mixed_forms(
    failures: &[Vec<ValidationIssue>],
    path: &[String],
) -> Option<ContractValidationError> {
    let unknown_sets = failures
        .iter()
        .filter_map(|issues| {
            let fields = issues
                .iter()
                .map(|item| {
                    (item.rule_id == "schema.unknown-field" && item.path.len() == path.len() + 1)
                        .then(|| item.path.last().cloned())
                        .flatten()
                })
                .collect::<Option<Vec<_>>>()?;
            (!fields.is_empty()).then_some(fields)
        })
        .collect::<Vec<_>>();
    for (index, left) in unknown_sets.iter().enumerate() {
        for right in &unknown_sets[index + 1..] {
            if left.iter().any(|field| right.contains(field)) {
                continue;
            }
            let quote = |fields: &[String]| {
                fields
                    .iter()
                    .map(|field| format!("`{field}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            return Some(ContractValidationError {
                issues: issue(
                    "schema.union-mixed",
                    path.to_vec(),
                    format!(
                        "{} and {} belong to different forms and cannot be combined; send one form",
                        quote(right),
                        quote(left)
                    ),
                )
                .issues,
            });
        }
    }
    None
}

fn score(issues: &[ValidationIssue], depth: usize) -> [usize; 4] {
    // A branch that does not declare a named discriminator (`tool`, `type`, …)
    // rejects it as strongly as a branch that pins it to other literals.
    let invalid = |names: &[&str]| {
        issues
            .iter()
            .filter(|issue| {
                matches!(
                    issue.rule_id.as_str(),
                    "schema.const" | "schema.enum" | "schema.unknown-field"
                ) && issue.path.len() == depth + 1
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
        // A continuation names its tool: a branch for another tool is never
        // the closest match, however few field issues it reports.
        invalid(&["tool", "operation", "type", "questionType"]),
        invalid(&["analysis", "resultView"]),
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
        let requires = sibling_missing_fields(&declaring, value, field)
            .or_else(|| sibling_selector_values(&declaring, value));
        if let (Some(requires), Some(schema)) = (
            requires,
            item.schema.as_mut().and_then(Value::as_object_mut),
        ) {
            schema.insert("siblingRequires".into(), Value::Array(requires));
        }
    }
}

/// The required fields each declaring branch still lacks, one alternative per
/// branch (`pattern or rule` for an astSearch match field), so the projector
/// names every form that accepts the field, not only the first.
fn sibling_missing_fields(declaring: &[&Value], value: &Value, field: &str) -> Option<Vec<Value>> {
    let mut options: Vec<String> = Vec::new();
    for branch in declaring {
        let missing = branch
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|required| *required != field && value.get(*required).is_none())
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

/// Branches chosen by a literal selector (e.g. `operation`) need no missing
/// field: name the selector values of every branch declaring the field, so
/// `review` on a commit query points at `operation:"pullRequest"`.
fn sibling_selector_values(declaring: &[&Value], value: &Value) -> Option<Vec<Value>> {
    let mut options = Vec::new();
    for branch in declaring {
        let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
            continue;
        };
        let selectors = properties
            .iter()
            .filter_map(|(name, schema)| {
                let literal = single_literal(schema)?;
                (value.get(name)? != literal).then(|| format!("{name}:{}", render_literal(literal)))
            })
            .collect::<Vec<_>>();
        if !selectors.is_empty() {
            let option = selectors.join(" and ");
            if !options.contains(&option) {
                options.push(option);
            }
        }
    }
    (!options.is_empty()).then(|| vec![Value::String(options.join(" or "))])
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

/// A `{"not":{}}` property forbids the field in its branch. When a sibling
/// branch allows that field but forbids fields the caller also sent, the
/// fields are mutually exclusive: name them instead of the bare `not` rule.
fn annotate_forbidden_fields(
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
            if properties.get(field).is_none_or(&forbids) {
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
                    "goal": "test", "reasoning":"Check the selected question.",
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

    #[test]
    fn forbidden_field_names_the_mutually_exclusive_fields() {
        let error = prepare_many_and_validate(
            "lspSearch",
            json!({"queries":[{
                "goal":"g","reasoning":"r","operation":"definition",
                "uri":"/tmp/a.rs","symbolName":"finish","lineHint":3,
                "position":{"line":3,"character":1}
            }]}),
            PrepareOptions::default(),
        )
        .expect_err("position conflicts with symbolName/lineHint");
        let message = error
            .issues
            .iter()
            .map(|issue| issue.message.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(!message.contains("forbidden schema"), "{message}");
        assert!(message.contains("mutually exclusive"), "{message}");
        assert!(message.contains("`symbolName`"), "{message}");
        assert!(message.contains("`lineHint`"), "{message}");
    }
}
