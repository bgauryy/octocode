//! Tool-owned diagnostics remain separate from public domain data, and the
//! one builder of executable follow-up calls.
mod channel;
mod error;
mod typed_row;
pub use crate::id::error_codes::ErrorClass;
pub use channel::insert_next;
pub use error::{STALE_SNAPSHOT_ERROR, ToolError, missing_path_lead};
use serde::Serialize;
use serde_json::Value;
pub use typed_row::{edited_query, query_row, query_value, wire_query, with_changes};

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
#[derive(Clone)]
pub struct Continuation {
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
    /// and the caller's brief ([`fresh_query_row`]), and it bills nothing
    /// ([`unbilled`]): a judgment is the caller's to ask for, never a lead's.
    /// Only continued work, the same call's saved page, keeps them, for its
    /// cursor.
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

    /// The call that resumes saved work from its cursor handle: `input` is
    /// the handle alone, so it carries no query row to check. The one
    /// builder of a `"kind":"continue"` call outside [`Self::new`].
    pub fn cursor(tool: &str, cursor: impl Into<Value>) -> Value {
        serde_json::json!({"kind": "continue", "tool": tool, "input": {"cursor": cursor.into()}})
    }

    /// The tool this call runs.
    pub fn tool(&self) -> ToolId {
        self.tool
    }

    pub fn build(self) -> Value {
        self.call
    }
}

/// `input` with each query row as [`fresh_query_row`], [`unbilled`].
fn without_private_progress(tool: ToolId, mut input: Value) -> Value {
    if let Some(rows) = input.get_mut("queries").and_then(Value::as_array_mut) {
        for row in rows.iter_mut() {
            *row = fresh_query_row(tool, row);
            unbilled(row);
        }
    }
    input
}

/// `row` without the fields that ask a classification provider for a
/// judgment ([`crate::contracts::PROVIDER_FIELDS`]): a fresh call a result
/// offers runs unbilled, since a broader or other search judging under the
/// caller's question would bill a scope the caller never asked it of.
fn unbilled(row: &mut Value) {
    if let Some(fields) = row.as_object_mut() {
        for field in crate::contracts::PROVIDER_FIELDS {
            fields.remove(field);
        }
    }
}

/// `input` with each query row as [`canonical_row`]: the call (and a cursor
/// that saves it) runs exactly the same query in fewer bytes.
fn without_defaults(tool: ToolId, mut input: Value) -> Value {
    let Some(rows) = input.get_mut("queries").and_then(Value::as_array_mut) else {
        return input;
    };
    for row in rows.iter_mut() {
        canonical_row(tool, row);
    }
    input
}

/// `row` without the fields that equal their contract default: parsing
/// re-applies the default, so a row stating one and a row omitting it are
/// one query, and compare equal once canonical. A default is read from the
/// variants of the row's `operation`; a field whose variants disagree or
/// declare none stays. `operation` itself always stays.
pub fn canonical_row(tool: ToolId, row: &mut Value) {
    let Some(row) = row.as_object_mut() else {
        return;
    };
    let operation = row
        .get("operation")
        .and_then(Value::as_str)
        .map(str::to_owned);
    row.retain(|field, value| {
        field == "operation"
            || crate::contracts::query_schema_value(tool, operation.as_deref(), field, "default")
                != Some(&*value)
    });
}

/// Rows one `tool` call accepts: the contract's `queries.maxItems`, so a
/// lead that batches rows stays within it.
#[must_use]
pub fn rows_per_call(tool: ToolId) -> usize {
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
#[cfg(any(test, feature = "test-support"))]
#[must_use]
pub fn continuation_row(call: &Value) -> Option<&Value> {
    call.get("input")?.get("queries")?.get(0)
}

/// Canonical public guidance keys.
pub(crate) const PAGES_KEY: &str = "next";
pub(crate) const HINTS_KEY: &str = "hints";

/// What a tool learned beside its data: the coverage gaps that make the
/// row partial and the recovery hints. The row stage writes them through
/// [`crate::response::paging::mark_partial`].
#[derive(Debug, Default)]
pub struct ToolDiagnostics {
    pub partial_reasons: Vec<crate::response::paging::PartialReason>,
    pub hints: Vec<String>,
    /// The row's linear walk continues the page it answered (the rest of
    /// one provider page the row's share of the response window did not
    /// hold), so it is the response's continuation: publication states it as
    /// `responsePagination.next`.
    pub continues_page: bool,
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

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct ToolData {
    pub data: Value,
    #[serde(skip)]
    pub diagnostics: ToolDiagnostics,
    #[serde(skip)]
    pub status: Option<&'static str>,
    /// Leaves of `data` judged against their whole source, for the output
    /// gate ([`crate::security::JudgedLeaves`]): a file's text a read cut
    /// from it is judged there as a local read of the file is.
    #[serde(skip)]
    pub judged: crate::security::JudgedLeaves,
}
impl From<Value> for ToolData {
    fn from(data: Value) -> Self {
        Self {
            data,
            diagnostics: ToolDiagnostics::default(),
            status: None,
            judged: crate::security::JudgedLeaves::default(),
        }
    }
}

/// The same semantic request without private saved-work progress or the
/// caller's brief, which is the caller's to state for fresh work.
pub fn fresh_query_row(tool: ToolId, row: &Value) -> Value {
    let mut fresh = row.clone();
    if let Some(fields) = fresh.as_object_mut() {
        for field in crate::contracts::state_fields(tool) {
            fields.remove(field);
        }
        for field in crate::contracts::BRIEF_FIELDS {
            fields.remove(field);
        }
    }
    fresh
}

/// A new request over the same scope; private progress belongs only to the saved work.
#[track_caller]
pub fn restart_query(tool: ToolId, row: &Value) -> Value {
    Continuation::related(tool, row.clone()).build()
}

/// [`ToolError::stale_snapshot`] as a result row, for a tool that returns
/// rows rather than errors.
pub fn stale_snapshot(restart: Value) -> Value {
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
pub fn remove_nulls(value: &mut Value) {
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
pub fn remove_null_fields(value: &mut Value) {
    if let Value::Object(map) = value {
        map.retain(|_, value| !value.is_null());
        map.values_mut().for_each(remove_null_fields);
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod place_numbered_tests;

#[cfg(any(test, feature = "test-support"))]
pub mod canonical_tests;

/// The one hint writer: appends each of `hints` not already stated under
/// `hints`, creating the list for a first hint and never an empty one. A row
/// that is not an object is left alone.
pub fn add_hints<I>(row: &mut Value, hints: I)
where
    I: IntoIterator,
    I::Item: Into<String>,
{
    let Some(fields) = row.as_object_mut() else {
        return;
    };
    let mut hints = hints.into_iter().map(Into::into).peekable();
    if hints.peek().is_none() {
        return;
    }
    let target = fields
        .entry(HINTS_KEY)
        .or_insert_with(|| serde_json::json!([]));
    if !target.is_array() {
        *target = serde_json::json!([]);
    }
    if let Some(list) = target.as_array_mut() {
        for hint in hints {
            let hint = Value::String(hint);
            if !list.contains(&hint) {
                list.push(hint);
            }
        }
    }
}

/// Replaces the row's hints with `hints`, for a recovery that supersedes
/// what the row stated (a request token's re-authentication over the
/// local-only login hint). No hints leave the row's own untouched.
pub fn set_hints<I>(row: &mut Value, hints: I)
where
    I: IntoIterator,
    I::Item: Into<String>,
{
    let hints = hints.into_iter().map(Into::into).collect::<Vec<String>>();
    let Some(fields) = row.as_object_mut() else {
        return;
    };
    if !hints.is_empty() {
        fields.remove(HINTS_KEY);
        add_hints(row, hints);
    }
}

/// Like [`add_hints`], but `hints` lead the list and the row's own follow.
pub fn prepend_hints<I>(row: &mut Value, hints: I)
where
    I: IntoIterator,
    I::Item: Into<String>,
{
    let Some(fields) = row.as_object_mut() else {
        return;
    };
    let existing = fields.remove(HINTS_KEY);
    add_hints(row, hints);
    let existing = existing.as_ref().and_then(Value::as_array);
    add_hints(
        row,
        existing
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned),
    );
}

/// The one writer of a public `next` call: `call` under `key`, creating the
/// channel for a first call and keeping the others. A row that is not an
/// object is left alone.
pub fn set_next(row: &mut Value, key: &str, call: Value) {
    let Some(fields) = row.as_object_mut() else {
        return;
    };
    let next = fields
        .entry(PAGES_KEY)
        .or_insert_with(|| serde_json::json!({}));
    if !next.is_object() {
        *next = serde_json::json!({});
    }
    next[key] = call;
}

/// `lead` under the next free numbered `prefix` key (`prefix`, `prefix2`, …,
/// [`crate::id::numbered`]) of the row's `calls`, after any call already
/// there: the one writer of a numbered lead, so no caller overwrites another's.
pub fn place_numbered(row: &mut Value, prefix: &str, lead: Value) {
    let mut index = 0;
    while row["calls"]
        .get(crate::id::numbered(prefix, index))
        .is_some()
    {
        index += 1;
    }
    row["calls"][crate::id::numbered(prefix, index)] = lead;
}

/// Adds `call` to the row's `related`, once: a call whose query rows another
/// related call already runs is no second lead. When `call` runs the rows of
/// calls already listed, it takes the first one's place and the rest go.
pub fn attach_related(data: &mut Value, call: Value) {
    if data.get("related").is_none() {
        data["related"] = serde_json::json!([]);
    }
    let Some(related) = data.get_mut("related").and_then(Value::as_array_mut) else {
        return;
    };
    if related.iter().any(|held| runs_all_of(held, &call)) {
        return;
    }
    let at = related.iter().position(|held| runs_all_of(&call, held));
    related.retain(|held| !runs_all_of(&call, held));
    related.insert(at.unwrap_or(related.len()), call);
}

/// Whether running `wider` runs every query row of `narrower`: the same call
/// (tool, kind and every input field) apart from its rows, with each of
/// `narrower`'s rows among `wider`'s. A call without rows is held only by
/// itself.
fn runs_all_of(wider: &Value, narrower: &Value) -> bool {
    fn rows(call: &Value) -> Option<&Vec<Value>> {
        call.pointer("/input/queries").and_then(Value::as_array)
    }
    let without_rows = |call: &Value| {
        let mut call = call.clone();
        if let Some(input) = call.get_mut("input").and_then(Value::as_object_mut) {
            input.remove("queries");
        }
        call
    };
    match (rows(wider), rows(narrower)) {
        (Some(wide), Some(narrow)) => {
            narrow.iter().all(|row| wide.contains(row))
                && without_rows(wider) == without_rows(narrower)
        }
        _ => wider == narrower,
    }
}

/// Assemble a producer's explicitly typed private `calls` into public channels.
/// [`finish_calls`] of an owned value, for a `map` over a row's result.
pub fn finished(mut value: Value) -> Value {
    finish_calls(&mut value);
    value
}

pub fn finish_calls(value: &mut Value) {
    if let Some(calls) = value.as_object_mut().and_then(|row| row.remove("calls")) {
        attach_calls(value, calls);
    }
}

/// Producers declare each call's semantics. Only that explicit kind chooses its output channel.
pub fn attach_calls(data: &mut Value, calls: Value) {
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
pub fn serialize_calls<T: serde::Serialize, S: serde::Serializer>(
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
