//! Canonical union branch selection, matching validation/unionIssues.ts.
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
    annotate_forbidden_fields, annotate_sibling_selectors, render_selector, selector_literals,
    selectors_accept, sibling_selector_values, single_literal,
};
use serde_json::Value;
use siblings::{annotate_sibling_branch_fields, branch_object};
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
    let missing_sets = failures
        .iter()
        .filter_map(|issues| only_missing(issues, path.len()))
        .collect::<Vec<_>>();
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
    name_missing_alternatives(&mut selected, &missing_sets, path.len());
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

#[cfg(test)]
mod tests;
