//! Tool-owned diagnostics remain separate from public domain data, and the
//! one builder of executable follow-up calls.
mod error;
pub use error::{FailureKind, ToolError};
pub(crate) use error::{STALE_SNAPSHOT_ERROR, failure_kind, missing_path_lead};
use serde::Serialize;
use serde_json::Value;

use crate::id::ToolId;

/// An explicit follow-up: saved work or a fresh related request.
#[must_use]
pub(crate) struct Continuation {
    tool: ToolId,
    input: Value,
    kind: ContinuationKind,
}

#[derive(Clone, Copy)]
enum ContinuationKind {
    Continue,
    Related,
}

impl Continuation {
    pub fn continued(tool: ToolId, row: Value) -> Self {
        Self::continued_input(tool, serde_json::json!({"queries": [row]}))
    }

    pub fn related(tool: ToolId, row: Value) -> Self {
        Self::related_input(tool, serde_json::json!({"queries": [row]}))
    }

    pub fn continued_input(tool: ToolId, input: Value) -> Self {
        Self {
            tool,
            input,
            kind: ContinuationKind::Continue,
        }
    }

    pub fn related_input(tool: ToolId, input: Value) -> Self {
        Self {
            tool,
            input,
            kind: ContinuationKind::Related,
        }
    }

    pub fn build(self) -> Value {
        serde_json::json!({
            "kind": match self.kind { ContinuationKind::Continue => "continue", ContinuationKind::Related => "related" },
            "tool": self.tool.as_str(),
            "input": self.input,
        })
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

/// The first row of a continuation call (`input.queries[0]`).
#[must_use]
pub(crate) fn continuation_row(call: &Value) -> Option<&Value> {
    call.get("input")?.get("queries")?.get(0)
}

/// [`continuation_row`], mutable.
pub(crate) fn continuation_row_mut(call: &mut Value) -> Option<&mut Value> {
    call.get_mut("input")?.get_mut("queries")?.get_mut(0)
}

/// Explicit executable calls are never inferred from a source identity object.
#[must_use]
pub(crate) fn is_call(value: &Value) -> bool {
    matches!(
        value.get("kind").and_then(Value::as_str),
        Some("continue" | "related")
    ) && value.get("tool").is_some_and(Value::is_string)
        && value.get("input").is_some_and(Value::is_object)
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

/// Canonical public guidance keys.
pub(crate) const PAGES_KEY: &str = "next";
pub(crate) const HINTS_KEY: &str = "hints";

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

/// A page of a row that leaves a counted rest to read: the page's `next`
/// name and what is left (`17 more files`), as data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
pub(crate) struct Remaining {
    pub page: String,
    pub left: String,
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
    pub remaining: Vec<Remaining>,
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

/// The same semantic request without private saved-work progress.
pub(crate) fn fresh_query_row(tool: ToolId, row: &Value) -> Value {
    let mut fresh = row.clone();
    if let Some(fields) = fresh.as_object_mut() {
        for field in crate::contracts::state_fields(tool) {
            fields.remove(field);
        }
    }
    fresh
}

/// A new request over the same scope; private progress belongs only to the saved work.
pub(crate) fn restart_query(tool: ToolId, row: &Value) -> Value {
    Continuation::related(tool, fresh_query_row(tool, row)).build()
}

/// [`ToolError::stale_snapshot`] as a result row, for a tool that returns
/// rows rather than errors.
pub(crate) fn stale_snapshot(restart: Value) -> Value {
    let error = ToolError::stale_snapshot(restart);
    let mut data =
        serde_json::json!({"status":"error","errorCode":error.code,"error":error.message});
    if let Some(calls) = error.calls {
        attach_calls(&mut data, *calls);
    }
    data
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

pub(crate) fn attach_related(data: &mut Value, call: Value) {
    if data.get("related").is_none() {
        data["related"] = serde_json::json!([]);
    }
    if let Some(related) = data.get_mut("related").and_then(Value::as_array_mut)
        && !related.contains(&call)
    {
        related.push(call);
    }
}

/// Producers declare each call's semantics. Only that explicit kind chooses its output channel.
pub(crate) fn attach_calls(data: &mut Value, calls: Value) {
    let Some(entries) = calls.as_object() else {
        data["next"] = calls;
        return;
    };
    for (name, call) in entries {
        match call["kind"].as_str() {
            Some("related") => attach_related(data, call.clone()),
            _ => {
                data["next"][name] = call.clone();
            }
        }
    }
}

/// Serialize a private typed call bundle into the two public channels.
pub(crate) fn serialize_calls<T: serde::Serialize, S: serde::Serializer>(
    calls: &Option<T>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut data = serde_json::json!({});
    if let Some(calls) = calls {
        let value = serde_json::to_value(calls).map_err(serde::ser::Error::custom)?;
        attach_calls(&mut data, value);
    }
    data.serialize(serializer)
}
