use super::super::{ContractValidationError, ValidationIssue, issue};
use serde_json::Value;

/// The required fields a failed branch lacks, when that is all it rejects
/// (no selector literal among them).
pub(super) fn only_missing(issues: &[ValidationIssue], depth: usize) -> Option<Vec<String>> {
    issues
        .iter()
        .map(|item| {
            (item.rule_id == "schema.required"
                && item.path.len() == depth + 1
                && !missing_literal(item, depth))
            .then(|| item.path.last().cloned())
            .flatten()
        })
        .collect::<Option<Vec<_>>>()
        .filter(|fields| !fields.is_empty())
}

/// A chosen branch that lacks only required fields, beside other forms of
/// the same mode that lack only others (`workspaceSymbol`: `path`, or
/// `workspaceRoot`): its first issue names those alternatives too.
pub(super) fn name_missing_alternatives(
    selected: &mut [ValidationIssue],
    missing_sets: &[Vec<String>],
    depth: usize,
) {
    let Some(missing) = only_missing(selected, depth) else {
        return;
    };
    let mut alternatives: Vec<String> = Vec::new();
    for set in missing_sets {
        if set.iter().any(|field| missing.contains(field)) {
            continue;
        }
        let option = set.join(" and ");
        if !alternatives.contains(&option) {
            alternatives.push(option);
        }
    }
    if alternatives.is_empty() {
        return;
    }
    let instead = match missing.as_slice() {
        [_] => String::new(),
        several => format!(" of {}", several.join(" and ")),
    };
    selected[0].message = format!(
        "{}; or send {} instead{instead}",
        selected[0].message,
        alternatives.join(" or ")
    );
}

/// The fields of a `oneOf` whose branches each only require one field
/// (`[{required:["state"]},{required:["source"]}]`): the value must carry
/// exactly one of them, whether it sent none or several.
pub(super) fn required_alternatives(branches: &[Value]) -> Option<Vec<&str>> {
    branches
        .iter()
        .map(|branch| {
            let branch = branch.as_object().filter(|branch| branch.len() == 1)?;
            match branch.get("required")?.as_array()?.as_slice() {
                [field] => field.as_str(),
                _ => None,
            }
        })
        .collect::<Option<Vec<_>>>()
        .filter(|fields| fields.len() > 1)
}

/// A missing required field that the schema pins to literals
/// (`operation:"models"`): core Zod reports it as `invalid_value`, a selector
/// rejection, not a missing value.
pub(super) fn missing_literal(issue: &ValidationIssue, depth: usize) -> bool {
    issue.rule_id == "schema.required"
        && issue.path.len() == depth + 1
        && issue
            .schema
            .as_ref()
            .is_some_and(|schema| schema.get("const").is_some() || schema.get("enum").is_some())
}

/// A literal issue on one of the union's `selectors` (fields that choose
/// between its branches, [`super::branches::selector_fields`]).
fn selector_mismatch(item: &ValidationIssue, depth: usize, selectors: &[String]) -> bool {
    matches!(item.rule_id.as_str(), "schema.const" | "schema.enum")
        && item.path.len() == depth + 1
        && item
            .path
            .last()
            .is_some_and(|field| selectors.contains(field))
}

/// The failed branches whose selector fields (`operation`, `ecosystem`, …) all
/// accept the value: their enum issues are the value's own. When no branch's
/// selectors match, every branch competes and all of them count.
pub(super) fn selected_failures<'a>(
    failures: &'a [Vec<ValidationIssue>],
    path: &[String],
    selectors: &[String],
) -> Vec<&'a [ValidationIssue]> {
    let matched = failures
        .iter()
        .filter(|issues| {
            !issues.iter().any(|item| {
                item.path.starts_with(path) && selector_mismatch(item, path.len(), selectors)
            })
        })
        .map(Vec::as_slice)
        .collect::<Vec<_>>();
    if matched.is_empty() {
        failures.iter().map(Vec::as_slice).collect()
    } else {
        matched
    }
}

/// One field's allowed literals across the failed branches, and the former
/// values their branches map onto them (`x-replacedValues`).
pub(super) struct AllowedLiterals {
    path: Vec<String>,
    literals: Vec<Value>,
    replaced: serde_json::Map<String, Value>,
}

/// Collects the allowed literal values for each field path across every failed
/// branch. A branch reports one `schema.const` (a single literal) or
/// `schema.enum` (a set) per selector; their union is the field's true allowed
/// set, which no single branch's issue can name on its own.
pub(super) fn aggregate_allowed_literals(failures: &[&[ValidationIssue]]) -> Vec<AllowedLiterals> {
    let mut allowed: Vec<AllowedLiterals> = Vec::new();
    for issues in failures {
        for item in *issues {
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
                .position(|field| field.path == item.path)
                .unwrap_or_else(|| {
                    allowed.push(AllowedLiterals {
                        path: item.path.clone(),
                        literals: Vec::new(),
                        replaced: serde_json::Map::new(),
                    });
                    allowed.len() - 1
                });
            let entry = &mut allowed[index];
            for literal in literals {
                if !entry.literals.contains(&literal) {
                    entry.literals.push(literal);
                }
            }
            if let Some(replaced) = item
                .schema
                .as_ref()
                .and_then(|schema| schema.get("x-replacedValues"))
                .and_then(Value::as_object)
            {
                entry.replaced.extend(
                    replaced
                        .iter()
                        .map(|(former, now)| (former.clone(), now.clone())),
                );
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
pub(super) fn widen_literal_issues(issues: &mut [ValidationIssue], allowed: &[AllowedLiterals]) {
    for item in issues {
        if !matches!(item.rule_id.as_str(), "schema.const" | "schema.enum") {
            continue;
        }
        let Some(field) = allowed.iter().find(|field| field.path == item.path) else {
            continue;
        };
        if field.literals.len() < 2 {
            continue;
        }
        let replaced = Value::Object(field.replaced.clone());
        item.rule_id = "schema.enum".into();
        item.message = super::super::literals::enum_message(
            item.received.as_ref(),
            &field.literals,
            Some(&replaced),
        );
        let mut schema = serde_json::json!({ "enum": field.literals });
        if !field.replaced.is_empty() {
            schema["x-replacedValues"] = replaced;
        }
        item.schema = Some(schema);
    }
}

/// Two branches that each reject only the other's fields (e.g. a clasify
/// preset `questionType`+`target` sent together with a custom `type`+
/// `instructions`, or `context.value` with `tool`+`query`): the input mixes
/// forms. Name both field sets instead of calling one set "unknown".
pub(super) fn mixed_forms(
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
            let mut issues = issue(
                "schema.union-mixed",
                path.to_vec(),
                format!(
                    "{} and {} belong to different forms and cannot be combined; send one form",
                    quote(right),
                    quote(left)
                ),
            )
            .issues;
            // The two forms' fields, for a message that knows which form a
            // request field holds alone (`cursor`).
            for item in &mut issues {
                item.schema = Some(serde_json::json!({"forms": [right, left]}));
            }
            return Some(ContractValidationError { issues });
        }
    }
    None
}

/// Honor explicit selectors before comparing the fields of failed branches;
/// among those, the branch that declares more of the value's fields
/// (`undeclared`, [`super::branches::undeclared_fields`]) describes it more
/// specifically than one that only lacks fewer.
pub(super) fn score(
    issues: &[ValidationIssue],
    depth: usize,
    selectors: &super::branches::Selectors,
    undeclared: usize,
) -> [usize; 8] {
    let at_field = |issue: &&ValidationIssue| issue.path.len() == depth + 1;
    [
        // A selector whose value names one form decides first: the value
        // is of that form, whatever another selector it fails there.
        issues
            .iter()
            .filter(|issue| selector_mismatch(issue, depth, selectors))
            .filter(|issue| {
                issue.path.last().is_some_and(|field| {
                    selectors.names_one(field)
                        || issue
                            .received
                            .as_ref()
                            .is_some_and(|sent| selectors.value_names_one(field, sent))
                })
            })
            .count(),
        issues
            .iter()
            .filter(|issue| selector_mismatch(issue, depth, selectors))
            .count(),
        issues
            .iter()
            .filter(at_field)
            .filter(|issue| issue.rule_id == "schema.unknown-field")
            .count(),
        issues
            .iter()
            .filter(|issue| missing_literal(issue, depth))
            .count(),
        issues
            .iter()
            .filter(at_field)
            .filter(|issue| issue.rule_id == "schema.not")
            .count(),
        undeclared,
        // A branch of another JSON type says only that: one of the value's own
        // type says what is wrong with it.
        issues
            .iter()
            .filter(|issue| issue.path.len() == depth && issue.rule_id == "schema.type")
            .count(),
        grouped_issue_count(issues),
    ]
}

/// How many issues a branch reports once its unknown fields are grouped the
/// way core Zod reports unrecognized keys, one issue per object: every other
/// issue counts once, and each parent with unknown fields counts once.
fn grouped_issue_count(issues: &[ValidationIssue]) -> usize {
    let mut parents: Vec<&[String]> = Vec::new();
    let mut count = 0;
    for item in issues {
        if item.rule_id != "schema.unknown-field" {
            count += 1;
            continue;
        }
        let parent = &item.path[..item.path.len().saturating_sub(1)];
        if !parents.contains(&parent) {
            parents.push(parent);
            count += 1;
        }
    }
    count
}
