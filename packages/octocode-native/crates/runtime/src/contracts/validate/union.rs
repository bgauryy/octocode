//! Canonical union branch selection, matching validation/unionIssues.ts.
use super::schema::validate_schema;
use super::{ContractValidationError, ValidationIssue, issue};
use serde_json::Value;
use std::cell::Cell;

const SELECTORS: &[&str] = &[
    "operation",
    "type",
    "ecosystem",
    "resultView",
    "fullContent",
    "matchString",
    "ranges",
    "contextBytes",
    "regex",
    "caseMode",
    "packageName",
    "keywords",
];

thread_local! {
    /// Depth of speculative validation on this thread: a branch tried only
    /// for acceptance, whose issues nobody reads.
    static SPECULATIVE: Cell<u32> = const { Cell::new(0) };
}

/// True while validating a value whose issues are discarded (a union branch
/// tried for acceptance, a `not` probe). Only pass/fail matters there, so
/// issue construction may skip copying schemas and received values, and a
/// failing union skips branch scoring.
pub(super) fn speculative() -> bool {
    SPECULATIVE.with(Cell::get) > 0
}

/// Marks the current thread speculative until dropped (panic-safe).
pub(super) struct Speculation;

impl Speculation {
    pub(super) fn enter() -> Self {
        SPECULATIVE.with(|depth| depth.set(depth.get() + 1));
        Self
    }
}

impl Drop for Speculation {
    fn drop(&mut self) {
        SPECULATIVE.with(|depth| depth.set(depth.get() - 1));
    }
}

#[cfg(test)]
thread_local! {
    /// Differential tests switch the fast acceptance path off to compare
    /// against the reference algorithm (every branch, full issues).
    static REFERENCE_ONLY: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn validate(
    root: &Value,
    branches: &[Value],
    exclusive: bool,
    value: &mut Value,
    path: &[String],
) -> Result<(), ContractValidationError> {
    #[cfg(test)]
    if REFERENCE_ONLY.with(Cell::get) {
        return reference(root, branches, exclusive, value, path);
    }
    if accept(root, branches, exclusive, value, path) {
        return Ok(());
    }
    if speculative() {
        // A speculative caller reads only pass/fail; the scored diagnosis
        // runs once, when a non-speculative validation fails.
        return Err(issue(
            "schema.union",
            path.to_vec(),
            "No schema branch accepts the value",
        ));
    }
    reference(root, branches, exclusive, value, path)
}

/// The acceptance `reference` reaches, without its failure diagnosis.
/// Branches that certainly reject the value are skipped (`reference` fails
/// them too); the rest validate speculatively on copies. On success the value
/// becomes the first accepting branch's normalized copy, as in `reference`.
fn accept(
    root: &Value,
    branches: &[Value],
    exclusive: bool,
    value: &mut Value,
    path: &[String],
) -> bool {
    let _speculation = Speculation::enter();
    let mut accepted = None;
    for branch in branches
        .iter()
        .filter(|branch| !certainly_rejects(root, branch, value))
    {
        let mut candidate = value.clone();
        if validate_schema(root, branch, &mut candidate, &mut path.to_vec()).is_err() {
            continue;
        }
        if accepted.is_some() {
            // A second match fails an exclusive union.
            return false;
        }
        if !exclusive {
            *value = candidate;
            return true;
        }
        accepted = Some(candidate);
    }
    match accepted {
        Some(candidate) => {
            *value = candidate;
            true
        }
        None => false,
    }
}

/// True when the branch certainly rejects the value: it is a plain object
/// schema that requires a field the value lacks and gives no default for, or
/// whose property pins a field the value carries to other literals (`tool`,
/// `operation`, `ecosystem`). `validate_schema` checks both on every object,
/// so such a branch always fails.
fn certainly_rejects(root: &Value, branch: &Value, value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    let target = branch_object(root, branch);
    if target.get("type").and_then(Value::as_str) != Some("object")
        || ["$ref", "oneOf", "anyOf"]
            .iter()
            .any(|keyword| target.get(*keyword).is_some())
    {
        return false;
    }
    let properties = target.get("properties").and_then(Value::as_object);
    let missing_required = target
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .any(|name| {
            !object.contains_key(name)
                && properties
                    .and_then(|properties| properties.get(name))
                    .is_none_or(|schema| schema.get("default").is_none())
        });
    if missing_required {
        return true;
    }
    let Some(properties) = properties else {
        return false;
    };
    properties.iter().any(|(name, schema)| {
        let Some(field) = object.get(name) else {
            return false;
        };
        if ["$ref", "oneOf", "anyOf"]
            .iter()
            .any(|keyword| schema.get(*keyword).is_some())
        {
            return false;
        }
        if let Some(constant) = schema.get("const") {
            return field != constant;
        }
        schema
            .get("enum")
            .and_then(Value::as_array)
            .is_some_and(|values| !values.contains(field))
    })
}

/// Canonical selection: validate every branch, then accept a sole match or
/// diagnose the closest failing branch.
fn reference(
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
    // allowed literals for every field across the branches the value selects
    // (all branches when its selectors match none) before one branch is
    // chosen, so the surfaced error lists the full set for the chosen form,
    // never another form's literals (parity: validation/unionIssues.ts).
    let allowed = aggregate_allowed_literals(&selected_failures(&failures, path));
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
    annotate_sibling_branch_fields(&mut selected, root, branches, value);
    annotate_sibling_selectors(&mut selected, root, branches, value, path);
    annotate_forbidden_fields(&mut selected, root, branches, value, path);
    // Branch scoring may group key errors for parity, but the selected branch
    // must retain individual paths and schemas for precise diagnostics.
    Err(ContractValidationError { issues: selected })
}

/// The failed branches whose selector fields (`operation`, `ecosystem`, …) all
/// accept the value: their enum issues are the value's own. When no branch's
/// selectors match, every branch competes and all of them count.
fn selected_failures(
    failures: &[Vec<ValidationIssue>],
    path: &[String],
) -> Vec<Vec<ValidationIssue>> {
    let selector_mismatch = |item: &ValidationIssue| {
        matches!(item.rule_id.as_str(), "schema.const" | "schema.enum")
            && item.path.len() == path.len() + 1
            && item.path.starts_with(path)
            && item
                .path
                .last()
                .is_some_and(|field| SELECTORS.contains(&field.as_str()))
    };
    let matched = failures
        .iter()
        .filter(|issues| !issues.iter().any(selector_mismatch))
        .cloned()
        .collect::<Vec<_>>();
    if matched.is_empty() {
        failures.to_vec()
    } else {
        matched
    }
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
        invalid(&["tool", "operation", "type", "ecosystem", "questionType"]),
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
/// `review` on a commit query points at `operation:"pullRequest"` and
/// `keywords` on a pypi query at every `type` that accepts it.
fn sibling_selector_values(declaring: &[&Value], value: &Value) -> Option<Vec<Value>> {
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
                (!literals.contains(&value.get(name)?)).then_some((name, literals))
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
                    if !merged[index].1.contains(literal) {
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

/// The literal values a `const` or `enum` selector accepts.
fn selector_literals(schema: &Value) -> Option<Vec<&Value>> {
    if let Some(literal) = schema.get("const") {
        return Some(vec![literal]);
    }
    schema
        .get("enum")
        .and_then(Value::as_array)
        .map(|values| values.iter().collect())
}

fn render_selector<V: std::borrow::Borrow<Value>>(name: &str, literals: &[V]) -> String {
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
    use crate::contracts::prepare_many_and_validate;
    use serde_json::{Value, json};

    /// Runs `check` with the fast acceptance path and again with only the
    /// reference algorithm, and returns both results.
    fn both<T>(check: impl Fn() -> T) -> (T, T) {
        let fast = check();
        super::REFERENCE_ONLY.with(|flag| flag.set(true));
        let reference = check();
        super::REFERENCE_ONLY.with(|flag| flag.set(false));
        (fast, reference)
    }

    /// Output validation as `validate_output` runs it, keeping the
    /// normalized copy so defaults chosen by union branches compare too.
    fn validate_output_normalized(
        tool: &str,
        output: &Value,
    ) -> Result<Value, super::ContractValidationError> {
        let schema = &super::super::contract_tool(tool)?["outputSchema"];
        let mut candidate = output.clone();
        crate::contracts::shared_fields::restore(&mut candidate);
        super::validate_schema(schema, schema, &mut candidate, &mut Vec::new())?;
        Ok(candidate)
    }

    /// B10: the copy-free validation the response stage runs returns what
    /// `validate_output` returns and leaves the response byte-identical.
    fn assert_in_place_matches(tool: &str, output: &Value) {
        let mut in_place = output.clone();
        assert_eq!(
            crate::contracts::validate_output_in_place(tool, &mut in_place),
            crate::contracts::validate_output(tool, output),
            "{tool}: {output}"
        );
        assert_eq!(
            serde_json::to_string(&in_place).expect("json"),
            serde_json::to_string(output).expect("json"),
            "{tool}"
        );
    }

    fn fixtures() -> Vec<Value> {
        let fixtures: Value =
            serde_json::from_str(crate::contracts::generated::CONTRACT_FIXTURES_JSON)
                .expect("generated fixtures");
        fixtures.as_array().expect("fixture array").clone()
    }

    /// Each fixture query, then one variant per field: removed, a boolean,
    /// an unknown literal, an object; plus an unknown field. The variants
    /// reach every union failure diagnosis (selector mismatch, missing field,
    /// mixed forms, widened literals, sibling annotations).
    fn query_variants(query: &Value) -> Vec<Value> {
        let mut variants = vec![query.clone()];
        let Some(object) = query.as_object() else {
            return variants;
        };
        for key in object.keys() {
            let mut removed = object.clone();
            removed.remove(key);
            variants.push(Value::Object(removed));
            for replacement in [json!(true), json!("zz-not-a-value"), json!({}), json!(7)] {
                let mut changed = object.clone();
                changed.insert(key.clone(), replacement);
                variants.push(Value::Object(changed));
            }
        }
        let mut unknown = object.clone();
        unknown.insert("bogusField".into(), json!(1));
        variants.push(Value::Object(unknown));
        variants
    }

    /// B20: the discriminator-first acceptance path returns exactly what
    /// the reference algorithm returns (same normalized value, same issues)
    /// for every generated fixture and its field variants, as input.
    #[test]
    fn fast_union_acceptance_matches_the_reference_on_every_input_fixture() {
        let mut compared = 0;
        let mut rejected = 0;
        for fixture in fixtures() {
            let tool = fixture["tool"].as_str().expect("tool");
            let Some(query) = fixture["input"]["queries"].get(0) else {
                continue;
            };
            for variant in query_variants(query) {
                let input = json!({"queries":[variant]});
                let (fast, reference) = both(|| prepare_many_and_validate(tool, input.clone()));
                assert_eq!(fast, reference, "{}: {input}", fixture["id"]);
                compared += 1;
                rejected += usize::from(fast.is_err());
            }
        }
        assert!(compared > 1_000 && rejected > 500, "{compared}/{rejected}");
    }

    /// B20: the same on output envelopes, where a row's continuation is the
    /// 16-branch `ExecutableContinuation` union: every fixture query and a
    /// sixth of its variants as a continuation in its tool's output, some
    /// under a wrong tool name; each tool's first query in every output.
    #[test]
    fn fast_union_acceptance_matches_the_reference_on_continuation_outputs() {
        let tools: Vec<&str> = crate::contracts::parsed_contract().expect("contract")["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        let envelope = |continuation: Value| {
            json!({"results":[
                {"index":0,"data":{"path":"a.txt","content":"x\n","totalLines":1,
                    "next":{"continue":continuation.clone()}}},
                {"index":1,"status":"error","data":{"error":"Not found","errorCode":"notFound",
                    "hints":{"retry":continuation}}}
            ]})
        };
        let mut compared = 0;
        let mut rejected = 0;
        let mut compare = |output_tool: &str, output: &Value| {
            let (fast, reference) = both(|| validate_output_normalized(output_tool, output));
            assert_eq!(fast, reference, "{output_tool}: {output}");
            assert_in_place_matches(output_tool, output);
            compared += 1;
            rejected += usize::from(fast.is_err());
        };
        let mut seen = Vec::new();
        for fixture in fixtures() {
            let tool = fixture["tool"].as_str().expect("tool").to_owned();
            let Some(query) = fixture["input"]["queries"].get(0) else {
                continue;
            };
            let unchanged = envelope(json!({"tool":tool,"query":{"queries":[query]}}));
            let output_tools = if seen.contains(&tool) {
                vec![tool.as_str()]
            } else {
                tools.clone()
            };
            for output_tool in output_tools {
                compare(output_tool, &unchanged);
            }
            for (index, variant) in query_variants(query).into_iter().enumerate().step_by(6) {
                let named = if index % 7 == 6 { "localFetch" } else { &tool };
                let output = envelope(json!({"tool":named,"query":{"queries":[variant]}}));
                compare(&tool, &output);
            }
            seen.push(tool);
        }
        eprintln!("compared {compared} outputs, {rejected} rejected");
        assert!(compared > 600 && rejected > 200, "{compared}/{rejected}");
    }

    /// B20/B10: recorded responses (`OCTOCODE_RECORDED_OUTPUTS`: a directory
    /// of `<n>-<tool>-<surface>.json` envelopes captured from the live tools)
    /// validate identically on both paths and in place, unchanged and with
    /// each row's fields individually corrupted.
    #[test]
    #[ignore = "needs a recorded corpus: OCTOCODE_RECORDED_OUTPUTS=<dir>"]
    fn fast_union_acceptance_matches_the_reference_on_recorded_outputs() {
        let dir = std::env::var("OCTOCODE_RECORDED_OUTPUTS").expect("OCTOCODE_RECORDED_OUTPUTS");
        let mut compared = 0;
        for entry in std::fs::read_dir(dir).expect("corpus dir") {
            let path = entry.expect("entry").path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .expect("name")
                .to_owned();
            let Some(tool) = name.split('-').nth(1) else {
                continue;
            };
            let output: Value =
                serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
            let output = output.get("structuredContent").cloned().unwrap_or(output);
            if output.get("thrown").is_some() {
                continue;
            }
            let mut variants = vec![output.clone()];
            for (row_index, row) in output["results"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                for field in row["data"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(k, _)| k)
                {
                    for replacement in [json!(true), json!({"tool":"nope"}), json!([1])] {
                        let mut changed = output.clone();
                        changed["results"][row_index]["data"][field] = replacement;
                        variants.push(changed);
                    }
                }
            }
            for variant in variants {
                let (fast, reference) = both(|| validate_output_normalized(tool, &variant));
                assert_eq!(fast, reference, "{name}");
                assert_in_place_matches(tool, &variant);
                compared += 1;
            }
        }
        assert!(compared > 0);
        eprintln!("compared {compared} recorded outputs and variants");
    }

    #[test]
    fn clasify_question_selectors_report_the_selected_forms_missing_field() {
        for (question, field) in [
            (json!({"type":"adds","ask":"Retry safety"}), "known"),
            (json!({"type":"relevant"}), "ask"),
            (json!({"type":"choice","ask":"Choose a label"}), "labels"),
        ] {
            let error = prepare_many_and_validate(
                "clasify",
                json!({"queries":[{
                    "id":"missing-question-field",
                    "mainGoal": "test", "reasoning":"Check the selected question.",
                    "resources":[{"id":"held","value":"Observed evidence"}],
                    "questions":[question]
                }]}),
            )
            .expect_err("the selected question lacks a required field");
            assert_eq!(error.issues.len(), 1, "{error:?}");
            assert_eq!(
                error.issues[0].path,
                ["queries", "0", "questions", "0", field]
            );
            assert_eq!(error.issues[0].rule_id, "schema.required");
        }
    }

    /// A field that only another form's selector values accept names those
    /// values, not "unknown field".
    #[test]
    fn a_field_of_another_selector_value_names_the_values_that_accept_it() {
        let error = prepare_many_and_validate(
            "artifactSearch",
            json!({"queries":[{"ecosystem":"pypi","keywords":["http"]}]}),
        )
        .expect_err("pypi has no keyword discovery");
        let projected = crate::contracts::format_input_error("artifactSearch", &error, true);
        let details = projected["details"].to_string();
        assert!(
            details.contains(
                "Remove 'keywords' from queries[0]: it applies only with ecosystem one of \
                 \\\"npm\\\", \\\"crates\\\""
            ),
            "{projected}"
        );
        assert!(!details.contains("Unknown field"), "{projected}");
    }

    /// HI5: an enum error lists the values of the branch the selector
    /// chose, not every branch's merged enum (compare has no issue sections).
    #[test]
    fn enum_error_lists_only_the_selected_branch_values() {
        let error = prepare_many_and_validate(
            "ghGetHistoryItem",
            json!({"queries":[{
                "mainGoal":"g","reasoning":"r","operation":"compare",
                "owner":"o","repo":"r","base":"v1","head":"v2","sections":["bogus"]
            }]}),
        )
        .expect_err("bogus section");
        let message = error
            .issues
            .iter()
            .map(|issue| issue.message.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(message.contains("patches"), "{message}");
        for foreign in ["body", "comments", "reviews"] {
            assert!(!message.contains(foreign), "{foreign}: {message}");
        }
        // A selector that matches no branch still lists every branch's value.
        let error = prepare_many_and_validate(
            "ghGetHistoryItem",
            json!({"queries":[{"mainGoal":"g","reasoning":"r","operation":"bogus","owner":"o","repo":"r"}]}),
        )
        .expect_err("bogus operation");
        let message = error
            .issues
            .iter()
            .map(|issue| issue.message.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        for operation in ["pullRequest", "issue", "commit", "compare"] {
            assert!(message.contains(operation), "{operation}: {message}");
        }
    }

    #[test]
    fn forbidden_field_names_the_mutually_exclusive_fields() {
        let error = prepare_many_and_validate(
            "lspSearch",
            json!({"queries":[{
                "mainGoal":"g","reasoning":"r","operation":"documentSymbols",
                "path":"/tmp/a.rs","symbolName":"finish","lineHint":3
            }]}),
        )
        .expect_err("a document query forbids symbolName/lineHint");
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
