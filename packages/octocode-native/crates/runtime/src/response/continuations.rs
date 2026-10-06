//! The one continuation stage.
//!
//! Tools build every follow-up call with
//! [`crate::tools::result::Continuation`]: `{tool, query, why?, confidence?}`
//! where `query` is the complete tool input. [`finalize`] then shapes every
//! call of a response once, row by row:
//! 1. inherit the source row's intent fields (only the fields it sent,
//!    never over the call's own): the brief on same-tool calls, `debug:true`
//!    on every call;
//! 2. drop cross-tool calls this surface cannot run (a row whose drop a scope
//!    rule names gets one warning);
//! 3. channel each call (pages stay in `next`, leads and prose go to `hints`;
//!    see [`super::channels`]);
//! 4. compact each kept call: drop the fields validation restores, and
//!    validate it against the strict input contract. An invalid call is a
//!    runtime defect and is returned, never passed on silently.

use std::collections::HashMap;

use serde_json::{Map, Value};

use super::channels::{HINTS_KEY, PAGES_KEY};
use crate::contracts::{ContractValidationError, ValidationIssue, validate_query};
use crate::tools::id::ToolId;
use crate::tools::result::INTENT_FIELDS;

/// Which cross-tool calls a surface can run; a call to a disabled tool drops
/// silently.
pub struct Scope<'a> {
    pub is_available: &'a dyn Fn(&str) -> bool,
}

impl Scope<'static> {
    /// Every tool runs (embedded and test surfaces).
    #[must_use]
    pub fn everything() -> Self {
        Scope {
            is_available: &|_| true,
        }
    }
}

/// The source query of each output row.
pub enum Sources<'a> {
    /// Ordinary rows: the input query of row `index` (`None` when rejected).
    Rows(&'a [Option<&'a Value>]),
    /// Clasify results: the matrix with the same `id`, else the same position.
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

/// Shape every continuation of `structured` (see the module docs). Returns
/// the issues of calls that fail the strict input contract, with the path of
/// each call, for the caller to isolate.
pub fn finalize(
    structured: &mut Value,
    tool: ToolId,
    sources: &Sources<'_>,
    scope: &Scope<'_>,
) -> Result<(), ContractValidationError> {
    let rows_key = if tool.output().resource_major() {
        "queries"
    } else {
        "results"
    };
    let mut memo = Memo::new();
    let mut issues = Vec::new();
    let Some(rows) = structured.get_mut(rows_key).and_then(Value::as_array_mut) else {
        return Ok(());
    };
    for (position, row) in rows.iter_mut().enumerate() {
        let source = sources.of(position, row).cloned();
        let recovery = matches!(
            row.get("status").and_then(Value::as_str),
            Some("error" | "empty")
        );
        if let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) {
            super::channels::move_text(data);
        }
        if let Some(object) = row.as_object_mut() {
            super::channels::move_text(object);
        }
        let mut row_issues = Vec::new();
        let mut path = vec![rows_key.to_owned(), position.to_string()];
        visit(
            row,
            &mut Visit {
                tool,
                recovery,
                intent: source.as_ref().map(intent_of).unwrap_or_default(),
                scope,
                memo: &mut memo,
                issues: &mut row_issues,
            },
            &mut path,
        );
        issues.extend(row_issues);
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContractValidationError { issues })
    }
}

/// Compacts one admitted tool input: a flat query or a `{queries: [...]}`
/// envelope. Admission already validated it, so a row that still fails the
/// contract is left exactly as sent rather than failing the response.
pub fn compact_input(tool: &str, input: &mut Value) {
    let mut memo = Memo::new();
    let _ = compact_rows(tool, input, &mut memo);
}

type Memo = HashMap<(String, String), Value>;

struct Visit<'a, 's> {
    tool: ToolId,
    recovery: bool,
    intent: Map<String, Value>,
    scope: &'a Scope<'s>,
    memo: &'a mut Memo,
    issues: &'a mut Vec<ValidationIssue>,
}

/// Every object holding `next`/`hints` in `value`, at any depth; calls are
/// caller content and never entered.
fn visit(value: &mut Value, ctx: &mut Visit<'_, '_>, path: &mut Vec<String>) {
    match value {
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                path.push(index.to_string());
                visit(item, ctx, path);
                path.pop();
            }
        }
        Value::Object(object) => {
            if is_call(object) {
                return;
            }
            if ctx.tool.output().resource_major() {
                seal_reads(object);
            }
            for key in [PAGES_KEY, HINTS_KEY] {
                if let Some(Value::Object(calls)) = object.get_mut(key) {
                    calls.retain(|_, call| keep(call, ctx));
                    for call in calls.values_mut().filter(|call| call_of(call)) {
                        inherit(call, ctx);
                    }
                    if calls.is_empty() {
                        object.remove(key);
                    }
                }
            }
            super::channels::shape(object, ctx.tool, ctx.recovery);
            for key in [PAGES_KEY, HINTS_KEY] {
                if let Some(Value::Object(calls)) = object.get_mut(key) {
                    for (name, call) in calls.iter_mut() {
                        path.extend([key.to_owned(), name.clone()]);
                        compact_call(name, call, ctx, path);
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

fn is_call(object: &Map<String, Value>) -> bool {
    object.get("tool").is_some_and(Value::is_string)
        && object.get("query").is_some_and(Value::is_object)
}

fn call_of(value: &Value) -> bool {
    value.as_object().is_some_and(is_call)
}

/// Clasify walks its reads in the clasify resource shape (`{tool, query:
/// row}`); as output they become complete inputs like every continuation.
fn seal_reads(object: &mut Map<String, Value>) {
    for key in [PAGES_KEY, HINTS_KEY] {
        if let Some(Value::Object(calls)) = object.get_mut(key) {
            for call in calls.values_mut() {
                if let Some(sealed) = crate::tools::result::Continuation::from_row_call(call) {
                    *call = sealed;
                }
            }
        }
    }
}

/// Whether the surface keeps `call`: its own tool always; another tool only
/// when available (the drop is recorded).
fn keep(call: &Value, ctx: &mut Visit<'_, '_>) -> bool {
    let Some(target) = call
        .as_object()
        .filter(|object| is_call(object))
        .and_then(|object| object["tool"].as_str())
    else {
        return true;
    };
    target == ctx.tool.as_str() || (ctx.scope.is_available)(target)
}

/// The source row's intent fields on every row of the call (a field the
/// call carries is kept). The brief (`mainGoal`, `reasoning`) steers the
/// same tool's walk only; a cross-tool lead starts its own research step.
fn inherit(call: &mut Value, ctx: &Visit<'_, '_>) {
    let same_tool = call.get("tool").and_then(Value::as_str) == Some(ctx.tool.as_str());
    let Some(rows) = call
        .get_mut("query")
        .and_then(|query| query.get_mut("queries"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    for row in rows.iter_mut().filter_map(Value::as_object_mut) {
        for (field, value) in &ctx.intent {
            if same_tool || field == "debug" {
                row.entry(field.clone()).or_insert_with(|| value.clone());
            }
        }
    }
}

/// The intent fields `query` sent; `debug` only when on (off is the default).
fn intent_of(query: &Value) -> Map<String, Value> {
    INTENT_FIELDS
        .into_iter()
        .filter_map(|field| {
            query
                .get(field)
                .filter(|value| value.is_string() || *value == &Value::Bool(true))
                .map(|value| (field.to_owned(), value.clone()))
        })
        .collect()
}

/// Compact one `next`/`hints` entry: a call, or clasify's own `next.clasify`
/// input (its matrices' reads compact against their own tools).
fn compact_call(name: &str, call: &mut Value, ctx: &mut Visit<'_, '_>, path: &[String]) {
    let target = call.get("tool").and_then(Value::as_str).map(str::to_owned);
    let (within, issues) = match (target, call.get_mut("query")) {
        (Some(tool), Some(query)) => (
            Some("query"),
            compact_rows(&tool, query, ctx.memo)
                .err()
                .map(|error| error.issues),
        ),
        _ if ToolId::from_name(name).is_some_and(|named| named.output().resource_major()) => {
            (None, Some(compact_matrix_reads(call, ctx.memo)))
        }
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
    let resource_major =
        ToolId::from_name(tool).is_some_and(|named| named.output().resource_major());
    let mut issues = Vec::new();
    for (index, row) in rows.into_iter().enumerate() {
        let mut row_issues = match compact_query(tool, row, memo) {
            Ok(()) => Vec::new(),
            Err(error) => error.issues,
        };
        if resource_major {
            row_issues.extend(compact_matrix_reads(row, memo));
        }
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

/// A clasify input nests one ordinary read per resource; compact each
/// against its own tool (the matrix itself is validated as clasify). Returns
/// the issues of invalid reads, by path within `input`.
fn compact_matrix_reads(input: &mut Value, memo: &mut Memo) -> Vec<ValidationIssue> {
    let matrices: Vec<(Vec<String>, &mut Value)> =
        match input.get_mut("queries").and_then(Value::as_array_mut) {
            Some(rows) => rows
                .iter_mut()
                .enumerate()
                .map(|(index, row)| (vec!["queries".to_owned(), index.to_string()], row))
                .collect(),
            None => vec![(Vec::new(), input)],
        };
    let mut issues = Vec::new();
    for (prefix, matrix) in matrices {
        let Some(resources) = matrix.get_mut("resources").and_then(Value::as_array_mut) else {
            continue;
        };
        for (index, read) in resources.iter_mut().enumerate() {
            let Some(read) = read.as_object_mut() else {
                continue;
            };
            let Some(tool) = read.get("tool").and_then(Value::as_str).map(str::to_owned) else {
                continue;
            };
            let Some(query) = read.get_mut("query").filter(|query| query.is_object()) else {
                continue;
            };
            if let Err(error) = compact_query(&tool, query, memo) {
                issues.extend(error.issues.into_iter().map(|mut issue| {
                    let mut path = prefix.clone();
                    path.extend([
                        "resources".to_owned(),
                        index.to_string(),
                        "query".to_owned(),
                    ]);
                    path.append(&mut issue.path);
                    issue.path = path;
                    issue
                }));
            }
        }
    }
    issues
}

/// Drop each field whose removal validates to the identical query, so the
/// compact call executes exactly like the full one. Only a field the
/// contract can restore (a default) or that validation drops is tried; any
/// other present field stays. An invalid query is an error and unchanged.
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
    let restorable = ToolId::from_name(tool).and_then(crate::contracts::restorable_fields);
    let mut compact = object.clone();
    for field in object.keys() {
        let candidate = full.get(field).is_none()
            || restorable.is_none_or(|fields| fields.contains(field.as_str()));
        if !candidate {
            continue;
        }
        let Some(removed) = compact.remove(field) else {
            continue;
        };
        let restored = validate_query(tool, Value::Object(compact.clone()))
            .is_ok_and(|validated| validated == full);
        if !restored {
            compact.insert(field.clone(), removed);
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
    fn same_tool_calls_inherit_the_brief_and_every_call_inherits_debug_true() {
        let first = json!({"mainGoal":"Find a.","reasoning":"First.","debug":true});
        let second = json!({"mainGoal":"Find b.","debug":false});
        let mut out = json!({"results":[
            {"index":0,"data":{"next":{
                "nextPage":call("ghSearchCode", json!({"owner":"o","keywords":["a"],"page":2})),
                "viewRepo":call("ghStructure", json!({"owner":"o","repo":"r"})),
                "clasify":call("clasify", json!({"mainGoal":"Locate.","reasoning":"Unread.",
                    "resources":[{"id":"f","tool":"localFetch","query":{"path":"/a"}}],
                    "questions":[{"id":"t","type":"locate","ask":"where"}]}))
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
        assert_eq!(page["mainGoal"], "Find a.", "a page keeps the brief");
        assert_eq!(page["reasoning"], "First.");
        assert_eq!(page["debug"], true);
        let lead = &out["results"][0]["data"]["hints"]["viewRepo"]["query"]["queries"][0];
        assert!(
            lead.get("mainGoal").is_none(),
            "a cross-tool lead takes no brief: {lead}"
        );
        assert!(lead.get("reasoning").is_none(), "{lead}");
        assert_eq!(lead["debug"], true, "debug follows the source row");
        let clasify = &out["results"][0]["data"]["hints"]["clasify"]["query"]["queries"][0];
        assert_eq!(clasify["mainGoal"], "Locate.", "an existing brief is kept");
        let items = &out["results"][1]["data"]["items"][0];
        let page = &items["next"]["nextPage"]["query"]["queries"][0];
        assert_eq!(page["mainGoal"], "Find b.");
        assert!(
            page.get("reasoning").is_none(),
            "only the fields the row sent"
        );
        assert!(
            page.get("debug").is_none(),
            "debug:false is the default: {page}"
        );
        let read = &items["hints"]["read"]["query"]["queries"][0];
        assert!(read.get("mainGoal").is_none(), "{read}");
        assert!(read.get("debug").is_none(), "{read}");
    }

    #[test]
    fn clasify_reads_become_complete_inputs_without_a_brief() {
        let input = json!({"queries":[{"id":"m1","mainGoal":"G1"},{"id":"m2","mainGoal":"G2"}]});
        let mut out = json!({"queries":[{"id":"m2","next":{"clasify":{"queries":[{"resources":[{"tool":"localFetch","query":{"path":"/a"}}]}]}},
            "resources":[{"pages":[{"next":{"read":{"tool":"localFetch","confidence":"high","query":{"path":"/a","ranges":["1-2"]}}}}]}]}]});
        finalize(
            &mut out,
            ToolId::Clasify,
            &Sources::Matrices(&input),
            &Scope::everything(),
        )
        .expect("valid");
        let read = &out["queries"][0]["resources"][0]["pages"][0]["hints"]["read"];
        assert_eq!(
            read["query"],
            json!({"queries":[{"path":"/a","ranges":["1-2"]}]}),
            "a cross-tool read takes no brief"
        );
        assert!(
            read.get("confidence").is_none(),
            "leads carry no confidence: {read}"
        );
        // The walk stays a page, and its matrix is clasify input.
        let walk = &out["queries"][0]["next"]["clasify"]["queries"][0]["resources"][0];
        assert_eq!(walk["query"], json!({"path":"/a"}));
    }

    #[test]
    fn compaction_drops_only_fields_validation_restores() {
        let row = json!({"matchString":"foo","path":"/tmp","mainGoal":"Find foo.","reasoning":"r","debug":false,
            "caseMode":"smart","matchContentLength":200,"page":2,"contextLines":0});
        let mut out =
            json!({"results":[{"index":0,"data":{"next":{"nextPage":call("localSearch", row)}}}]});
        run(&mut out, ToolId::LocalSearch, &[None]);
        let query = &out["results"][0]["data"]["next"]["nextPage"]["query"]["queries"][0];
        // matchContentLength has no schema default (it scales with
        // contextLines), so validation cannot restore it and it is kept.
        assert_eq!(
            query,
            &json!({"matchString":"foo","path":"/tmp","mainGoal":"Find foo.","reasoning":"r","matchContentLength":200,"page":2,"contextLines":0})
        );
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
    fn an_invalid_matrix_read_is_an_error_with_its_path() {
        let input = json!({"queries":[{"id":"m1","mainGoal":"G1"}]});
        let mut out = json!({"queries":[{"id":"m1","next":{"clasify":{"queries":[
            {"resources":[{"tool":"localFetch","query":{"bogus":true}}]}]}}}]});
        let error = finalize(
            &mut out,
            ToolId::Clasify,
            &Sources::Matrices(&input),
            &Scope::everything(),
        )
        .expect_err("invalid read");
        assert_eq!(
            &error.issues[0].path[..8],
            [
                "queries",
                "0",
                "next",
                "clasify",
                "queries",
                "0",
                "resources",
                "0"
            ]
        );
    }

    #[test]
    fn compact_input_leaves_an_invalid_row_as_sent() {
        let row = json!({"bogus":true,"debug":false});
        let mut input = json!({"queries":[row.clone()]});
        compact_input("localSearch", &mut input);
        assert_eq!(input["queries"][0], row);
    }

    /// Trying only restorable or dropped fields compacts exactly like trying
    /// every field.
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
                json!({"matchString":"foo","path":"/tmp","mainGoal":"g","debug":false,
                "caseMode":"smart","page":1,"matchContentLength":200,"contextLines":0,"sort":"relevance"}),
            ),
            (
                "localFetch",
                json!({"path":"/a","debug":false,"matchString":"x","reasoning":" "}),
            ),
            (
                "ghSearchCode",
                json!({"owner":"o","repo":"r","keywords":["a"],"page":1,"debug":false}),
            ),
            (
                "structureSearch",
                json!({"operation":"tree","path":"/a","debug":false}),
            ),
        ] {
            let mut compact = query.clone();
            compact_query(tool, &mut compact, &mut Memo::new()).expect("valid");
            assert_eq!(compact, every_field(tool, &query), "{tool}");
        }
    }

    #[test]
    fn compacts_each_row_of_an_input_and_keeps_window_fields() {
        let mut input = json!({"queries":[{"matchString":"a","path":"/tmp","mainGoal":"Find a.","reasoning":"r","debug":false}],
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
                "materialized":call("localSearch", json!({"path":"/x","matchString":"a"})),
                "clasify":call("clasify", json!({"resources":[{"id":"f","tool":"localFetch","query":{"path":"/a"}}],
                    "questions":[{"id":"t","type":"locate","ask":"where"}]}))
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
    fn recovery_leads_and_why_stay_only_on_empty_or_failed_rows() {
        let tree = call("ghStructure", json!({"owner":"o","repo":"r","path":"src"}));
        let mut why = call(
            "ghGetFileContent",
            json!({"owner":"o","repo":"r","path":"a"}),
        );
        why["why"] = json!("Read  the   top hit.");
        let row = |status: &str| json!({"index":0,"status":status,"data":{"next":{"viewStructure":tree.clone(),"readTopMatch":why.clone()}}});
        let mut ok = json!({"results":[row("ok")]});
        run(&mut ok, ToolId::GhSearchCode, &[None]);
        let hints = &ok["results"][0]["data"]["hints"];
        assert!(hints.get("viewStructure").is_none(), "{hints}");
        assert!(hints["readTopMatch"].get("why").is_none(), "{hints}");
        let mut empty = json!({"results":[row("empty")]});
        run(&mut empty, ToolId::GhSearchCode, &[None]);
        let hints = &empty["results"][0]["data"]["hints"];
        assert!(hints["viewStructure"].is_object(), "{hints}");
        assert_eq!(hints["readTopMatch"]["why"], "Read the top hit.");
    }
}
