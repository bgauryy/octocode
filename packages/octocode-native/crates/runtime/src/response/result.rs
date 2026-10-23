//! Tool-owned diagnostics remain separate from public domain data, and the
//! one builder of executable follow-up calls.
mod error;
pub use error::{FailureKind, ToolError};
pub(crate) use error::{failure_kind, missing_path_lead};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::id::ToolId;

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
pub(crate) const CURSOR_FIELDS: [&str; 12] = [
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
    "cursor",
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
mod tests;
