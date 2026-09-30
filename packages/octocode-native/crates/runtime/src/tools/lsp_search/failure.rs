//! Typed failures, empty rows, partial marking, and executable `next.*`
//! continuations shared by every `lspSearch` operation.

use super::LspSearchQuery;
use super::render::uri_to_path;
use octocode_engine::error::{Error as EngineError, ErrorCode, ErrorKind};
use serde_json::{Value, json};

/// A request-level failure `execute` could not render as a row. The
/// dispatcher emits `code`/`message`/`hint`/`retryable` verbatim, so each
/// cause keeps its own recovery instead of one generic "unavailable" code.
#[derive(Clone, Debug, PartialEq)]
pub struct LspFailure {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
    pub hint: &'static str,
}

const RETRY_OR_FALL_BACK: &str =
    "Retry once; if it persists, re-anchor from localFetch source or use astSearch/localSearch.";

impl LspFailure {
    pub(super) fn invalid_query(message: impl Into<String>) -> Self {
        Self {
            code: "lsp.invalidQuery",
            message: message.into(),
            retryable: false,
            hint: "Correct the lspSearch fields (see `octocode scheme lspSearch`), then retry.",
        }
    }

    /// The caller cancelled or the deadline expired; the dispatcher reports
    /// the runtime cancellation outcome instead of this row.
    pub(super) fn cancelled(message: String) -> Self {
        Self {
            code: "lsp.cancelled",
            message,
            retryable: false,
            hint: "The request was cancelled or exceeded its deadline.",
        }
    }

    /// Path-policy refusal: the same code local reads use, so agents route it
    /// as a sandbox/path problem rather than a missing language server.
    pub(super) fn path_denied(error: crate::policy::PolicyError) -> Self {
        use crate::policy::PolicyErrorCode;
        let hint = match error.code {
            PolicyErrorCode::OutsideAllowedRoots | PolicyErrorCode::SymlinkEscape => {
                "The path is outside the allowed roots: run from inside the workspace, or add it to ALLOWED_PATHS / WORKSPACE_ROOT."
            }
            _ => {
                "Verify the path with structureSearch operation:\"files\", then retry the exact path."
            }
        };
        Self {
            code: error.local_error_code("fileAccessFailed"),
            message: error.message,
            retryable: false,
            hint,
        }
    }

    /// Classify an engine error by its typed kind (never by message text):
    /// a timeout, a server that exited, an unimplemented method, a stale or
    /// server-cancelled request, and any other request failure each keep
    /// their own code and retry advice.
    pub(super) fn from_engine(error: &EngineError) -> Self {
        let message = error.to_string();
        match error.kind() {
            ErrorKind::Timeout => Self {
                code: "lsp.timeout",
                message,
                retryable: true,
                hint: "Retry once after indexing settles; narrow workspaceRoot if the timeout persists.",
            },
            ErrorKind::ConnectionClosed => Self {
                code: "lsp.serverCrashed",
                message,
                retryable: true,
                hint: "The language server exited or its connection failed; retry once (it restarts), then fall back to astSearch/localSearch.",
            },
            ErrorKind::Rpc(rpc) => match rpc.code {
                ErrorCode::MethodNotFound => Self {
                    code: "lsp.capabilityUnavailable",
                    message,
                    retryable: false,
                    hint: "Use the advertised LSP operations, or fall back to astSearch/localSearch and exact source reads.",
                },
                ErrorCode::ContentModified
                | ErrorCode::ServerCancelled
                | ErrorCode::RequestCancelled
                | ErrorCode::ServerNotInitialized => Self {
                    code: "lsp.requestFailed",
                    message,
                    retryable: true,
                    hint: RETRY_OR_FALL_BACK,
                },
                ErrorCode::InvalidParams | ErrorCode::InvalidRequest | ErrorCode::ParseError => {
                    Self {
                        code: "lsp.requestFailed",
                        message,
                        retryable: false,
                        hint: "The language server rejected the request; re-anchor from localFetch source or use astSearch/localSearch.",
                    }
                }
                _ => Self::request_failed(message),
            },
            _ => Self::request_failed(message),
        }
    }

    fn request_failed(message: String) -> Self {
        Self {
            code: "lsp.requestFailed",
            message,
            retryable: true,
            hint: RETRY_OR_FALL_BACK,
        }
    }
}

impl From<EngineError> for LspFailure {
    fn from(error: EngineError) -> Self {
        Self::from_engine(&error)
    }
}

pub(super) fn failure_hint(query: &LspSearchQuery, code: &str) -> &'static str {
    match code {
        "lsp.serverUnavailable"
            if query.operation() == "workspaceSymbol" && query.uri().is_none() =>
        {
            "Provide uri for a representative workspace source file so Octocode can select its language server."
        }
        "lsp.serverUnavailable" => {
            "Use astSearch symbols/match or localSearch for candidates, then localFetch exact source."
        }
        "lsp.capabilityUnavailable"
            if matches!(query.operation().as_str(), "supertypes" | "subtypes") =>
        {
            "This server cannot prove type hierarchy; inspect declarations with astSearch and confirm exact source."
        }
        "lsp.capabilityUnavailable" => {
            "Use the advertised LSP operations, or fall back to astSearch/localSearch and exact source reads."
        }
        "lsp.anchorUnresolved" => {
            "Read the source, then provide an exact position or a unique symbolName with lineHint."
        }
        "lsp.timeout" => {
            "Retry once after indexing settles; narrow workspaceRoot if the timeout persists."
        }
        "lsp.documentTooLarge" => {
            "Use localSearch or astSearch to locate a bounded region, then read that exact source range."
        }
        _ => "Use astSearch or localSearch to locate candidates, then localFetch exact source.",
    }
}

pub(super) fn empty_hint(category: &str) -> &'static str {
    match category {
        "noLocations" => {
            "Verify the symbol and anchor; then try references/definition alternatives or exact syntax/text search."
        }
        "unsupportedOperation" => "Choose an operation advertised by the lspSearch schema.",
        "noDiagnostics" => {
            "No errors or warnings were reported; confirm with the project's own type-check or build if it matters."
        }
        "diagnosticsNotPublished" => {
            "Retry once the server has analyzed the file, or run the project's type-check/build for authoritative errors."
        }
        _ => "Use astSearch or localSearch to locate candidates, then localFetch exact source.",
    }
}

pub(super) fn failure(
    query: &LspSearchQuery,
    canonical_uri: &str,
    code: &str,
    message: &str,
    server_available: bool,
) -> Value {
    let mut value = json!({
        "status": "error",
        "errorCode": code,
        "error": message,
        "type": query.operation(),
        "lsp": { "serverAvailable": server_available },
        "hints": [failure_hint(query, code)]
    });
    value["uri"] = json!(canonical_uri);
    if code == "lsp.timeout" {
        value["next"]["retry"] = continuation(query_value(query));
    }
    if query.uri().is_some() {
        attach_recovery_next(&mut value, query);
    }
    value
}

pub(super) fn empty(
    query: &LspSearchQuery,
    category: &str,
    reason: &str,
    server_available: bool,
) -> Value {
    json!({
        "status": "empty",
        "type": query.operation(),
        "uri": query.uri(),
        "lsp": { "serverAvailable": server_available },
        "payload": { "kind": "empty", "category": category, "reason": reason },
        "hints": [empty_hint(category)]
    })
}

pub(super) fn query_value(query: &LspSearchQuery) -> Value {
    serde_json::to_value(query).unwrap_or_else(|_| json!({}))
}

/// An exact, executable `lspSearch` continuation.
pub(super) fn continuation(query: Value) -> Value {
    json!({ "tool": "lspSearch", "query": query, "confidence": "exact" })
}

/// Mark a row partial with `reason` and warnings, without a continuation
/// (the caller attaches its own `next.*`).
pub(super) fn push_reason(row: &mut Value, reason: &str, warnings: &[String]) {
    let Some(object) = row.as_object_mut() else {
        return;
    };
    object.insert("isPartial".into(), json!(true));
    if let Some(reasons) = object
        .entry("partialReasons")
        .or_insert_with(|| json!([]))
        .as_array_mut()
    {
        reasons.push(json!(reason));
    }
    if let Some(existing) = object
        .entry("warnings")
        .or_insert_with(|| json!([]))
        .as_array_mut()
    {
        existing.extend(warnings.iter().map(|warning| json!(warning)));
    }
}

/// Flag a row as incomplete with a machine-readable reason, human warnings,
/// and an executable `next.retry` of the same query.
pub(super) fn mark_partial(
    row: &mut Value,
    query: &LspSearchQuery,
    reason: &str,
    warnings: &[String],
) {
    if !row.is_object() {
        return;
    }
    push_reason(row, reason, warnings);
    row["next"]["retry"] = continuation(query_value(query));
}

/// Flag a row as incomplete for a cap that retrying cannot lift: a typed
/// terminal limit (no `next.retry`).
pub(super) fn mark_terminal_limit(row: &mut Value, reason: &str, warnings: &[String]) {
    if !row.is_object() {
        return;
    }
    push_reason(row, reason, warnings);
    row["terminalLimit"] = json!(true);
}

pub(super) fn with_next(query: &LspSearchQuery, mut value: Value) -> Value {
    if value
        .pointer("/pagination/hasMore")
        .and_then(Value::as_bool)
        == Some(true)
    {
        let mut next_query = query_value(query);
        if let Some(object) = next_query.as_object_mut() {
            object.insert(
                "page".into(),
                json!(
                    value
                        .pointer("/pagination/nextPage")
                        .and_then(Value::as_u64)
                        .unwrap_or(2)
                ),
            );
            if let Some(snapshot) = value.get("snapshot").and_then(Value::as_str).or_else(|| {
                value
                    .pointer("/pagination/snapshot")
                    .and_then(Value::as_str)
            }) {
                object.insert("snapshot".into(), json!(snapshot));
            }
        }
        value["next"]["nextPage"] = continuation(next_query);
    } else if value.pointer("/payload/kind").and_then(Value::as_str) == Some("empty") {
        value["status"] = json!("empty");
        // A workspaceRoot-only query has no file to fall back to reading, and
        // reading source cannot stand in for missing diagnostics.
        let category = value.pointer("/payload/category").and_then(Value::as_str);
        if query.uri().is_some()
            && !matches!(category, Some("noDiagnostics" | "diagnosticsNotPublished"))
        {
            attach_recovery_next(&mut value, query);
        }
    }
    if let Some(context) = &query.rust_context() {
        value["rustContext"] = json!({
            "context": context,
            "fingerprint": super::receipt::rust_fingerprint(context)
        });
    }
    value
}

/// `next.readFile` recovery: the symbol's match windows when a name anchored
/// the request (not the whole file), otherwise the file's default view.
pub(super) fn attach_recovery_next(value: &mut Value, query: &LspSearchQuery) {
    let path = query.uri().map(uri_to_path).unwrap_or_default();
    let mut read = json!({ "path": path });
    if let Some(symbol) = query.symbol_name().filter(|name| !name.trim().is_empty()) {
        read["matchString"] = json!(symbol);
        read["matchStringCaseSensitive"] = json!(true);
        read["contextLines"] = json!(3);
    }
    value["next"]["readFile"] = json!({
        "tool": "localFetch",
        "why": "Read the source directly to confirm the symbol and its anchor.",
        "query": read,
        "confidence": "exact"
    });
}

/// Lines read on each side of `lineHint` when the symbol is not there.
const ANCHOR_READ_RADIUS: u32 = 5;
/// Lines scanned on each side of `lineHint` for a near-miss name.
const SUGGESTION_RADIUS: usize = 20;

/// Recovery for a `symbolName` that did not resolve: read the lines around
/// `lineHint` (a `matchString` of the missing name cannot match), and offer
/// the closest identifier near the hint as an executable `next.didYouMean`.
pub(super) fn anchor_recovery(value: &mut Value, query: &LspSearchQuery, source: Option<&str>) {
    let (Some(name), Some(path)) = (query.symbol_name(), query.uri().map(uri_to_path)) else {
        return;
    };
    let hint = query.line_hint().filter(|line| *line > 0);
    if let Some(line) = hint {
        value["next"]["readFile"] = json!({
            "tool": "localFetch",
            "why": format!("`{name}` was not found near line {line}; read the lines around it."),
            "query": {
                "path": path,
                "startLine": line.saturating_sub(ANCHOR_READ_RADIUS).max(1),
                "endLine": line.saturating_add(ANCHOR_READ_RADIUS)
            },
            "confidence": "exact"
        });
    }
    let suggestions = source
        .map(|source| near_names(source, name, hint))
        .unwrap_or_default();
    let Some((best, line)) = suggestions.first() else {
        return;
    };
    let mut retry = query_value(query);
    if let Some(object) = retry.as_object_mut() {
        object.insert("symbolName".into(), json!(best));
        object.insert("lineHint".into(), json!(line));
        object.remove("orderHint");
    }
    value["next"]["didYouMean"] = json!({
        "tool": "lspSearch",
        "query": retry,
        "confidence": "medium"
    });
    let listed = suggestions
        .iter()
        .map(|(name, line)| format!("`{name}` (line {line})"))
        .collect::<Vec<_>>()
        .join(", ");
    if let Some(hints) = value["hints"].as_array_mut() {
        hints.push(json!(format!("Closest names: {listed}.")));
    }
}

/// Up to three distinct identifiers within a small edit distance of `name`,
/// nearest `lineHint` first (the whole file without a hint).
fn near_names(source: &str, name: &str, hint: Option<u32>) -> Vec<(String, usize)> {
    let hint = hint.map(|line| line as usize);
    let limit = (name.chars().count() / 4).max(1);
    let mut found: Vec<(usize, usize, String, usize)> = Vec::new();
    for (index, text) in source.lines().enumerate() {
        let line = index + 1;
        if hint.is_some_and(|hint| line.abs_diff(hint) > SUGGESTION_RADIUS) {
            continue;
        }
        for word in text
            .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
            .filter(|word| !word.is_empty() && *word != name && word.len() <= 256)
        {
            let distance = edit_distance(word, name);
            if distance <= limit && !found.iter().any(|entry| entry.2 == word) {
                let offset = hint.map_or(0, |hint| line.abs_diff(hint));
                found.push((distance, offset, word.to_owned(), line));
            }
        }
    }
    found.sort();
    found
        .into_iter()
        .take(3)
        .map(|(_, _, word, line)| (word, line))
        .collect()
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b = b.chars().collect::<Vec<_>>();
    let mut row = (0..=b.len()).collect::<Vec<_>>();
    for (i, left) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, right) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (above + 1)
                .min(row[j] + 1)
                .min(diagonal + usize::from(left != *right));
            diagonal = above;
        }
    }
    row[b.len()]
}
