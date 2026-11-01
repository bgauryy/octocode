//! Union branch selection and actionable diagnostics from the embedded schema.
mod branches;
mod failures;
mod selectors;
mod siblings;
#[cfg(test)]
use super::schema::validate_schema;
use super::schema::{ValidationMode, validate_with_mode};
use super::{ContractValidationError, issue};
use failures::{
    aggregate_allowed_literals, missing_literal, mixed_forms, name_missing_alternatives,
    only_missing, required_alternatives, score, selected_failures, widen_literal_issues,
};
use selectors::{
    annotate_forbidden_fields, annotate_missing_selector, annotate_sibling_selectors,
    render_selector, selector_literals, selectors_accept, sibling_selector_values,
};
use serde_json::Value;
use siblings::{annotate_sibling_branch_fields, branch_object};
use std::cell::Cell;

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
    if accept(root, branches, exclusive, value, path, mode)? {
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
/// them too); the rest validate speculatively, on copies unless the mode
/// only reads. On success the value becomes the first accepting branch's
/// normalized copy, as in `reference`.
fn accept(
    root: &Value,
    branches: &[Value],
    exclusive: bool,
    value: &mut Value,
    path: &[String],
    mode: ValidationMode,
) -> Result<bool, ContractValidationError> {
    let _speculation = Speculation::enter();
    let mut accepted = None;
    for branch in branches {
        if certainly_rejects(root, branch, value) {
            continue;
        }
        let Some(result) = try_branch(root, branch, value, path, mode)? else {
            continue;
        };
        if accepted.is_some() {
            // A second match fails an exclusive union.
            return Ok(false);
        }
        if !exclusive {
            keep(value, result);
            return Ok(true);
        }
        accepted = Some(result);
    }
    match accepted {
        Some(result) => {
            keep(value, result);
            Ok(true)
        }
        None => Ok(false),
    }
}

/// What a branch that accepts the value leaves.
enum Accepted {
    /// A read-only mode checked the value in place: it is the result.
    InPlace,
    /// The value's normalized copy.
    Normalized(Value),
}

/// `value` validated against `branch`: `None` when the branch rejects it.
/// A read-only mode checks the value in place because nothing is written.
fn try_branch(
    root: &Value,
    branch: &Value,
    value: &mut Value,
    path: &[String],
    mode: ValidationMode,
) -> Result<Option<Accepted>, ContractValidationError> {
    if mode == ValidationMode::ReadOnly {
        return super::schema::probe(validate_with_mode(
            root,
            branch,
            value,
            &mut path.to_vec(),
            mode,
        ))
        .map(|accepted| accepted.then_some(Accepted::InPlace));
    }
    let mut candidate = value.clone();
    super::schema::probe(validate_with_mode(
        root,
        branch,
        &mut candidate,
        &mut path.to_vec(),
        mode,
    ))
    .map(|accepted| accepted.then_some(Accepted::Normalized(candidate)))
}

/// Take an accepting branch's result.
fn keep(value: &mut Value, accepted: Accepted) {
    if let Accepted::Normalized(normalized) = accepted {
        *value = normalized;
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
    let mut undeclared = Vec::new();
    for branch in branches {
        let mut candidate = value.clone();
        match validate_with_mode(root, branch, &mut candidate, &mut path.to_vec(), mode) {
            Ok(()) => {
                successes += 1;
                if success.is_none() {
                    success = Some(candidate);
                }
            }
            Err(error) if error.is_contract_error() => return Err(error),
            Err(error) => {
                failures.push(error.issues);
                undeclared.push(branches::undeclared_fields(root, branch, value));
            }
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
    // Each branch pins its selector fields to distinct literals, so any single
    // branch's const/enum issue names only that branch's value(s). Collect the
    // allowed literals for every field across the branches the value selects
    // (all branches when its selectors match none) before one branch is
    // chosen, so the surfaced error lists the full set for the chosen form,
    // never another form's literals.
    let selectors = branches::selector_fields(root, branches);
    let allowed = aggregate_allowed_literals(&selected_failures(&failures, path, &selectors));
    let missing_sets = failures
        .iter()
        .filter_map(|issues| only_missing(issues, path.len()))
        .collect::<Vec<_>>();
    let Some((mut selected, _)) = failures
        .into_iter()
        .zip(undeclared)
        .min_by_key(|(issues, undeclared)| score(issues, path.len(), &selectors, *undeclared))
    else {
        return Err(issue(
            "schema.union",
            path.to_vec(),
            "Input matches multiple exclusive schema branches",
        ));
    };
    widen_literal_issues(&mut selected, &allowed);
    name_missing_alternatives(&mut selected, &missing_sets, path.len());
    annotate_sibling_branch_fields(&mut selected, root, branches, value);
    annotate_sibling_selectors(&mut selected, root, branches, value, path);
    annotate_forbidden_fields(&mut selected, root, branches, value, path);
    annotate_missing_selector(&mut selected, root, branches, value, path);
    // A wrong or missing selector is the first repair: the other issues are
    // relative to the branch it picked. The sort is stable.
    let depth = path.len();
    selected.sort_by_key(|item| {
        let leads = item.path.len() == depth + 1
            && (matches!(item.rule_id.as_str(), "schema.const" | "schema.enum")
                || missing_literal(item, depth));
        !leads
    });
    // Branch scoring may group key errors for parity, but the selected branch
    // must retain individual paths and schemas for precise diagnostics.
    Err(ContractValidationError { issues: selected })
}

#[cfg(test)]
mod tests;
