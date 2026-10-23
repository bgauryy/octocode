//! The one continuation stage.
//!
//! Tools build every follow-up call with
//! [`crate::response::result::Continuation`]: `{tool, query, why?, confidence?}`
//! where `query` is the complete tool input. [`finalize`] then shapes every
//! call of a response once, row by row:
//! 1. inherit the source row's non-empty `extras` (never over the call's
//!    own) on same-tool calls, only the `hints` opt-in on cross-tool calls.
//!    Ordinary continuations omit the optional brief (`mainGoal`, `reasoning`).
//!    Context selection keeps a copied brief because it affects judgment and replay;
//! 2. drop cross-tool calls this surface cannot run (a row whose drop a scope
//!    rule names gets one warning);
//! 3. channel each call (pages stay in `next`, leads and prose go to `hints`;
//!    a partial row with no page keeps every lead whole; see
//!    [`super::channels`]);
//! 4. compact each kept call: drop the fields validation restores, and
//!    validate it against the strict input contract. An invalid call is a
//!    runtime defect: it is dropped from its row, which keeps its evidence
//!    and states the drop (`partialReasons: ["leadInvalid"]` and a
//!    warning), and its issues are returned, never passed on silently.

mod compact;
use compact::compact_call;
use std::collections::HashMap;

use serde_json::Value;

use crate::contracts::{ContractValidationError, ValidationIssue};
use crate::id::ToolId;
use crate::id::continuation_channels::{HINTS_KEY, PAGES_KEY};
use crate::response::result::is_call;

/// Which cross-tool calls a surface can run; a call to a disabled tool drops
/// silently.
pub struct Scope<'a> {
    pub is_available: &'a dyn Fn(&str) -> bool,
}

#[cfg(test)]
impl Scope<'static> {
    /// Every tool runs (test surfaces).
    #[must_use]
    pub fn everything() -> Self {
        Scope {
            is_available: &|_| true,
        }
    }
}

/// The source query of each output row.
pub(crate) enum Sources<'a> {
    /// Ordinary rows: the input query of row `index` (`None` when rejected).
    Rows(&'a [Option<&'a Value>]),
    /// Clasify results: the input row with the same `id`, else the same position.
    Matrices(&'a Value),
}

impl Sources<'_> {
    fn of<'v>(&'v self, position: usize, row: &Value) -> Option<&'v Value> {
        match self {
            Sources::Rows(rows) => {
                let index = row
                    .get("index")
                    .and_then(Value::as_u64)
                    .and_then(|index| usize::try_from(index).ok())
                    .unwrap_or(position);
                rows.get(index).copied().flatten()
            }
            Sources::Matrices(input) => {
                let id = row.get("id");
                match input.get("queries").and_then(Value::as_array) {
                    Some(matrices) => matrices
                        .iter()
                        .find(|matrix| id.is_some() && matrix.get("id") == id)
                        .or_else(|| matrices.get(position)),
                    // A one-row call's response query is that row itself.
                    None => Some(*input),
                }
            }
        }
    }
}

/// Keep the caller's explicit `responseLength` on `tool`'s own pages: a
/// share it cut continues at the size the caller asked for, not the default
/// window. Cross-tool leads and calls that set their own length are left as
/// they are.
pub(crate) fn carry_response_length(structured: &mut Value, tool: ToolId, length: usize) {
    let rows_key = tool.output().rows_key();
    for row in structured
        .get_mut(rows_key)
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let Some(pages) = row
            .pointer_mut(&format!("/data/{PAGES_KEY}"))
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        for call in pages.values_mut() {
            if call.get("tool").and_then(Value::as_str) != Some(tool.as_str()) {
                continue;
            }
            if let Some(query) = call.get_mut("query").and_then(Value::as_object_mut)
                && query.contains_key("queries")
            {
                query
                    .entry("responseLength")
                    .or_insert_with(|| Value::from(length));
            }
        }
    }
}

/// The `partialReasons` entry of a row that lost a continuation the strict
/// input contract rejects.
pub(crate) const LEAD_INVALID: &str = "leadInvalid";

/// Shape every continuation of `structured` (see the module docs). A call
/// that fails the strict input contract is dropped from its row, which is
/// kept and marked [`LEAD_INVALID`]; the issues are returned, with the path
/// of each call, so debug builds and tests fail on the defect.
pub(crate) fn finalize(
    structured: &mut Value,
    tool: ToolId,
    sources: &Sources<'_>,
    scope: &Scope<'_>,
) -> Result<(), ContractValidationError> {
    let rows_key = tool.output().rows_key();
    let mut memo = Memo::new();
    let mut issues = Vec::new();
    let Some(rows) = structured.get_mut(rows_key).and_then(Value::as_array_mut) else {
        return Ok(());
    };
    for (position, row) in rows.iter_mut().enumerate() {
        let source = sources.of(position, row).cloned();
        let recovery = matches!(
            row.get("status").and_then(Value::as_str),
            Some("error" | "empty" | "outOfRange")
        );
        if let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) {
            super::channels::move_text(data);
        }
        if let Some(object) = row.as_object_mut() {
            super::channels::move_text(object);
        }
        let had_recovery = recovery && super::rows::has_recovery(row);
        let mut row_issues = Vec::new();
        let mut path = vec![rows_key.to_owned(), position.to_string()];
        visit(
            row,
            &mut Visit {
                tool,
                recovery,
                extras: source.as_ref().and_then(extras_of),
                scope,
                memo: &mut memo,
                issues: &mut row_issues,
            },
            &mut path,
        );
        drop_invalid_calls(row, &row_issues);
        if had_recovery && !super::rows::has_recovery(row) {
            super::rows::ensure_recovery(row, tool, source.as_ref().unwrap_or(&Value::Null));
            if let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) {
                super::channels::move_text(data);
            }
        }
        issues.extend(row_issues);
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}

/// Drops each call `issues` name from `row` (paths `[rows, position, …,
/// next|hints, name, …]`) and marks the row's data partial: the evidence
/// stays, only the lead the tool would reject is gone.
fn drop_invalid_calls(row: &mut Value, issues: &[ValidationIssue]) {
    let mut calls = issues
        .iter()
        .filter_map(|issue| {
            let at = issue
                .path
                .iter()
                .skip(2)
                .position(|segment| segment == PAGES_KEY || segment == HINTS_KEY)?
                + 2;
            Some(issue.path.get(2..at + 2)?.to_vec())
        })
        .collect::<Vec<_>>();
    calls.sort();
    calls.dedup();
    if calls.is_empty() {
        return;
    }
    for call in &calls {
        let Some((name, parent)) = call.split_last() else {
            continue;
        };
        let pointer = parent.iter().fold(String::new(), |pointer, segment| {
            format!("{pointer}/{segment}")
        });
        if let Some(calls) = row.pointer_mut(&pointer).and_then(Value::as_object_mut) {
            calls.remove(name);
            if calls.is_empty()
                && let Some((key, owner)) = parent.split_last()
            {
                let owner = owner.iter().fold(String::new(), |pointer, segment| {
                    format!("{pointer}/{segment}")
                });
                if let Some(owner) = row.pointer_mut(&owner).and_then(Value::as_object_mut) {
                    owner.remove(key);
                }
            }
        }
    }
    let target = if row.get("data").is_some_and(Value::is_object) {
        &mut row["data"]
    } else {
        row
    };
    crate::response::paging::mark_partial(
        target,
        LEAD_INVALID,
        &["leadInvalid: a continuation failed its tool's input contract and was withheld; the shown evidence stands.".to_owned()],
    );
}

type Memo = HashMap<(String, String), Value>;

struct Visit<'a, 's> {
    tool: ToolId,
    recovery: bool,
    /// The source row's non-empty `extras`.
    extras: Option<Value>,
    scope: &'a Scope<'s>,
    memo: &'a mut Memo,
    issues: &'a mut Vec<ValidationIssue>,
}

/// Every object holding `next`/`hints` in `value`, at any depth; calls are
/// caller content and never entered.
fn visit(value: &mut Value, ctx: &mut Visit<'_, '_>, path: &mut Vec<String>) {
    if is_call(value) {
        return;
    }
    match value {
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                path.push(index.to_string());
                visit(item, ctx, path);
                path.pop();
            }
        }
        Value::Object(object) => {
            for key in [PAGES_KEY, HINTS_KEY] {
                if let Some(Value::Object(calls)) = object.get_mut(key) {
                    calls.retain(|_, call| keep(call, ctx));
                    for call in calls.values_mut().filter(|call| is_call(call)) {
                        drop_brief(call);
                        inherit(call, ctx);
                    }
                    if calls.is_empty() {
                        object.remove(key);
                    }
                }
            }
            // Row-level leads of a successful row are opt-in (`extras`
            // `hints`); an item's leads (a PR row's content menu) disclose
            // what that item withholds and stay.
            let row_level = path.len() <= 3;
            // A resource-major tool's reads are its deliverable (clasify's
            // read of exactly what was judged), never optional leads.
            let opt_in_leads = row_level
                && !ctx.recovery
                && !ctx.tool.output().resource_major()
                && !wants_hints(ctx.extras.as_ref());
            // An object that reports a failure (`errorCode`) is a recovery
            // row wherever it sits: a clasify row's error carries no row `status`.
            let recovery = ctx.recovery || object.contains_key("errorCode");
            super::channels::shape(object, ctx.tool, recovery, opt_in_leads);
            for key in [PAGES_KEY, HINTS_KEY] {
                if let Some(Value::Object(calls)) = object.get_mut(key) {
                    for (name, call) in calls.iter_mut() {
                        path.extend([key.to_owned(), name.clone()]);
                        compact_call(call, ctx, path);
                        path.truncate(path.len() - 2);
                    }
                }
            }
            for (key, child) in object.iter_mut() {
                if key != PAGES_KEY && key != HINTS_KEY {
                    path.push(key.clone());
                    visit(child, ctx, path);
                    path.pop();
                }
            }
        }
        _ => {}
    }
}

/// Whether the surface keeps `call`: its own tool always; another tool only
/// when available (the drop is recorded).
fn keep(call: &Value, ctx: &mut Visit<'_, '_>) -> bool {
    let Some(target) = Some(call)
        .filter(|call| is_call(call))
        .and_then(|call| call["tool"].as_str())
    else {
        return true;
    };
    target == ctx.tool.as_str() || (ctx.scope.is_available)(target)
}

/// The source row's `extras` on every row of the call (a row that carries
/// its own keeps it): the output opt-ins steer the same tool's walk; a
/// cross-tool lead starts its own research step and keeps only the `hints`
/// opt-in, so the walk still offers leads.
fn inherit(call: &mut Value, ctx: &Visit<'_, '_>) {
    let same_tool = call.get("tool").and_then(Value::as_str) == Some(ctx.tool.as_str());
    let extras = match (&ctx.extras, same_tool) {
        (None, _) => return,
        (Some(extras), true) => extras.clone(),
        (Some(extras), false) if wants_hints(Some(extras)) => Value::Array(vec![Value::from(
            octocode_contract_types::ExtraKind::Hints.to_string(),
        )]),
        (Some(_), false) => return,
    };
    let Some(rows) = call
        .get_mut("query")
        .and_then(|query| query.get_mut("queries"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    for row in rows.iter_mut().filter_map(Value::as_object_mut) {
        row.entry("extras").or_insert_with(|| extras.clone());
    }
}

/// Removes request-only metadata from ordinary calls. Context selection keeps
/// the brief because it contributes to provider questions and the saved plan.
fn drop_brief(call: &mut Value) {
    let Some(rows) = call
        .get_mut("query")
        .and_then(|query| query.get_mut("queries"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    for row in rows.iter_mut().filter_map(Value::as_object_mut) {
        if row.get("clasifyContext").is_some_and(Value::is_object) {
            continue;
        }
        row.remove("mainGoal");
        row.remove("reasoning");
    }
}

/// Whether an `extras` list opts into lead hints.
fn wants_hints(extras: Option<&Value>) -> bool {
    extras.and_then(Value::as_array).is_some_and(|kinds| {
        kinds
            .iter()
            .any(|kind| super::extras::names(kind, octocode_contract_types::ExtraKind::Hints))
    })
}

/// The non-empty `extras` list `query` sent.
fn extras_of(query: &Value) -> Option<Value> {
    query
        .get("extras")
        .filter(|value| value.as_array().is_some_and(|kinds| !kinds.is_empty()))
        .cloned()
}

#[cfg(test)]
mod carry_tests;

#[cfg(test)]
mod tests;
