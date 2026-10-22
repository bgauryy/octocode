//! Canonical union branch selection, matching validation/unionIssues.ts.
#[cfg(test)]
use super::schema::validate_schema;
use super::schema::{ValidationMode, validate_with_mode};
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
    "regex",
    "caseMode",
    "packageName",
    "keywords",
];

/// Fields that name a branch outright: a branch rejecting one is never the
/// closest match, and their issues lead the chosen branch's errors.
const DISCRIMINATORS: &[&str] = &["tool", "operation", "type", "ecosystem"];

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
    mode: ValidationMode,
) -> Result<(), ContractValidationError> {
    #[cfg(test)]
    if REFERENCE_ONLY.with(Cell::get) {
        return reference(root, branches, exclusive, value, path, mode);
    }
    if accept(root, branches, exclusive, value, path, mode) {
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
    reference(root, branches, exclusive, value, path, mode)
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
    mode: ValidationMode,
) -> bool {
    let _speculation = Speculation::enter();
    let mut accepted = None;
    for branch in branches
        .iter()
        .filter(|branch| !certainly_rejects(root, branch, value))
    {
        let mut candidate = value.clone();
        if validate_with_mode(root, branch, &mut candidate, &mut path.to_vec(), mode).is_err() {
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
    mode: ValidationMode,
) -> Result<(), ContractValidationError> {
    let mut success = None;
    let mut successes = 0;
    let mut failures = Vec::new();
    for branch in branches {
        let mut candidate = value.clone();
        match validate_with_mode(root, branch, &mut candidate, &mut path.to_vec(), mode) {
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
    if exclusive && let Some(fields) = required_alternatives(branches) {
        return Err(issue(
            "schema.union",
            path.to_vec(),
            format!("Send exactly one of: {}.", fields.join(", ")),
        ));
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
                        | "schema.size"
                        | "schema.range"
                        | "schema.pattern"
                        | "schema.uri"
                        | "schema.repository-url"
                        | "schema.clone-suffix"
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
    // A wrong or missing selector is the first repair: the other issues are
    // relative to the branch it picked. The sort is stable.
    let depth = path.len();
    selected.sort_by_key(|item| {
        let leads = item.path.len() == depth + 1
            && (matches!(item.rule_id.as_str(), "schema.const" | "schema.enum")
                || missing_literal(item, depth))
            && item.path.get(depth).is_some_and(|name| {
                DISCRIMINATORS.contains(&name.as_str()) || SELECTORS.contains(&name.as_str())
            });
        !leads
    });
    // Branch scoring may group key errors for parity, but the selected branch
    // must retain individual paths and schemas for precise diagnostics.
    Err(ContractValidationError { issues: selected })
}

/// The fields of a `oneOf` whose branches each only require one field
/// (`[{required:["state"]},{required:["source"]}]`): the value must carry
/// exactly one of them, whether it sent none or several.
fn required_alternatives(branches: &[Value]) -> Option<Vec<&str>> {
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
fn missing_literal(issue: &ValidationIssue, depth: usize) -> bool {
    issue.rule_id == "schema.required"
        && issue.path.len() == depth + 1
        && issue
            .schema
            .as_ref()
            .is_some_and(|schema| schema.get("const").is_some() || schema.get("enum").is_some())
}

/// The failed branches whose selector fields (`operation`, `ecosystem`, …) all
/// accept the value: their enum issues are the value's own. When no branch's
/// selectors match, every branch competes and all of them count.
fn selected_failures<'a>(
    failures: &'a [Vec<ValidationIssue>],
    path: &[String],
) -> Vec<&'a [ValidationIssue]> {
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
        .map(Vec::as_slice)
        .collect::<Vec<_>>();
    if matched.is_empty() {
        failures.iter().map(Vec::as_slice).collect()
    } else {
        matched
    }
}

/// Collects the allowed literal values for each field path across every failed
/// branch. A branch reports one `schema.const` (a single literal) or
/// `schema.enum` (a set) per selector; their union is the field's true allowed
/// set, which no single branch's issue can name on its own.
fn aggregate_allowed_literals(failures: &[&[ValidationIssue]]) -> Vec<(Vec<String>, Vec<Value>)> {
    let mut allowed: Vec<(Vec<String>, Vec<Value>)> = Vec::new();
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
            .map(|received| super::enum_value::enum_hint(literals, received, &item.path))
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

fn score(issues: &[ValidationIssue], depth: usize) -> [usize; 5] {
    // A branch that does not declare a named discriminator (`tool`, `type`, …)
    // rejects it as strongly as a branch that pins it to other literals.
    // A branch missing a required literal (`operation:"models"`) rejects it
    // too, as core Zod's `invalid_value` does.
    let rejects = |issue: &ValidationIssue| {
        matches!(
            issue.rule_id.as_str(),
            "schema.const" | "schema.enum" | "schema.unknown-field"
        )
    };
    let invalid = |names: &[&str]| {
        issues
            .iter()
            .filter(|issue| {
                ((rejects(issue) && issue.path.len() == depth + 1) || missing_literal(issue, depth))
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
            (rejects(issue) || missing_literal(issue, depth))
                && issue
                    .path
                    .get(depth)
                    .is_some_and(|name| SELECTORS.contains(&name.as_str()))
        })
        .count();
    let count = grouped_issue_count(issues);
    [
        // A continuation names its tool: a branch for another tool is never
        // the closest match, however few field issues it reports.
        invalid(DISCRIMINATORS),
        invalid(&["resultView"]),
        issues
            .iter()
            .filter(|issue| issue.rule_id == "schema.not" && issue.path.len() == depth + 1)
            .count(),
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
/// When the query omits the selectors of the one branch in its mode, and
/// each is a single literal, the fix sets them (`setFields`:
/// `nameRegex` → `operation:"files"`); when the query's own selector picks
/// another branch, the field is that mode's (`siblingConflict`).
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
        // The fields any branch lists the unknown field's value for: the
        // selector a misnamed `type`/`analysis` meant; for a `true` field,
        // the fields that list its name (`merged:true` → `state`).
        let listing = |listed: &Value| {
            let mut fields = branches
                .iter()
                .filter_map(|branch| branch_object(root, branch).get("properties")?.as_object())
                .flat_map(|properties| super::schema::fields_listing(root, properties, listed))
                .collect::<Vec<_>>();
            fields.sort();
            fields.dedup();
            fields
        };
        // The fields whose declared value kinds name the unknown field
        // (`branch` → `ref`).
        let mut of_kind = branches
            .iter()
            .filter_map(|branch| branch_object(root, branch).get("properties")?.as_object())
            .flat_map(|properties| super::schema::fields_of_kind(root, properties, field))
            .collect::<Vec<_>>();
        of_kind.sort();
        of_kind.dedup();
        for (key, fields) in [
            ("kindFields", of_kind),
            ("valueFields", listing(&value[field])),
            (
                "nameFields",
                if value[field] == Value::Bool(true) {
                    listing(&Value::from(field.as_str()))
                } else {
                    Vec::new()
                },
            ),
        ] {
            if !fields.is_empty()
                && let Some(schema) = item.schema.as_mut().and_then(Value::as_object_mut)
            {
                schema.insert(key.into(), serde_json::json!(fields));
            }
        }
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
        // Missing fields only repair a branch in the query's own mode; when no
        // declaring branch shares its selectors, name the selectors instead
        // (`matchString` on an issue → the operations that accept it).
        let same_mode = declaring
            .iter()
            .copied()
            .filter(|branch| selectors_accept(branch, value))
            .collect::<Vec<_>>();
        let Some(schema) = item.schema.as_mut().and_then(Value::as_object_mut) else {
            continue;
        };
        if let Some(requires) = sibling_missing_fields(root, &same_mode, value, field) {
            schema.insert("siblingRequires".into(), Value::Array(requires));
            if let [branch] = same_mode.as_slice()
                && let Some(selectors) = omitted_selectors(root, branch, value, field)
                && !selectors.is_empty()
            {
                schema.insert("setFields".into(), Value::Object(selectors));
            }
        } else if let Some(requires) = sibling_selector_values(&declaring, value) {
            schema.insert("siblingRequires".into(), Value::Array(requires));
            schema.insert("siblingConflict".into(), Value::Bool(true));
        }
    }
}

/// The required fields `branch` still lacks (besides `field`), each with
/// its one literal (`operation:"files"`); `None` when one is no single
/// literal, so only the caller can fill it, or when the query carries a
/// field `branch` does not declare (it mixes forms: setting the selector
/// would only move the conflict).
fn omitted_selectors(
    root: &Value,
    branch: &Value,
    value: &Value,
    field: &str,
) -> Option<serde_json::Map<String, Value>> {
    let declared = branch.get("properties").and_then(Value::as_object)?;
    if !value
        .as_object()?
        .keys()
        .all(|key| declared.contains_key(key))
    {
        return None;
    }
    branch
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|required| *required != field && value.get(*required).is_none())
        .map(|required| {
            let schema = super::super::resolve_ref(&branch["properties"][required], &root["$defs"]);
            Some((required.to_owned(), single_literal(schema)?.clone()))
        })
        .collect()
}

/// The required fields each declaring branch still lacks, one alternative per
/// branch (`pattern or rule` for an astSearch match field), so the projector
/// names every form that accepts the field, not only the first.
fn sibling_missing_fields(
    root: &Value,
    declaring: &[&Value],
    value: &Value,
    field: &str,
) -> Option<Vec<Value>> {
    let mut options: Vec<String> = Vec::new();
    for branch in declaring {
        let missing = branch
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|required| *required != field && value.get(*required).is_none())
            .map(|required| {
                let schema =
                    super::super::resolve_ref(&branch["properties"][required], &root["$defs"]);
                selector_literals(schema).map_or_else(
                    || required.to_owned(),
                    |literals| render_selector(required, &literals),
                )
            })
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

/// Whether every literal selector of `branch` accepts the query's value.
fn selectors_accept(branch: &Value, value: &Value) -> bool {
    branch
        .get("properties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .all(
            |(name, schema)| match (selector_literals(schema), value.get(name)) {
                (Some(literals), Some(actual)) => literals.contains(&actual),
                _ => true,
            },
        )
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

#[cfg(test)]
mod tests;
