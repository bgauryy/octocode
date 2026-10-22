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

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::contracts::{ContractValidationError, ValidationIssue, validate_query};
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

/// Compacts one admitted tool input: a flat query or a `{queries: [...]}`
/// envelope. Admission already validated it, so a row that still fails the
/// contract is left exactly as sent rather than failing the response.
pub(crate) fn compact_input(tool: &str, input: &mut Value) {
    let mut memo = Memo::new();
    let _ = compact_rows(tool, input, &mut memo);
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

/// Compact one `next`/`hints` call against the tool it names.
fn compact_call(call: &mut Value, ctx: &mut Visit<'_, '_>, path: &[String]) {
    let target = call.get("tool").and_then(Value::as_str).map(str::to_owned);
    let (within, issues) = match (target, call.get_mut("query")) {
        (Some(tool), Some(query)) => (
            Some("query"),
            compact_rows(&tool, query, ctx.memo)
                .err()
                .map(|error| error.issues),
        ),
        _ => (None, None),
    };
    ctx.issues
        .extend(issues.into_iter().flatten().map(|mut issue| {
            let mut full = path.to_vec();
            full.extend(within.map(str::to_owned));
            full.append(&mut issue.path);
            issue.path = full;
            issue
        }));
}

/// Every row of a `{queries}` input (or a flat row) compacted against `tool`.
fn compact_rows(
    tool: &str,
    query: &mut Value,
    memo: &mut Memo,
) -> Result<(), ContractValidationError> {
    let rows: Vec<&mut Value> = match query.get_mut("queries").and_then(Value::as_array_mut) {
        Some(rows) => rows.iter_mut().collect(),
        None => vec![query],
    };
    let mut issues = Vec::new();
    for (index, row) in rows.into_iter().enumerate() {
        let row_issues = match compact_query(tool, row, memo) {
            Ok(()) => Vec::new(),
            Err(error) => error.issues,
        };
        issues.extend(row_issues.into_iter().map(|mut issue| {
            issue
                .path
                .splice(0..0, ["queries".to_owned(), index.to_string()]);
            issue
        }));
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}

/// Drop each field whose removal validates to the identical query, so the
/// compact call executes exactly like the full one. Only a field the
/// contract can restore (a default) or that validation drops is tried; any
/// other present field stays. An invalid query is an error and unchanged.
///
/// Fields restore independently in the common case: a candidate drops when
/// validation dropped it or its value is the field's one declared default,
/// and one validation proves the set. A field with several defaults (an
/// operation's own) reads its restored value from one validation of the
/// query without every candidate instead. Only a query whose fields
/// interact falls back to trying each candidate in turn.
fn compact_query(
    tool: &str,
    query: &mut Value,
    memo: &mut Memo,
) -> Result<(), ContractValidationError> {
    let Some(object) = query.as_object() else {
        return Ok(());
    };
    let key = (tool.to_owned(), query.to_string());
    if let Some(compact) = memo.get(&key) {
        *query = compact.clone();
        return Ok(());
    }
    let full = validate_query(tool, query.clone())?;
    let restorable = ToolId::from_name(tool).map(crate::contracts::restorable_fields);
    let candidates: Vec<&String> = object
        .keys()
        .filter(|field| {
            full.get(field.as_str()).is_none()
                || restorable.is_none_or(|fields| fields.contains_key(field.as_str()))
        })
        .collect();
    let mut compact = object.clone();
    if !candidates.is_empty() {
        // The restored value of each candidate: its one declared default,
        // else what validation without every candidate restores.
        let predicted = restorable.filter(|fields| {
            candidates.iter().all(|field| {
                full.get(field.as_str()).is_none()
                    || fields.get(field.as_str()).is_some_and(Option::is_some)
            })
        });
        let restored = match predicted {
            Some(_) => None,
            None => {
                let mut stripped = object.clone();
                for field in &candidates {
                    stripped.remove(field.as_str());
                }
                validate_query(tool, Value::Object(stripped)).ok()
            }
        };
        for field in &candidates {
            let full_value = full.get(field.as_str());
            let same = match predicted {
                Some(fields) => {
                    full_value.is_none()
                        || fields.get(field.as_str()).copied().flatten() == full_value
                }
                None => restored
                    .as_ref()
                    .is_some_and(|restored| restored.get(field.as_str()) == full_value),
            };
            if same {
                compact.remove(field.as_str());
            }
        }
        let proven = compact.len() == object.len()
            || validate_query(tool, Value::Object(compact.clone()))
                .is_ok_and(|validated| validated == full);
        if !proven {
            compact = object.clone();
            for field in &candidates {
                let Some(removed) = compact.remove(field.as_str()) else {
                    continue;
                };
                let restored = validate_query(tool, Value::Object(compact.clone()))
                    .is_ok_and(|validated| validated == full);
                if !restored {
                    compact.insert((*field).clone(), removed);
                }
            }
        }
    }
    // Keep the caller's field order for the fields that remain.
    let ordered: Map<String, Value> = object
        .iter()
        .filter_map(|(field, _)| compact.get(field).map(|v| (field.clone(), v.clone())))
        .collect();
    *query = Value::Object(ordered);
    memo.insert(key, query.clone());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(tool: &str, row: Value) -> Value {
        json!({"tool": tool, "query": {"queries": [row]}})
    }

    fn run(out: &mut Value, tool: ToolId, rows: &[Option<&Value>]) {
        finalize(out, tool, &Sources::Rows(rows), &Scope::everything()).expect("valid calls");
    }

    #[test]
    fn same_tool_calls_inherit_only_extras_cross_tool_calls_only_hints() {
        let first = json!({"mainGoal":"Find a.","reasoning":"First.","extras":["hints","stats"]});
        let second = json!({"mainGoal":"Find b.","extras":[]});
        let mut out = json!({"results":[
            {"index":0,"data":{"next":{
                "nextPage":call("ghSearchCode", json!({"owner":"o","keywords":["a"],"page":2})),
                "viewRepo":call("ghStructure", json!({"owner":"o","repo":"r"}))
            }}},
            {"index":1,"data":{"items":[{"next":{
                "nextPage":call("ghSearchCode", json!({"owner":"o","keywords":["b"],"page":2})),
                "read":call("localFetch", json!({"path":"/a","offset":10}))
            }}]}}
        ]});
        run(
            &mut out,
            ToolId::GhSearchCode,
            &[Some(&first), Some(&second)],
        );
        let page = &out["results"][0]["data"]["next"]["nextPage"]["query"]["queries"][0];
        assert!(
            page.get("mainGoal").is_none() && page.get("reasoning").is_none(),
            "a page carries no brief: {page}"
        );
        assert_eq!(page["extras"], json!(["hints", "stats"]), "{page}");
        let lead = &out["results"][0]["data"]["hints"]["viewRepo"]["query"]["queries"][0];
        assert!(lead.get("mainGoal").is_none(), "{lead}");
        assert_eq!(
            lead["extras"],
            json!(["hints"]),
            "a cross-tool lead keeps only the lead opt-in"
        );
        let items = &out["results"][1]["data"]["items"][0];
        let page = &items["next"]["nextPage"]["query"]["queries"][0];
        assert_eq!(
            page,
            &json!({"owner":"o","keywords":["b"],"page":2}),
            "an empty extras array is the default"
        );
        let read = &items["hints"]["read"]["query"]["queries"][0];
        assert!(read.get("extras").is_none(), "{read}");
    }

    #[test]
    fn selection_pages_keep_the_brief_but_unfiltered_recovery_drops_it() {
        let source = json!({"owner":"o","keywords":["a"],"mainGoal":"Find capacity ownership.","reasoning":"Inspect the release path.","clasifyContext":{"classifiers":["Where is reserved capacity released?"]}});
        let mut page = source.clone();
        page["page"] = json!(2);
        let mut recovery = source.clone();
        recovery.as_object_mut().unwrap().remove("clasifyContext");
        let mut out = json!({"results":[{"index":0,"data":{"next":{
            "nextPage":call("ghSearchCode",page),
            "unfiltered":call("ghSearchCode",recovery)
        }}}]});
        run(&mut out, ToolId::GhSearchCode, &[Some(&source)]);
        let next = &out["results"][0]["data"]["next"];
        let page = &next["nextPage"]["query"]["queries"][0];
        assert_eq!(page["mainGoal"], source["mainGoal"]);
        assert_eq!(page["reasoning"], source["reasoning"]);
        let recovery = &next["unfiltered"]["query"]["queries"][0];
        assert!(recovery.get("mainGoal").is_none());
        assert!(recovery.get("reasoning").is_none());
        assert!(recovery.get("clasifyContext").is_none());
    }

    /// A tool that builds its page by cloning the row query copies the brief
    /// with it; the one continuation stage drops it there too, on pages and
    /// leads alike, so no tool has to remember to.
    #[test]
    fn a_brief_a_tool_copied_into_its_continuation_is_dropped() {
        let source = json!({"mainGoal":"Find a.","reasoning":"First."});
        let copied = |page: u32| json!({"matchString":"a","path":"/r","mainGoal":"Find a.","reasoning":"First.","page":page});
        let mut out = json!({"results":[{"index":0,"data":{
            "next":{"nextPage":call("localSearch", copied(2))},
            "hints":{
                "readFile":call("localFetch", json!({"path":"/r/a","mainGoal":"Find a.","reasoning":"First."}))
            }
        }}]});
        run(&mut out, ToolId::LocalSearch, &[Some(&source)]);
        let data = &out["results"][0]["data"];
        for (name, row) in [
            ("nextPage", &data["next"]["nextPage"]["query"]["queries"][0]),
            (
                "readFile",
                &data["hints"]["readFile"]["query"]["queries"][0],
            ),
        ] {
            assert!(
                row.get("mainGoal").is_none() && row.get("reasoning").is_none(),
                "{name} carries no brief: {row}"
            );
        }
    }

    /// The brief is request-level: copied into every row of a page it was
    /// the largest token sink of a walk. Measured on this five-row page with
    /// an 87-char `mainGoal` and a 97-char `reasoning` (serde_json bytes of
    /// the page's `query`): 1343 with the brief on every row, 298 without it
    /// (-1045 bytes, 78%); the inherited `extras` is 19 of those bytes per
    /// row, and a 500-char brief would have cost ~5 KB per five-row page.
    #[test]
    fn a_five_row_page_carries_no_brief() {
        let source = json!({
            "mainGoal":"Find where the response stage inherits the brief into continuation rows across tools.",
            "reasoning":"The walk pages a wide search; every page repeated the brief in each of its rows, so measure it.",
            "extras":["stats"]
        });
        let rows: Vec<Value> = (1..=5)
            .map(|index| json!({"path":format!("src/f{index}.rs"),"ranges":["1-9"]}))
            .collect();
        let mut out = json!({"results":[{"index":0,"data":{"next":{
            "readHits":{"tool":"localFetch","query":{"queries":rows.clone()}}
        }}}]});
        run(&mut out, ToolId::LocalFetch, &[Some(&source)]);
        let page = &out["results"][0]["data"]["next"]["readHits"]["query"];
        let without = serde_json::to_vec(page).expect("json").len();
        let mut briefed = page.clone();
        for row in briefed["queries"].as_array_mut().expect("rows") {
            row["mainGoal"] = source["mainGoal"].clone();
            row["reasoning"] = source["reasoning"].clone();
        }
        let with = serde_json::to_vec(&briefed).expect("json").len();
        for row in page["queries"].as_array().expect("rows") {
            assert_eq!(row["extras"], json!(["stats"]), "{page}");
            assert!(row.get("mainGoal").is_none(), "{page}");
        }
        assert!(
            with > without * 4,
            "brief on every row: {with} bytes, extras only: {without} bytes"
        );
    }

    /// A successful row offers only the contract's default leads unless its
    /// `extras` names `hints`; empty and error rows keep every lead, and an
    /// item's leads (inside a row's array) always stay.
    #[test]
    fn row_level_leads_of_a_successful_row_are_opt_in() {
        let next = || {
            json!({
                "nextPage":call("ghSearchCode", json!({"owner":"o","keywords":["a"],"page":2})),
                "viewRepo":call("ghStructure", json!({"owner":"o","repo":"r"})),
                "includeArchived":call("ghSearchCode", json!({"owner":"o","keywords":["a"]}))
            })
        };
        let plain = json!({"mainGoal":"Find a."});
        let hinted = json!({"mainGoal":"Find a.","extras":["hints"]});
        let mut out = json!({"results":[
            {"index":0,"data":{"next":next()}},
            {"index":1,"data":{"next":next()}},
            {"index":2,"status":"empty","data":{"next":next()}},
            {"index":3,"data":{"items":[{"next":next()}]}}
        ]});
        run(
            &mut out,
            ToolId::GhSearchCode,
            &[Some(&plain), Some(&hinted), Some(&plain), Some(&plain)],
        );
        let default = &out["results"][0]["data"];
        assert!(
            default["next"]["nextPage"].is_object(),
            "pages stay: {default}"
        );
        assert!(
            default["hints"].get("viewRepo").is_none(),
            "a row-level lead is opt-in: {default}"
        );
        assert!(
            default["hints"]["includeArchived"].is_object(),
            "a contract default lead stays: {default}"
        );
        assert!(
            out["results"][1]["data"]["hints"]["viewRepo"].is_object(),
            "extras:[hints] keeps the lead: {}",
            out["results"][1]
        );
        assert!(
            out["results"][2]["data"]["hints"]["viewRepo"].is_object(),
            "an empty row keeps its leads: {}",
            out["results"][2]
        );
        assert!(
            out["results"][3]["data"]["items"][0]["hints"]["viewRepo"].is_object(),
            "an item's leads stay: {}",
            out["results"][3]
        );
    }

    #[test]
    fn a_clasify_read_lead_is_its_result_and_always_shows() {
        let input = json!({"queries":[{"id":"m1","mainGoal":"G1"},{"id":"m2","mainGoal":"G2"}]});
        let mut out = json!({"queries":[{"id":"m2","answers":{},
            "hints":{"read":call("localFetch", json!({"path":"/a","ranges":["1-2"]}))}}]});
        finalize(
            &mut out,
            ToolId::Clasify,
            &Sources::Matrices(&input),
            &Scope::everything(),
        )
        .expect("valid");
        let read = &out["queries"][0]["hints"]["read"];
        assert_eq!(
            read["query"]["queries"][0]["path"], "/a",
            "shown without extras hints: {read}"
        );
        assert!(
            read["query"]["queries"][0].get("mainGoal").is_none(),
            "{read}"
        );
    }

    #[test]
    fn compaction_drops_only_fields_validation_restores() {
        let row = json!({"matchString":"foo","path":"/tmp","mainGoal":"Find foo.","reasoning":"r","extras":["stats"],
            "caseMode":"smart","matchContentLength":200,"page":2,"contextLines":0});
        let mut out =
            json!({"results":[{"index":0,"data":{"next":{"nextPage":call("localSearch", row)}}}]});
        run(&mut out, ToolId::LocalSearch, &[None]);
        let query = &out["results"][0]["data"]["next"]["nextPage"]["query"]["queries"][0];
        // matchContentLength has no schema default (it scales with
        // contextLines), so validation cannot restore it and it is kept. The
        // brief is request-level: no continuation carries it.
        assert_eq!(
            query,
            &json!({"matchString":"foo","path":"/tmp","extras":["stats"],"matchContentLength":200,"page":2,"contextLines":0})
        );
    }

    #[test]
    fn an_invalid_call_is_dropped_and_its_row_kept() {
        let read = call("localFetch", json!({"path":"/a"}));
        let mut out = json!({"results":[{"index":0,"data":{"files":["a.rs"],
            "next":{"nextPage":call("localSearch", json!({"bogus":true}))},
            "hints":{"read":read}}},
            {"index":1,"data":{"files":["b.rs"],"next":{"nextPage":call("localSearch", json!({"bogus":true}))}}}]});
        finalize(
            &mut out,
            ToolId::LocalSearch,
            &Sources::Rows(&[None, None]),
            &Scope::everything(),
        )
        .expect_err("invalid call");
        for row in out["results"].as_array().expect("rows") {
            let data = &row["data"];
            assert!(data["files"].is_array(), "the evidence stays: {row}");
            assert!(
                data.get("next").is_none(),
                "the invalid lead is gone: {row}"
            );
            assert_eq!(data["partialReasons"], json!([LEAD_INVALID]), "{row}");
            assert_eq!(data["isPartial"], true, "{row}");
            let warning = data["warnings"][0].as_str().expect("warning");
            assert!(warning.len() <= 120, "{warning}");
        }
    }

    #[test]
    fn an_invalid_call_is_an_error_with_its_path() {
        let mut out = json!({"results":[{"index":0,"data":{"next":{"nextPage":call("localSearch", json!({"bogus":true}))}}}]});
        let error = finalize(
            &mut out,
            ToolId::LocalSearch,
            &Sources::Rows(&[None]),
            &Scope::everything(),
        )
        .expect_err("invalid call");
        let path = &error.issues[0].path;
        assert_eq!(
            &path[..6],
            ["results", "0", "data", "next", "nextPage", "query"]
        );
    }

    #[test]
    fn compact_input_leaves_an_invalid_row_as_sent() {
        let row = json!({"bogus":true,"extras":[]});
        let mut input = json!({"queries":[row.clone()]});
        compact_input("localSearch", &mut input);
        assert_eq!(input["queries"][0], row);
    }

    /// Trying only restorable or dropped fields compacts exactly like trying
    /// every field.
    /// A restart restates no contract default (`pageSize` of an issue read).
    #[test]
    fn a_restart_drops_the_default_page_size() {
        let query = json!({"operation":"issue","owner":"o","repo":"r","number":7,
            "sections":["comments"],"page":40});
        let full = validate_query("ghGetHistoryItem", query).expect("valid");
        let mut out = json!({"results":[{"index":0,"data":{"next":{
            "restart": crate::response::result::restart_query(ToolId::GhGetHistoryItem, &full)
        }}}]});
        run(&mut out, ToolId::GhGetHistoryItem, &[Some(&full)]);
        let row = &out["results"][0]["data"]["next"]["restart"]["query"]["queries"][0];
        assert!(row.get("pageSize").is_none(), "{row}");
    }

    #[test]
    fn restorable_fields_compact_like_trying_every_field() {
        fn every_field(tool: &str, query: &Value) -> Value {
            let full = validate_query(tool, query.clone()).expect("valid");
            let mut compact = query.as_object().cloned().unwrap_or_default();
            for field in query
                .as_object()
                .into_iter()
                .flatten()
                .map(|(field, _)| field)
            {
                let Some(removed) = compact.remove(field) else {
                    continue;
                };
                if validate_query(tool, Value::Object(compact.clone())).ok() != Some(full.clone()) {
                    compact.insert(field.clone(), removed);
                }
            }
            query
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(field, _)| compact.get(field).map(|v| (field.clone(), v.clone())))
                .collect::<Map<_, _>>()
                .into()
        }
        for (tool, query) in [
            (
                "localSearch",
                json!({"matchString":"foo","path":"/tmp","mainGoal":"g","extras":["stats"],
                "caseMode":"smart","page":1,"matchContentLength":200,"contextLines":0,"sort":"relevance"}),
            ),
            (
                "localFetch",
                json!({"path":"/a","matchString":"x","reasoning":" ","caseMode":"smart"}),
            ),
            (
                "ghSearchCode",
                json!({"owner":"o","repo":"r","keywords":["a"],"page":1,"extras":["stats"]}),
            ),
            (
                "structureSearch",
                json!({"operation":"tree","path":"/a","extras":["receipts","echo"]}),
            ),
        ] {
            let mut compact = query.clone();
            compact_query(tool, &mut compact, &mut Memo::new()).expect("valid");
            assert_eq!(compact, every_field(tool, &query), "{tool}");
        }
    }

    #[test]
    fn compacts_each_row_of_an_input_and_keeps_window_fields() {
        let mut input = json!({"queries":[{"matchString":"a","path":"/tmp","mainGoal":"Find a.","reasoning":"r","caseMode":"smart"}],
            "responseOffset":10});
        compact_input("localSearch", &mut input);
        assert_eq!(
            input["queries"][0],
            json!({"matchString":"a","path":"/tmp","mainGoal":"Find a.","reasoning":"r"})
        );
        assert_eq!(input["responseOffset"], 10);
    }

    #[test]
    fn unavailable_cross_tool_calls_drop_silently_and_own_pages_stay() {
        let mut out = json!({"results":[
            {"index":0,"data":{"next":{
                "nextPage":call("ghStructure", json!({"owner":"o","repo":"r","page":2})),
                "materialized":call("localSearch", json!({"path":"/x","matchString":"a"}))
            },"warnings":["tool warning"]}},
            {"index":1,"data":{"next":{"read":call("localFetch", json!({"path":"/a"}))}}}
        ]});
        let scope = Scope {
            is_available: &|_| false,
        };
        finalize(
            &mut out,
            ToolId::GhStructure,
            &Sources::Rows(&[None, None]),
            &scope,
        )
        .expect("valid");
        let first = &out["results"][0]["data"];
        assert!(
            first["next"]["nextPage"].is_object(),
            "own pages stay: {first}"
        );
        assert!(first.get("hints").is_none(), "{first}");
        assert_eq!(first["warnings"], json!(["tool warning"]));
        assert!(
            out["results"][1]["data"].get("warnings").is_none(),
            "{}",
            out["results"][1]
        );
    }

    #[test]
    fn an_empty_row_that_loses_its_only_lead_to_the_surface_keeps_a_recovery_hint() {
        let query = json!({"owner":"o","repo":"r","keywords":["zzz"]});
        let mut out = json!({"results":[{"index":0,"status":"empty","data":{"next":{
            "cloneRepo":call("ghCloneRepo", json!({"owner":"o","repo":"r"}))
        }}}]});
        let scope = Scope {
            is_available: &|_| false,
        };
        finalize(
            &mut out,
            ToolId::GhSearchCode,
            &Sources::Rows(&[Some(&query)]),
            &scope,
        )
        .expect("valid");
        let data = &out["results"][0]["data"];
        assert!(data.get("next").is_none(), "{data}");
        assert_eq!(
            data["hints"],
            json!({(crate::id::continuation_channels::HINT_TEXT_KEY): [ToolId::GhSearchCode.output().fallback_hint(&query)]}),
            "{data}"
        );
    }

    #[test]
    /// A recovery lead stays only on an empty or failed row. A lead carries
    /// no prose: the tool description states what each lead kind is for.
    fn recovery_leads_stay_only_on_empty_or_failed_rows() {
        let tree = call("ghStructure", json!({"owner":"o","repo":"r","path":"src"}));
        let read = call(
            "ghGetFileContent",
            json!({"owner":"o","repo":"r","path":"a"}),
        );
        let row = |status: &str| json!({"index":0,"status":status,"data":{"next":{"viewTree":tree.clone(),"read":read.clone()}}});
        let mut ok = json!({"results":[row("ok")]});
        run(&mut ok, ToolId::GhSearchCode, &[None]);
        let data = &ok["results"][0]["data"];
        assert!(
            data.get("hints")
                .and_then(|hints| hints.get("viewTree"))
                .is_none(),
            "{data}"
        );
        let mut empty = json!({"results":[row("empty")]});
        run(&mut empty, ToolId::GhSearchCode, &[None]);
        let hints = &empty["results"][0]["data"]["hints"];
        for name in ["viewTree", "read"] {
            assert_eq!(
                hints[name]
                    .as_object()
                    .map(|call| call.keys().cloned().collect::<Vec<_>>()),
                Some(vec!["tool".to_owned(), "query".to_owned()]),
                "{name}: {hints}"
            );
        }
    }
}
