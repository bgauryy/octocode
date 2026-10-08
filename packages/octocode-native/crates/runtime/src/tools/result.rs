//! Tool-owned diagnostics remain separate from public domain data, and the
//! one builder of executable follow-up calls.
use serde::Serialize;
use serde_json::{Map, Value};

use crate::tools::id::ToolId;

/// One executable follow-up call, `{tool, query, why?, confidence?}`. The
/// `query` is always the complete tool input (`{queries:[row]}` plus any
/// response-window field), so MCP and the CLI run it verbatim. Every `next`
/// page and `hints` lead is built here; debug builds and tests validate each
/// one against the strict input contract when it is built.
#[must_use]
pub struct Continuation {
    tool: ToolId,
    input: Value,
    why: Option<String>,
    confidence: Option<String>,
}

impl Continuation {
    /// A one-row call.
    pub fn new(tool: ToolId, row: Value) -> Self {
        Self::input(tool, serde_json::json!({ "queries": [row] }))
    }

    /// A complete input: rows plus response-window fields.
    pub fn input(tool: ToolId, input: Value) -> Self {
        Self {
            tool,
            input,
            why: None,
            confidence: None,
        }
    }

    pub fn why(mut self, why: impl Into<String>) -> Self {
        self.why = Some(why.into());
        self
    }

    /// `exact` (a replay), or `high`/`medium`/`low` for a judgment.
    pub fn confidence(mut self, confidence: impl Into<String>) -> Self {
        self.confidence = Some(confidence.into());
        self
    }

    pub fn build(self) -> Value {
        #[cfg(any(test, debug_assertions))]
        assert_valid(self.tool, &self.input);
        let mut call = Map::new();
        call.insert("tool".into(), Value::from(self.tool.as_str()));
        call.insert("query".into(), self.input);
        if let Some(why) = self.why {
            call.insert("why".into(), Value::String(why));
        }
        if let Some(confidence) = self.confidence {
            call.insert("confidence".into(), Value::String(confidence));
        }
        Value::Object(call)
    }
}

impl Continuation {
    /// A `{tool, query: row, ...}` call (clasify's resource-read shape) as a
    /// built continuation; other keys ride along. `None` when `call` is not
    /// a one-row call of a known tool.
    pub fn from_row_call(call: &Value) -> Option<Value> {
        let object = call.as_object()?;
        let tool = ToolId::from_name(object.get("tool")?.as_str()?)?;
        let row = object
            .get("query")
            .filter(|row| row.get("queries").is_none())?;
        let mut built = Self::new(tool, row.clone()).build();
        for (key, value) in object {
            if key != "tool" && key != "query" {
                built[key.as_str()] = value.clone();
            }
        }
        Some(built)
    }
}

/// Fails loudly on a continuation the tool it names would reject.
#[cfg(any(test, debug_assertions))]
#[allow(clippy::panic)]
fn assert_valid(tool: ToolId, input: &Value) {
    if let Err(error) = crate::contracts::validate(tool.as_str(), input.clone()) {
        panic!(
            "invalid {} continuation {input}: {:?}",
            tool.as_str(),
            error.issues
        );
    }
}

/// The first row of a continuation call (`query.queries[0]`).
#[must_use]
pub fn continuation_row(call: &Value) -> Option<&Value> {
    call.get("query")?.get("queries")?.get(0)
}

/// [`continuation_row`], mutable.
pub fn continuation_row_mut(call: &mut Value) -> Option<&mut Value> {
    call.get_mut("query")?.get_mut("queries")?.get_mut(0)
}

/// How a call failed, for the caller's exit code and error state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureKind {
    NotFound,
    Authentication,
    Permission,
    RateLimited,
    Execution,
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
}

impl ToolError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            hints: Vec::new(),
            next: None,
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
    pub fn root_policy(
        error: crate::policy::PolicyError,
        requested: &str,
        paths: &crate::policy::path::PathPolicy,
        access_code: &'static str,
    ) -> Self {
        let missing = error.code == crate::policy::PolicyErrorCode::NotFound;
        let mut out = Self::policy(error, access_code);
        if missing && let Some(parent) = paths.nearest_existing_dir(requested) {
            let lead = Continuation::new(
                ToolId::StructureSearch,
                serde_json::json!({"operation": "tree", "path": parent}),
            )
            .build();
            out.next = Some(Box::new(serde_json::json!({ "viewTree": lead })));
        }
        out
    }
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ToolError {}

/// Fields that steer how a call reports, never what it reads; a
/// continuation inherits them from its source row.
pub const INTENT_FIELDS: [&str; 3] = ["mainGoal", "reasoning", "debug"];

#[derive(Debug, Default, Serialize)]
pub struct ToolDiagnostics {
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
pub struct ToolData {
    pub data: Value,
    #[serde(skip)]
    pub diagnostics: ToolDiagnostics,
    #[serde(skip)]
    pub status: Option<&'static str>,
}
impl From<Value> for ToolData {
    fn from(data: Value) -> Self {
        Self {
            data,
            diagnostics: ToolDiagnostics::default(),
            status: None,
        }
    }
}

/// The row of a continuation whose snapshot no longer describes the source:
/// `restart` (the same query from its first page, without its snapshot) is
/// `next.restart`. The runtime keeps that restart and writes the one shared
/// `error` text and clears `isPartial` (`response::pages::restart_stale`).
pub(crate) fn stale_snapshot(restart: Value) -> Value {
    serde_json::json!({
        "status": "error",
        "errorCode": "staleSnapshot",
        "next": {"restart": restart},
    })
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
