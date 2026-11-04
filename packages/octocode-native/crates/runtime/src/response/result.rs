//! Tool-owned diagnostics remain separate from public domain data, and the
//! one builder of executable follow-up calls.
mod channel;
mod error;
mod typed_row;
pub(crate) use channel::insert_next;
pub use error::{FailureKind, ToolError};
pub(crate) use error::{STALE_SNAPSHOT_ERROR, failure_kind, missing_path_lead};
use serde::Serialize;
use serde_json::Value;
pub(crate) use typed_row::{edited_query, query_row, query_value, wire_query, with_changes};

use crate::id::ToolId;
use octocode_contract_types::ToolQuery;

/// An explicit follow-up: saved work or a fresh related request.
///
/// The call is complete when constructed. Debug and test builds also check
/// it there against its tool's query contract, through the same check
/// publication applies ([`crate::response::continuations::checked_input`]),
/// and panic at the constructing call site: a test that builds a bad call
/// fails at its source instead of failing whichever tool call publishes it.
/// Release builds skip the check; publication still applies it.
#[must_use]
pub(crate) struct Continuation {
    tool: ToolId,
    call: Value,
}

impl Continuation {
    #[track_caller]
    pub fn continued(tool: ToolId, row: Value) -> Self {
        Self::continued_input(tool, serde_json::json!({"queries": [row]}))
    }

    #[track_caller]
    pub fn related(tool: ToolId, row: Value) -> Self {
        Self::related_input(tool, serde_json::json!({"queries": [row]}))
    }

    /// A continuation of saved work: `query` is the row to run, its tool the
    /// one the generated [`ToolQuery`] variant names.
    #[track_caller]
    pub fn continued_query(query: impl Into<ToolQuery>) -> Self {
        Self::from_query(query.into(), false)
    }

    /// A fresh related request: `query` is the row to run, its tool the one
    /// the generated [`ToolQuery`] variant names.
    #[track_caller]
    pub fn related_query(query: impl Into<ToolQuery>) -> Self {
        Self::from_query(query.into(), true)
    }

    #[track_caller]
    fn from_query(query: ToolQuery, related: bool) -> Self {
        let Some(tool) = ToolId::from_name(query.tool_name()) else {
            unreachable!("a generated ToolQuery variant names a runtime tool");
        };
        let row = match query.to_value() {
            Ok(row) => row,
            Err(error) => unreachable!("a generated query is plain data: {error}"),
        };
        Self::new(tool, serde_json::json!({"queries": [row]}), related)
    }

    #[track_caller]
    pub fn continued_input(tool: ToolId, input: Value) -> Self {
        Self::new(tool, input, false)
    }

    #[track_caller]
    pub fn related_input(tool: ToolId, input: Value) -> Self {
        Self::new(tool, input, true)
    }

    /// The call: its input's rows without the fields that hold their
    /// contract default ([`without_defaults`]). A related call is a fresh
    /// public request, so its rows also drop the tool's private progress
    /// and the caller's brief ([`fresh_query_row`]); only continued work
    /// keeps them, for its cursor.
    #[track_caller]
    fn new(tool: ToolId, input: Value, related: bool) -> Self {
        let input = if related {
            without_private_progress(tool, input)
        } else {
            input
        };
        let input = without_defaults(tool, input);
        #[cfg(debug_assertions)]
        {
            let checked = crate::response::continuations::checked_input(tool, related, &input);
            debug_assert!(
                checked.is_ok(),
                "{} call built at {} breaks its query contract: {}",
                tool.as_str(),
                std::panic::Location::caller(),
                checked
                    .as_ref()
                    .err()
                    .and_then(|error| serde_json::to_string(&error.issues).ok())
                    .unwrap_or_default(),
            );
        }
        Self {
            tool,
            call: serde_json::json!({
                "kind": if related { "related" } else { "continue" },
                "tool": tool.as_str(),
                "input": input,
            }),
        }
    }

    /// The tool this call runs.
    pub fn tool(&self) -> ToolId {
        self.tool
    }

    pub fn build(self) -> Value {
        self.call
    }
}

/// `input` with each query row as [`fresh_query_row`].
fn without_private_progress(tool: ToolId, mut input: Value) -> Value {
    if let Some(rows) = input.get_mut("queries").and_then(Value::as_array_mut) {
        for row in rows.iter_mut() {
            *row = fresh_query_row(tool, row);
        }
    }
    input
}

/// `input` with each query row's fields that equal their contract default
/// dropped: parsing re-applies the default, so the call (and a cursor that
/// saves it) runs exactly the same query in fewer bytes. A default is read
/// from the variants of the row's `operation`; a field whose variants
/// disagree or declare none stays. `operation` itself always stays.
fn without_defaults(tool: ToolId, mut input: Value) -> Value {
    let Some(rows) = input.get_mut("queries").and_then(Value::as_array_mut) else {
        return input;
    };
    for row in rows.iter_mut().filter_map(Value::as_object_mut) {
        let operation = row
            .get("operation")
            .and_then(Value::as_str)
            .map(str::to_owned);
        row.retain(|field, value| {
            field == "operation"
                || crate::contracts::query_schema_value(
                    tool,
                    operation.as_deref(),
                    field,
                    "default",
                ) != Some(&*value)
        });
    }
    input
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
                .filter_map(|schema| {
                    crate::contracts::schema_keyword(schema, &input["$defs"], "maxItems")
                        .and_then(Value::as_u64)
                })
                .min()
        })
        .and_then(|rows| usize::try_from(rows).ok())
        .unwrap_or(1)
        .max(1)
}

/// The first row of a continuation call (`input.queries[0]`).
#[cfg(test)]
#[must_use]
pub(crate) fn continuation_row(call: &Value) -> Option<&Value> {
    call.get("input")?.get("queries")?.get(0)
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

/// Canonical public guidance keys.
pub(crate) const PAGES_KEY: &str = "next";
pub(crate) const HINTS_KEY: &str = "hints";

/// What a tool learned beside its data: the coverage gaps that make the
/// row partial and the recovery hints. The row stage writes them through
/// [`crate::response::paging::mark_partial`].
#[derive(Debug, Default)]
pub(crate) struct ToolDiagnostics {
    pub partial_reasons: Vec<crate::response::paging::PartialReason>,
    pub hints: Vec<String>,
}
impl ToolDiagnostics {
    /// A recovery hint, once.
    pub fn hint(&mut self, hint: &str) {
        if !self.hints.iter().any(|value| value == hint) {
            self.hints.push(hint.into());
        }
    }

    /// A coverage gap: the row is partial for `reason`.
    pub fn partial(&mut self, reason: crate::response::paging::PartialReason) {
        if !self.partial_reasons.contains(&reason) {
            self.partial_reasons.push(reason);
        }
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

/// The same semantic request without private saved-work progress or the
/// caller's brief, which is the caller's to state for fresh work.
pub(crate) fn fresh_query_row(tool: ToolId, row: &Value) -> Value {
    let mut fresh = row.clone();
    if let Some(fields) = fresh.as_object_mut() {
        for field in crate::contracts::state_fields(tool) {
            fields.remove(field);
        }
        fields.remove("mainGoal");
        fields.remove("reasoning");
    }
    fresh
}

/// A new request over the same scope; private progress belongs only to the saved work.
#[track_caller]
pub(crate) fn restart_query(tool: ToolId, row: &Value) -> Value {
    Continuation::related(tool, row.clone()).build()
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

/// Assemble a producer's explicitly typed private `calls` into public channels.
pub(crate) fn finish_calls(value: &mut Value) {
    if let Some(calls) = value.as_object_mut().and_then(|row| row.remove("calls")) {
        attach_calls(value, calls);
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
