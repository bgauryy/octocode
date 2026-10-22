//! Tool-owned diagnostics remain separate from public domain data, and the
//! one builder of executable follow-up calls.
use serde::Serialize;
use serde_json::{Map, Value};

use crate::id::error_codes::ErrorClass;
use crate::id::{ToolId, error_class};

/// How sure a continuation is to help: `exact` is a replay, `high`,
/// `medium` and `low` a judgment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Confidence {
    Exact,
    High,
    Medium,
    Low,
}

impl Confidence {
    const ALL: [Self; 4] = [Self::Exact, Self::High, Self::Medium, Self::Low];

    /// The wire value.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }

    /// The confidence a built continuation carries as `wire`.
    pub(crate) fn from_wire(wire: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|confidence| confidence.as_str() == wire)
    }
}

/// One executable follow-up call, `{tool, query, confidence?}`. The
/// `query` is always the complete tool input: query rows for new work, or
/// snapshot and paging fields for a saved response. MCP and CLI run it verbatim. Every `next`
/// page and `hints` lead is built here; debug builds and tests validate each
/// one against the strict input contract when it is built.
#[must_use]
pub(crate) struct Continuation {
    tool: ToolId,
    input: Value,
    confidence: Option<Confidence>,
}

impl Continuation {
    /// A one-row call.
    pub fn new(tool: ToolId, row: Value) -> Self {
        Self::input(tool, serde_json::json!({ "queries": [row] }))
    }

    /// A complete input: query rows or saved-response paging fields.
    pub fn input(tool: ToolId, input: Value) -> Self {
        Self {
            tool,
            input,
            confidence: None,
        }
    }

    pub fn confidence(mut self, confidence: Confidence) -> Self {
        self.confidence = Some(confidence);
        self
    }

    pub fn build(self) -> Value {
        #[cfg(any(test, debug_assertions))]
        assert_valid(self.tool, &self.input);
        let mut call = Map::new();
        call.insert("tool".into(), Value::from(self.tool.as_str()));
        call.insert("query".into(), self.input);
        if let Some(confidence) = self.confidence {
            call.insert("confidence".into(), Value::from(confidence.as_str()));
        }
        Value::Object(call)
    }
}

/// Fails loudly on a continuation the tool it names would reject.
#[cfg(any(test, debug_assertions))]
#[expect(
    clippy::panic,
    reason = "a continuation its target rejects is a runtime defect; debug builds fail loudly"
)]
fn assert_valid(tool: ToolId, input: &Value) {
    if let Err(error) = crate::contracts::validate(tool.as_str(), input.clone()) {
        panic!(
            "invalid {} continuation {input}: {:?}",
            tool.as_str(),
            error.issues
        );
    }
}

/// Rows one `tool` call accepts: the contract's `queries.maxItems`, so a
/// lead that batches rows stays within it.
#[must_use]
pub(crate) fn rows_per_call(tool: ToolId) -> usize {
    crate::contracts::tool_contract(tool)
        .ok()
        .and_then(|contract| {
            let input = &contract["inputSchema"];
            crate::contracts::property_schemas(input, &input["$defs"], "queries")
                .into_iter()
                .filter_map(|schema| schema["maxItems"].as_u64())
                .min()
        })
        .and_then(|rows| usize::try_from(rows).ok())
        .unwrap_or(1)
        .max(1)
}

/// The first row of a continuation call (`query.queries[0]`).
#[must_use]
pub(crate) fn continuation_row(call: &Value) -> Option<&Value> {
    call.get("query")?.get("queries")?.get(0)
}

/// [`continuation_row`], mutable.
pub(crate) fn continuation_row_mut(call: &mut Value) -> Option<&mut Value> {
    call.get_mut("query")?.get_mut("queries")?.get_mut(0)
}

/// Whether `value` is one executable call: `{tool: string, query: object}`.
#[must_use]
pub(crate) fn is_call(value: &Value) -> bool {
    value.get("tool").is_some_and(Value::is_string)
        && value.get("query").is_some_and(Value::is_object)
}

/// The deepest nesting a walk of a result enters: rows nest items, page
/// blocks and calls far above this.
pub(crate) const NESTING_DEPTH: usize = 12;

/// Every executable call in `value` ([`is_call`]) at any depth up to
/// [`NESTING_DEPTH`]; a call's own query is caller content and never entered.
pub(crate) fn for_each_call(value: &Value, visit: &mut impl FnMut(&Value)) {
    fn walk(value: &Value, visit: &mut impl FnMut(&Value), depth: usize) {
        if depth > NESTING_DEPTH {
            return;
        }
        if is_call(value) {
            visit(value);
            return;
        }
        match value {
            Value::Object(map) => map.values().for_each(|child| walk(child, visit, depth + 1)),
            Value::Array(items) => items.iter().for_each(|item| walk(item, visit, depth + 1)),
            _ => {}
        }
    }
    walk(value, visit, 0);
}

/// [`for_each_call`], mutable.
pub(crate) fn for_each_call_mut(value: &mut Value, visit: &mut impl FnMut(&mut Value)) {
    fn walk(value: &mut Value, visit: &mut impl FnMut(&mut Value), depth: usize) {
        if depth > NESTING_DEPTH {
            return;
        }
        if is_call(value) {
            visit(value);
            return;
        }
        match value {
            Value::Object(map) => map
                .values_mut()
                .for_each(|child| walk(child, visit, depth + 1)),
            Value::Array(items) => items
                .iter_mut()
                .for_each(|item| walk(item, visit, depth + 1)),
            _ => {}
        }
    }
    walk(value, visit, 0);
}

/// One call as one pasteable line, `<tool> <compact query JSON>`, which the
/// CLI runs as `octocode <tool> '<query JSON>'`; a low-confidence lead reads
/// `Approximate: …`. The hints channel and the YAML text both print it.
#[must_use]
pub(crate) fn call_line(call: &Value) -> Option<String> {
    let tool = call.get("tool").and_then(Value::as_str)?;
    if !is_call(call) {
        return None;
    }
    let qualification = if call.get("confidence").and_then(Value::as_str) == Some("low") {
        "Approximate: "
    } else {
        ""
    };
    Some(format!("{qualification}{tool} {}", call["query"]))
}

/// `row` with `changes` applied: each change sets its field, and a `null`
/// change drops it.
pub(crate) fn with_changes(mut row: Value, changes: &Value) -> Value {
    if let (Some(to), Some(from)) = (row.as_object_mut(), changes.as_object()) {
        for (field, value) in from {
            if value.is_null() {
                to.shift_remove(field);
            } else {
                to.insert(field.clone(), value.clone());
            }
        }
    }
    row
}

/// How a call failed, for the caller's exit code and error state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum FailureKind {
    NotFound,
    Authentication,
    Permission,
    RateLimited,
    Execution,
}

/// The failure an error row with `code` reports. A rejected request is an
/// execution failure here; [`crate::response::exit::ExitClass::InvalidInput`] is
/// decided from the rows.
pub(crate) fn failure_kind(code: &str) -> FailureKind {
    match error_class(code) {
        ErrorClass::NotFound => FailureKind::NotFound,
        ErrorClass::Authentication => FailureKind::Authentication,
        ErrorClass::Permission => FailureKind::Permission,
        ErrorClass::RateLimited => FailureKind::RateLimited,
        ErrorClass::InvalidInput | ErrorClass::Execution => FailureKind::Execution,
    }
}

/// A local tool call that failed as a whole. The dispatcher renders it as
/// the one error row: `errorCode`, `error`, then `hints` and `next` when set.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolError {
    pub code: String,
    pub message: String,
    pub hints: Vec<String>,
    /// Leads out of the error, e.g. a missing path's nearest existing parent.
    /// Boxed to keep the error small in `Result`s.
    pub next: Option<Box<Value>>,
    /// The coverage receipt of a classified row whose selection failed.
    pub selection: Option<Box<octocode_contract_types::Selection>>,
}

impl ToolError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            hints: Vec::new(),
            next: None,
            selection: None,
        }
    }

    /// A failed graph-facts scan: the engine's typed refusals keep their
    /// declared codes, and a walk stopped by `cancel` is a cancellation.
    pub(crate) fn graph_scan(
        error: &octocode_engine::fact_scan::GraphScanError,
        cancel: &dyn crate::execution::cancel::CancellationCheck,
    ) -> Self {
        use octocode_engine::fact_scan::GraphScanError;
        let code = match error {
            GraphScanError::UnsupportedLanguage(_) => "languageUnsupported",
            GraphScanError::InvalidGlob(_) => "invalidGlob",
            GraphScanError::Failed(_) if cancel.check().is_err() => "cancelled",
            GraphScanError::Failed(_) => "scanFailed",
        };
        Self::new(code, error.to_string())
    }

    /// A continuation whose snapshot no longer describes the source:
    /// `restart` (usually [`restart_query`]) is `next.restart`. The runtime
    /// writes the one shared `error` text and clears `isPartial`
    /// (`response::pages::restart_stale`); a tool never words it.
    pub(crate) fn stale_snapshot(restart: Value) -> Self {
        Self {
            next: Some(Box::new(serde_json::json!({ "restart": restart }))),
            ..Self::new(
                "staleSnapshot",
                crate::response::pages::STALE_SNAPSHOT_ERROR,
            )
        }
    }

    /// The call was cancelled or ran past its deadline.
    pub fn cancelled(message: impl Into<String>) -> Self {
        Self::new("cancelled", message)
    }

    /// A path-policy refusal under the flat code every local tool shares,
    /// else under the tool's own `access_code`
    /// ([`crate::policy::PolicyError::local_error_code`]).
    pub fn policy(error: crate::policy::PolicyError, access_code: &'static str) -> Self {
        Self::new(error.local_error_code(access_code), error.message)
    }

    /// [`Self::policy`] for a search root; a missing root also leads to a
    /// tree of its nearest existing parent (`next.viewTree`), where a typo's
    /// siblings show.
    pub(crate) fn root_policy(
        error: crate::policy::PolicyError,
        requested: &str,
        paths: &crate::policy::path::PathPolicy,
        access_code: &'static str,
    ) -> Self {
        let missing = error.code == crate::policy::PolicyErrorCode::NotFound;
        let mut out = Self::policy(error, access_code);
        if missing && let Some(next) = missing_path_lead(paths, requested) {
            out.next = Some(Box::new(next));
        }
        out
    }
}

/// `{ "viewTree": <lead> }` to the tree of `requested`'s nearest existing
/// parent, where a mistyped name's siblings show; the one recovery form every
/// local tool gives a missing path.
pub(crate) fn missing_path_lead(
    paths: &crate::policy::path::PathPolicy,
    requested: &str,
) -> Option<serde_json::Value> {
    let parent = paths.nearest_existing_dir(requested)?;
    let lead = Continuation::new(
        ToolId::StructureSearch,
        serde_json::json!({"operation": "tree", "path": parent}),
    )
    .build();
    Some(serde_json::json!({ "viewTree": lead }))
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ToolError {}

/// Fields that steer how a call reports, never what it reads: a row never
/// echoes them, and a continuation inherits only `extras` from its source
/// row (the brief is request-level).
pub(crate) const INTENT_FIELDS: [&str; 3] = ["mainGoal", "reasoning", "extras"];

#[derive(Debug, Default, Serialize)]
pub(crate) struct ToolDiagnostics {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub codes: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
    pub partial: bool,
}
impl ToolDiagnostics {
    pub fn add(&mut self, code: &str, hint: &str, partial: bool) {
        if !self.codes.iter().any(|value| value == code) {
            self.codes.push(code.into());
        }
        if !self.hints.iter().any(|value| value == hint) {
            self.hints.push(hint.into());
        }
        self.partial |= partial;
    }
}

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub(crate) struct ToolData {
    pub data: Value,
    #[serde(skip)]
    pub diagnostics: ToolDiagnostics,
    #[serde(skip)]
    pub status: Option<&'static str>,
    /// What the row's pages leave to read, counted by the tool from facts its
    /// public pagination does not state; never an output field. The response
    /// stage words a note only for a page a split moved
    /// ([`crate::response::pages::note_remaining`]).
    #[serde(skip)]
    pub remaining: Vec<crate::response::pages::Remaining>,
}
impl From<Value> for ToolData {
    fn from(data: Value) -> Self {
        Self {
            data,
            diagnostics: ToolDiagnostics::default(),
            status: None,
            remaining: Vec::new(),
        }
    }
}

/// Every query field that names where a walk stands: its pages, offsets and
/// snapshots, and the saved `selection` plan they were ordered by. A restart
/// drops them all; the contract guard keeps the list complete.
pub(crate) const CURSOR_FIELDS: [&str; 11] = [
    "page",
    "matchPage",
    "importerPage",
    "offset",
    "scanOffset",
    "materializeOffset",
    "walkOffset",
    "snapshot",
    "selection",
    "candidatePage",
    "headSha",
];

/// The one restart of a row query `row` of `tool`: the same query from its
/// first page, without any [`CURSOR_FIELDS`], as a replay (`exact`).
pub(crate) fn restart_query(tool: ToolId, row: &Value) -> Value {
    let mut restart = row.clone();
    if let Some(fields) = restart.as_object_mut() {
        for field in CURSOR_FIELDS {
            fields.remove(field);
        }
    }
    Continuation::new(tool, restart)
        .confidence(Confidence::Exact)
        .build()
}

/// [`ToolError::stale_snapshot`] as a result row, for a tool that returns
/// rows rather than errors.
pub(crate) fn stale_snapshot(restart: Value) -> Value {
    let error = ToolError::stale_snapshot(restart);
    serde_json::json!({
        "status": "error",
        "errorCode": error.code,
        "next": error.next.map_or(Value::Null, |next| *next),
    })
}

/// A generated query as JSON, every field as written. A generated query is
/// plain data, so serializing it cannot fail.
pub(crate) fn query_value(query: &impl Serialize) -> Value {
    serde_json::to_value(query).unwrap_or_default()
}

/// A typed query as a continuation row: every field it sets, except `omit`.
pub(crate) fn query_row(query: &impl Serialize, omit: &[&str]) -> Value {
    let mut row = query_value(query);
    remove_nulls(&mut row);
    if let Some(row) = row.as_object_mut() {
        for key in omit {
            row.remove(*key);
        }
    }
    row
}

/// Drop `null` members from every object in `value`, including objects nested
/// inside arrays.
pub(crate) fn remove_nulls(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|_, value| !value.is_null());
            map.values_mut().for_each(remove_nulls);
        }
        Value::Array(values) => values.iter_mut().for_each(remove_nulls),
        _ => {}
    }
}

/// Drop `null` members from `value` and its nested objects; arrays are left
/// untouched.
pub(crate) fn remove_null_fields(value: &mut Value) {
    if let Value::Object(map) = value {
        map.retain(|_, value| !value.is_null());
        map.values_mut().for_each(remove_null_fields);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CURSOR_FIELDS, ToolError, for_each_call, for_each_call_mut, restart_query, stale_snapshot,
        with_changes,
    };
    use crate::id::ToolId;
    use serde_json::{Value, json};

    /// Every query field of every tool that names where a walk stands (a
    /// page, an offset, a snapshot, a saved selection) is in the one list a
    /// restart drops, and every listed field is some tool's query field.
    #[test]
    fn the_cursor_list_is_every_contract_cursor_field() {
        let mut fields = std::collections::BTreeSet::new();
        for tool in ToolId::ALL {
            let contract = crate::contracts::tool_contract(tool).expect("tool contract");
            let schema = &contract["querySchema"];
            let defs = &schema["$defs"];
            let branches = ["oneOf", "anyOf"]
                .iter()
                .find_map(|key| schema.get(*key).and_then(Value::as_array))
                .map_or_else(|| vec![schema], |branches| branches.iter().collect());
            for branch in branches {
                let branch = crate::contracts::resolve_ref(branch, defs);
                for field in branch["properties"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(f, _)| f)
                {
                    fields.insert(field.clone());
                }
            }
        }
        let cursor = |field: &str| {
            matches!(field, "page" | "offset" | "snapshot" | "selection")
                || ["Page", "Offset", "Snapshot"]
                    .iter()
                    .any(|suffix| field.ends_with(suffix))
        };
        let found: Vec<&String> = fields.iter().filter(|field| cursor(field)).collect();
        let listed: Vec<&str> = CURSOR_FIELDS.to_vec();
        for field in &found {
            assert!(
                listed.contains(&field.as_str()),
                "{field} is a cursor field"
            );
        }
        for field in &listed {
            assert!(fields.contains(*field), "{field} is no query field");
        }
    }

    /// A restart is page 1 of the same query: every cursor and the saved
    /// selection dropped, a replay (`exact`), executable as built.
    #[test]
    fn a_restart_drops_every_cursor_and_the_saved_selection() {
        let row = json!({"path":"src","matchString":"f","page":3,"matchPage":2,
            "snapshot":"s1","selection":"plan","pageSize":5});
        let restart = restart_query(ToolId::LocalSearch, &row);
        assert_eq!(restart["tool"], "localSearch");
        assert_eq!(restart["confidence"], "exact");
        assert_eq!(
            restart["query"]["queries"][0],
            json!({"path":"src","matchString":"f","pageSize":5})
        );
    }

    /// The stale row a tool returns, as a row or as a whole-call error,
    /// carries the code and the restart; the runtime owns its text.
    #[test]
    fn a_stale_snapshot_carries_its_restart() {
        let restart = restart_query(
            ToolId::LocalFetch,
            &json!({"path":"a","offset":90,"snapshot":"s"}),
        );
        let row = stale_snapshot(restart.clone());
        assert_eq!(row["errorCode"], "staleSnapshot");
        assert_eq!(row["next"]["restart"], restart);
        let error = ToolError::stale_snapshot(restart.clone());
        assert_eq!(error.code, "staleSnapshot");
        assert_eq!(error.next.as_deref(), Some(&json!({"restart": restart})));
    }

    /// A change sets its field in place, a `null` change drops it, and the
    /// fields left keep their order.
    #[test]
    fn changes_set_fields_and_nulls_drop_them() {
        let row = json!({"path":"a","page":2,"pageSize":5,"snapshot":"s"});
        let changed = with_changes(row, &json!({"page":3,"snapshot":null,"depth":1}));
        assert_eq!(
            serde_json::to_string(&changed).expect("json"),
            r#"{"path":"a","page":3,"pageSize":5,"depth":1}"#
        );
    }

    /// The call walk visits every `{tool, query}` at any depth (under `next`,
    /// `hints`, items, a response page) and never enters a call's query.
    #[test]
    fn the_call_walk_visits_every_call_and_never_its_query() {
        let inner = json!({"tool":"localFetch","query":{"queries":[{"path":"b"}]}});
        let mut value = json!({
            "next":{"nextPage":{"tool":"localSearch","query":{"queries":[{"path":"a"}],
                "nested":inner}}},
            "items":[{"hints":[inner]}],
            "tool":"not a call"
        });
        let mut tools = Vec::new();
        for_each_call(&value, &mut |call| tools.push(call["tool"].clone()));
        assert_eq!(tools, vec![json!("localSearch"), json!("localFetch")]);
        for_each_call_mut(&mut value, &mut |call| call["visited"] = json!(true));
        assert_eq!(value["next"]["nextPage"]["visited"], true);
        assert!(
            value["next"]["nextPage"]["query"]["nested"]
                .get("visited")
                .is_none()
        );
        assert_eq!(value["items"][0]["hints"][0]["visited"], true);
    }
}
