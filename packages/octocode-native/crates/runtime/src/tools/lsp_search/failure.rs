//! Typed failures, empty rows, partial marking, and executable `next.*`
//! continuations shared by every `lspSearch` operation.

use super::LspSearchQuery;
use super::render::uri_to_path;
use crate::id::ToolId;
use crate::num::u32_of_usize;
use crate::response::result::Confidence;
use crate::tools::lsp_search::lsp_search_query;
use octocode_engine::error::{Error as EngineError, ErrorCode, ErrorKind};
use serde_json::{Value, json};

/// The code of a failure the request's cancel or deadline caused.
const CANCELLED: &str = "cancelled";

/// A request-level failure `execute` could not render as a row. The
/// dispatcher emits `code`/`message`/`hint`/`retryable` verbatim, so each
/// cause keeps its own recovery instead of one generic "unavailable" code.
#[derive(Clone, Debug, PartialEq)]
pub struct LspFailure {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
    pub hint: &'static str,
    /// The language server cannot answer this scope at all (it was stopped
    /// for its memory), so the row leads to an unverified text search
    /// instead of a retry.
    pub lexical_fallback: bool,
}

/// A timed-out request: the retry is the row's own query (the server keeps
/// loading between calls); the rest of the tip works without the server.
const TIMEOUT_TIP: &str = "Retry once (the server keeps indexing between calls); if it persists, locate candidates with astSearch or localSearch and read exact lines with localFetch.";

const RETRY_OR_FALL_BACK: &str =
    "Retry once; if it persists, re-anchor from localFetch source or use astSearch/localSearch.";

/// The fix for a C-family file clangd cannot build an AST for: tell clangd
/// how this file is compiled. Retrying without that repeats the failure; with
/// it, the next call restarts the stale server by itself.
const INVALID_AST_HINT: &str = "Give clangd a compile command (compile_commands.json, compile_flags.txt or .clangd); the next call reloads it.";

impl LspFailure {
    pub(super) fn invalid_query(message: impl Into<String>) -> Self {
        Self {
            code: "invalidInput",
            message: message.into(),
            retryable: false,
            hint: "Correct the lspSearch fields (see `octocode schema lspSearch`), then retry.",
            lexical_fallback: false,
        }
    }

    /// A relation the source and the server's answers could not establish
    /// (an unresolved base, an ambiguous candidate): asking again gives the
    /// same answer, so it is never retryable.
    pub(super) fn unestablished(message: impl Into<String>) -> Self {
        Self {
            code: "requestFailed",
            message: message.into(),
            retryable: false,
            hint: "Confirm the relation in source: locate the declarations with astSearch and read them with localFetch.",
            lexical_fallback: false,
        }
    }

    /// A request left unanswered past the time a cheaper labelled answer
    /// was willing to wait for it.
    pub(super) fn overdue(message: String) -> Self {
        Self {
            code: "timeout",
            message,
            retryable: true,
            hint: TIMEOUT_TIP,
            lexical_fallback: false,
        }
    }

    /// The caller cancelled or the deadline expired; the dispatcher reports
    /// the runtime cancellation outcome instead of this row.
    pub(super) fn cancelled(message: String) -> Self {
        Self {
            code: CANCELLED,
            message,
            retryable: false,
            hint: "The request was cancelled or exceeded its deadline.",
            lexical_fallback: false,
        }
    }

    /// Whether the request's cancel or deadline caused this failure: such a
    /// failure ends the request instead of falling back.
    pub(super) fn is_cancelled(&self) -> bool {
        self.code == CANCELLED
    }

    /// Path-policy refusal: the same code local reads use, so agents route it
    /// as a sandbox/path problem rather than a missing language server.
    pub(super) fn path_denied(error: crate::policy::PolicyError) -> Self {
        use crate::policy::PolicyErrorCode;
        let hint = match error.code {
            PolicyErrorCode::OutsideAllowedRoots | PolicyErrorCode::SymlinkEscape => {
                crate::response::output::SANDBOX_HINT
            }
            _ => crate::response::output::LIST_FILES_HINT,
        };
        Self {
            code: error.local_error_code("fileAccessFailed"),
            message: error.message,
            retryable: false,
            hint,
            lexical_fallback: false,
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
                code: "timeout",
                message,
                retryable: true,
                hint: TIMEOUT_TIP,
                lexical_fallback: false,
            },
            ErrorKind::ConnectionClosed => Self {
                code: "serverCrashed",
                message,
                retryable: true,
                hint: "The language server exited or its connection failed; retry once (it restarts), then fall back to astSearch/localSearch.",
                lexical_fallback: false,
            },
            ErrorKind::ResourceExhausted => Self {
                code: "serverCrashed",
                message,
                retryable: false,
                hint: "Server hit its memory limit; a retry repeats it. Use the unverified text lead, or astSearch, then localFetch the lines.",
                lexical_fallback: true,
            },
            ErrorKind::Rpc(rpc) => match rpc.code {
                ErrorCode::MethodNotFound => Self {
                    code: "capabilityUnavailable",
                    message,
                    retryable: false,
                    hint: "Use the advertised LSP operations, or fall back to astSearch/localSearch and exact source reads.",
                    lexical_fallback: false,
                },
                ErrorCode::ContentModified
                | ErrorCode::ServerCancelled
                | ErrorCode::RequestCancelled
                | ErrorCode::ServerNotInitialized => Self {
                    code: "requestFailed",
                    message,
                    retryable: true,
                    hint: RETRY_OR_FALL_BACK,
                    lexical_fallback: false,
                },
                ErrorCode::UnknownErrorCode if rpc.is_invalid_ast() => {
                    Self::project_not_configured(message)
                }
                ErrorCode::InvalidParams | ErrorCode::InvalidRequest | ErrorCode::ParseError => {
                    Self {
                        code: "requestFailed",
                        message,
                        retryable: false,
                        hint: "The language server rejected the request; re-anchor from localFetch source or use astSearch/localSearch.",
                        lexical_fallback: false,
                    }
                }
                _ => Self::request_failed(message),
            },
            _ => Self::request_failed(message),
        }
    }

    /// The row a failure the server cannot recover from becomes: the error,
    /// its one hint, and the leads the failed row offers (never a retry).
    pub(super) fn row(&self, query: &LspSearchQuery, canonical_uri: &str) -> Value {
        let mut value = failure(query, canonical_uri, self.code, &self.message, false);
        value["hints"] = json!([self.hint]);
        if lsp_search_query::operation(query) == "callers" {
            let mut references =
                crate::response::result::query_row(query, &["graphDepth", "page", "snapshot"]);
            references["operation"] = json!("references");
            value["next"]["findReferences"] = continuation(references);
        }
        value
    }

    /// clangd holds no AST for the file: its compile command is missing or
    /// cannot be turned into a compiler invocation. Asking again repeats the answer; only
    /// project setup changes it, and the next call picks that up.
    fn project_not_configured(message: String) -> Self {
        Self {
            code: "missingConfiguration",
            message,
            retryable: false,
            hint: INVALID_AST_HINT,
            lexical_fallback: false,
        }
    }

    fn request_failed(message: String) -> Self {
        Self {
            code: "requestFailed",
            message,
            retryable: true,
            hint: RETRY_OR_FALL_BACK,
            lexical_fallback: false,
        }
    }
}

impl From<EngineError> for LspFailure {
    fn from(error: EngineError) -> Self {
        Self::from_engine(&error)
    }
}

/// The static tip of a failure code; `None` when the row's leads and
/// per-call notes say it all (`anchorUnresolved`: `findSimilar`/`read`).
pub(super) fn failure_hint(query: &LspSearchQuery, code: &str) -> Option<&'static str> {
    Some(match code {
        "serverUnavailable"
            if lsp_search_query::operation(query) == "workspaceSymbol"
                && lsp_search_query::path(query).is_none() =>
        {
            "Provide path for a representative workspace source file so Octocode can select its language server."
        }
        // The row's `next.read` (and `next.findText` for a symbol) is the
        // fallback; no static tip repeats it.
        "serverUnavailable" if lsp_search_query::path(query).is_some() => return None,
        "serverUnavailable" => {
            "Use astSearch symbols/match or localSearch for candidates, then localFetch exact source."
        }
        "capabilityUnavailable"
            if matches!(
                lsp_search_query::op(query),
                super::Op::Supertypes | super::Op::Subtypes
            ) =>
        {
            "This server cannot prove type hierarchy; inspect declarations with astSearch and confirm exact source."
        }
        "capabilityUnavailable" => {
            "Use the advertised LSP operations, or fall back to astSearch/localSearch and exact source reads."
        }
        "timeout" => TIMEOUT_TIP,
        "fileTooLarge" => {
            "Use localSearch or astSearch to locate a bounded region, then read that exact source range."
        }
        "anchorUnresolved" => return None,
        _ => "Use astSearch or localSearch to locate candidates, then localFetch exact source.",
    })
}

/// The static tip of an empty category; `None` when the row's reason says
/// it all.
pub(super) fn empty_hint(category: &str, operation: &str) -> Option<&'static str> {
    Some(match category {
        // The anchor resolved: an empty hierarchy is an answer, not a bad anchor.
        "noLocations" if operation == "subtypes" => {
            "Nothing extends or implements this symbol in the files checked; confirm with the name search lead."
        }
        "noLocations" if operation == "supertypes" => {
            "No base type found: a root type has none. Confirm with the definition."
        }
        "noLocations" => {
            "Verify the symbol and anchor; then try references/definition alternatives or exact syntax/text search."
        }
        "noProject" => {
            "Use localSearch or astSearch here, or run lspSearch where its project file (Cargo.toml, package.json) is present."
        }
        "noDiagnostics" => return None,
        "diagnosticsNotPublished" => {
            "Retry once the server has analyzed the file, or run the project's type-check/build for authoritative errors."
        }
        _ => "Use astSearch or localSearch to locate candidates, then localFetch exact source.",
    })
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
        "lsp": { "serverAvailable": server_available },
        "hints": failure_hint(query, code).into_iter().collect::<Vec<_>>()
    });
    if !canonical_uri.is_empty() {
        value["path"] = json!(super::render::uri_to_path(canonical_uri));
    }
    if code == "timeout" {
        value["next"]["retry"] = continuation(lsp_search_query::to_row(query));
    }
    if lsp_search_query::path(query).is_some() {
        attach_recovery_next(&mut value, query);
        match code {
            "serverUnavailable" => attach_text_search(&mut value, query, None),
            // Whatever the server held is lost: the lexical check is all
            // that is left, and no verification stands behind it.
            "serverCrashed" => attach_text_search(&mut value, query, Some(Confidence::Low)),
            _ => {}
        }
    }
    value
}

/// With no language server, the lexical check of a named symbol: a
/// whole-word, case-sensitive localSearch over the workspace (else the
/// file's directory).
fn attach_text_search(value: &mut Value, query: &LspSearchQuery, confidence: Option<Confidence>) {
    let Some(symbol) = lsp_search_query::symbol_name(query).filter(|name| !name.trim().is_empty())
    else {
        return;
    };
    let path = lsp_search_query::path(query)
        .map(uri_to_path)
        .unwrap_or_default();
    let file = std::path::Path::new(&path);
    if file.is_dir() {
        return; // `next.read` already searches the directory.
    }
    let Some(scope) = lsp_search_query::workspace_root(query)
        .map(uri_to_path)
        .or_else(|| file.parent().map(|dir| dir.to_string_lossy().into_owned()))
    else {
        return;
    };
    let lead = text_search_lead(symbol, &scope, &[], None);
    value["next"]["findText"] = match confidence {
        Some(confidence) => lead.confidence(confidence),
        None => lead,
    }
    .build();
}

pub(super) fn empty(
    query: &LspSearchQuery,
    category: &str,
    reason: &str,
    server_available: bool,
) -> Value {
    let mut row = json!({
        "status": "empty",
        "kind": "empty",
        "category": category,
        "reason": reason,
        "lsp": { "serverAvailable": server_available },
    });
    if let Some(hint) = empty_hint(category, lsp_search_query::operation(query)) {
        row["hints"] = json!([hint]);
    }
    // A workspaceRoot-only query has no file: the row names none.
    if let Some(path) = lsp_search_query::path(query) {
        row["path"] = json!(path);
        // Incoming rows get the evidence-based include disclosure instead
        // (see `server_coverage`).
        if category == "noLocations"
            && path.ends_with(".rs")
            && !super::inferred_project::is_incoming(lsp_search_query::operation(query))
            && let Some(includes) = super::server_coverage::package_includes(&uri_to_path(path))
            && let Some(hints) = row["hints"].as_array_mut()
        {
            if !includes.outside.is_empty() {
                hints.push(json!(OUTSIDE_INCLUDE_TIP));
            }
            if !includes.generated.is_empty() && !runs_build_scripts(query) {
                hints.push(json!(BUILD_SCRIPT_TIP));
                // With an outside include as well, the name may be defined
                // there: the ~2 GB rerun is no repair to lead with.
                if includes.outside.is_empty() {
                    row["next"]["includeBuildScripts"] = build_scripts_rerun(query);
                }
            }
        }
    }
    row
}

/// rust-analyzer runs without build scripts by default, so in a package
/// with one, a name the script generates (`include!(concat!(env!("OUT_DIR"),
/// …))`) has no location until they run.
const BUILD_SCRIPT_TIP: &str = "A name a build script generates (include! of OUT_DIR) resolves only with rustContext {buildScripts:true}.";

/// rust-analyzer loads only the package's own directory: what a file
/// `include!`d from outside it defines never resolves.
const OUTSIDE_INCLUDE_TIP: &str = "A name defined in source this package include!s from outside its directory never resolves in rust-analyzer: read it with localSearch.";

/// The same query with rust-analyzer running the crate's build scripts
/// (`rustContext.buildScripts: true`, the rest of its Rust context kept),
/// from the walk's first page.
pub(super) fn build_scripts_rerun(query: &LspSearchQuery) -> Value {
    let mut rerun = lsp_search_query::to_row(query);
    if let Some(fields) = rerun.as_object_mut() {
        fields.retain(|key, value| {
            !value.is_null() && !crate::response::result::CURSOR_FIELDS.contains(&key.as_str())
        });
    }
    let mut context = lsp_search_query::rust_context(query).unwrap_or_else(|| json!({}));
    context["buildScripts"] = json!(true);
    rerun["rustContext"] = context;
    crate::response::result::Continuation::new(crate::id::ToolId::LspSearch, rerun)
        .confidence(Confidence::Medium)
        .build()
}

/// Whether the request already runs build scripts (`rustContext.buildScripts`).
pub(super) fn runs_build_scripts(query: &LspSearchQuery) -> bool {
    lsp_search_query::rust_context(query).is_some_and(|context| context["buildScripts"] == true)
}

/// An exact, executable `lspSearch` continuation.
pub(super) fn continuation(query: Value) -> Value {
    crate::response::result::Continuation::new(ToolId::LspSearch, query)
        .confidence(Confidence::Exact)
        .build()
}

/// Name the row's coverage state where the row reports coverage details
/// (`importerScan`, `textOnlyFiles`, …): `error` for a failure row, `partial`
/// for a row flagged incomplete or carrying a coverage reason, else `complete`.
/// A row without coverage details states no coverage object.
pub(super) fn stamp_coverage(row: &mut Value) {
    let state = if row.get("status").and_then(Value::as_str) == Some("error") {
        "error"
    } else if row.get("isPartial").and_then(Value::as_bool) == Some(true)
        || row.pointer("/coverage/reason").is_some()
    {
        "partial"
    } else {
        "complete"
    };
    if let Some(coverage) = row.get_mut("coverage").and_then(Value::as_object_mut) {
        coverage.entry("state").or_insert_with(|| json!(state));
    }
}

/// An empty diagnostics row whose server pushed nothing within `wait_ms`: the
/// document may still have diagnostics, so the row is partial with a retry.
pub(super) fn mark_diagnostics_unpublished(row: &mut Value, query: &LspSearchQuery, wait_ms: u32) {
    if row.pointer("/kind").and_then(Value::as_str) != Some("empty") {
        return;
    }
    row["category"] = json!("diagnosticsNotPublished");
    row["reason"] = json!(format!(
        "The language server published no diagnostics for this document within {wait_ms} ms."
    ));
    row["hints"] = json!(
        empty_hint("diagnosticsNotPublished", "diagnostic")
            .into_iter()
            .collect::<Vec<_>>()
    );
    partial_with_retry(row, query, "diagnosticsNotPublished", &[]);
}

/// Partial reason of an empty answer on a line where the server reports an
/// error: its analysis of that line may have dropped what was asked about
/// (clang without the CUDA SDK drops every `kernel<<<…>>>(…)` launch).
pub(super) const ANCHOR_LINE_ERROR: &str = "anchorLineError";

/// Mark an empty row partial when an error-severity diagnostic (published
/// shape, see `ops::diagnostic_items`) covers its one-based anchor `line`:
/// the warning names the first such error and counts the rest, and
/// `next.diagnostics` lists them all. Any other row is left as is.
pub(super) fn mark_anchor_line_errors(row: &mut Value, diagnostics: &Value, line: u32) {
    if row.get("kind").and_then(Value::as_str) != Some("empty") {
        return;
    }
    let line = u64::from(line);
    let errors = super::render::items(diagnostics)
        .iter()
        .filter(|item| item.get("severity").and_then(Value::as_str) == Some("error"))
        .filter(|item| {
            let start = item
                .pointer("/displayRange/startLine")
                .and_then(Value::as_u64);
            let end = item
                .pointer("/displayRange/endLine")
                .and_then(Value::as_u64);
            matches!((start, end), (Some(start), Some(end)) if start <= line && line <= end)
        })
        .collect::<Vec<_>>();
    let Some(first) = errors.first() else {
        return;
    };
    // A diagnostic's first line is its claim; later lines are notes and
    // stay in the full list behind `next.diagnostics`.
    let message = first
        .get("message")
        .and_then(Value::as_str)
        .and_then(|message| message.lines().next())
        .unwrap_or_default();
    let source = first
        .get("source")
        .and_then(Value::as_str)
        .map(|source| format!(" ({source})"))
        .unwrap_or_default();
    let more = match errors.len() - 1 {
        0 => String::new(),
        1 => ", plus 1 more error".to_owned(),
        n => format!(", plus {n} more errors"),
    };
    let warning = format!(
        "The language server reports an error on line {line}: {message}{source}{more}. Its analysis of this line is incomplete, so empty is not absence."
    );
    crate::response::paging::mark_partial(row, ANCHOR_LINE_ERROR, &[warning]);
    if let Some(path) = row.get("path").and_then(Value::as_str) {
        row["next"]["diagnostics"] = continuation(json!({"operation": "diagnostic", "path": path}));
    }
}

/// Flag a row as incomplete with a machine-readable reason, human warnings,
/// and an executable `next.retry` of the same query.
pub(super) fn partial_with_retry(
    row: &mut Value,
    query: &LspSearchQuery,
    reason: &str,
    warnings: &[String],
) {
    if !row.is_object() {
        return;
    }
    crate::response::paging::mark_partial(row, reason, warnings);
    row["next"]["retry"] = continuation(lsp_search_query::to_row(query));
}

/// Partial reason of a references or callers row whose alias recovery lost
/// a server request.
pub(super) const ALIAS_RECOVERY_FAILED: &str = "aliasRecoveryFailed";
/// Partial reason of a references or callers row whose verified-importer
/// recovery lost a server request.
pub(super) const IMPORTER_REQUEST_FAILED: &str = "importerRequestFailed";

/// Server requests a recovery stage could not complete. A recovery stage
/// asks the server again from other anchors (aliases, importers); a request
/// that fails there is a gap in the answer, so each one is recorded here and
/// disclosed on the row, never skipped silently. Cancellation is not a
/// failure: it ends the request.
#[derive(Debug, Default)]
pub(super) struct RequestFailures {
    /// Distinct messages with how many requests hit each, in first-seen order.
    distinct: Vec<(String, usize)>,
}

impl RequestFailures {
    /// Record `failure`, or return it when it is a cancellation.
    pub(super) fn record(&mut self, failure: LspFailure) -> Result<(), LspFailure> {
        if failure.is_cancelled() {
            return Err(failure);
        }
        self.push(format!("{} ({})", failure.message, failure.code), 1);
        Ok(())
    }

    /// Record an engine request error.
    pub(super) fn record_engine(&mut self, error: &EngineError) -> Result<(), LspFailure> {
        self.record(LspFailure::from_engine(error))
    }

    fn push(&mut self, message: String, hits: usize) {
        match self.distinct.iter_mut().find(|(seen, _)| *seen == message) {
            Some((_, count)) => *count += hits,
            None => self.distinct.push((message, hits)),
        }
    }

    pub(super) fn extend(&mut self, other: Self) {
        for (message, hits) in other.distinct {
            self.push(message, hits);
        }
    }

    fn total(&self) -> usize {
        self.distinct.iter().map(|(_, count)| count).sum()
    }

    /// Mark `row` partial with `reason`: how many `stage` requests failed
    /// and the most frequent error, plus a retry of the row's query.
    pub(super) fn disclose(
        &self,
        row: &mut Value,
        query: &LspSearchQuery,
        reason: &str,
        stage: &str,
    ) {
        let Some((message, count)) =
            self.distinct
                .iter()
                .fold(None::<&(String, usize)>, |top, entry| match top {
                    Some(top) if top.1 >= entry.1 => Some(top),
                    _ => Some(entry),
                })
        else {
            return;
        };
        let total = self.total();
        let requests = if total == 1 { "request" } else { "requests" };
        let warning = if self.distinct.len() == 1 {
            format!(
                "{stage}: {total} server {requests} failed: {message}; the locations they would add are missing."
            )
        } else {
            format!(
                "{stage}: {total} server {requests} failed ({} distinct errors; most frequent, {count}×: {message}); the locations they would add are missing.",
                self.distinct.len()
            )
        };
        partial_with_retry(row, query, reason, &[warning]);
    }
}

/// `next.nextPage` for a row that has a page after it: this query at that
/// page, pinned by its `snapshot`. A page past the contract's `page` maximum
/// ends the row at a terminal limit instead, with no lead.
pub(super) fn offer_next_page(
    query: &LspSearchQuery,
    row: &mut Value,
    next: Option<super::render::NextPage>,
) {
    let Some(next) = next else {
        return;
    };
    let Some(page) =
        crate::response::paging::next_cursor(row, ToolId::LspSearch, None, "page", next.page)
    else {
        return;
    };
    let mut next_query = lsp_search_query::to_row(query);
    if let Some(object) = next_query.as_object_mut() {
        object.insert("page".into(), json!(page));
        object.insert("snapshot".into(), json!(next.snapshot));
    }
    row["next"]["nextPage"] = continuation(next_query);
}

pub(super) fn with_next(query: &LspSearchQuery, mut value: Value) -> Value {
    if value.pointer("/kind").and_then(Value::as_str) == Some("empty") {
        value["status"] = json!("empty");
        // A workspaceRoot-only query has no file to fall back to reading, and
        // reading source cannot stand in for missing diagnostics.
        let category = value.pointer("/category").and_then(Value::as_str);
        if lsp_search_query::path(query).is_some()
            && !matches!(category, Some("noDiagnostics" | "diagnosticsNotPublished"))
        {
            attach_recovery_next(&mut value, query);
        }
    }
    if let Some(context) = &lsp_search_query::rust_context(query) {
        value["rustContext"] = json!({
            "context": context,
            "fingerprint": super::receipt::rust_fingerprint(context)
        });
    }
    value
}

/// A request whose path did not validate, as a row where its lead says
/// what to do (a directory anchor, a missing path), else the failure.
pub(super) fn path_failure(
    query: &LspSearchQuery,
    paths: &crate::policy::path::PathPolicy,
    failure: LspFailure,
) -> Result<Value, LspFailure> {
    if let Some(row) = directory_anchor(query, paths) {
        return Ok(row);
    }
    if failure.code != crate::policy::PATH_NOT_FOUND {
        return Err(failure);
    }
    let mut row = json!({
        "status": "error",
        "errorCode": failure.code,
        "error": failure.message,
        "lsp": { "serverAvailable": false },
    });
    let requested = lsp_search_query::path(query)
        .map(uri_to_path)
        .unwrap_or_default();
    if let Some(next) = crate::response::result::missing_path_lead(paths, &requested) {
        row["next"] = next;
    }
    Ok(row)
}

/// The `notAFile` row of a request whose file anchor names a directory,
/// leading into it: a whole-word search for the named symbol, else the
/// directory's file listing. `None` when the path is no authorized directory.
pub(super) fn directory_anchor(
    query: &LspSearchQuery,
    paths: &crate::policy::path::PathPolicy,
) -> Option<Value> {
    let decoded = super::render::decode_uri_path(lsp_search_query::path(query)?).ok()?;
    let directory = paths
        .validate(&decoded)
        .ok()
        .filter(|valid| valid.canonical.is_dir())?
        .canonical;
    let name = paths
        .workspace_relative(&directory)
        .unwrap_or_else(|| directory.to_string_lossy().into_owned());
    let mut row = json!({
        "status": "error",
        "errorCode": "notAFile",
        "error": format!("{name} is a directory, not a file."),
        "lsp": { "serverAvailable": false },
    });
    match lsp_search_query::symbol_name(query).filter(|symbol| !symbol.trim().is_empty()) {
        Some(symbol) => {
            row["next"]["findText"] = text_search_lead(symbol, &name, &[], None).build();
        }
        None => {
            row["next"]["list"] = crate::response::result::Continuation::new(
                ToolId::StructureSearch,
                json!({"operation": "files", "path": name}),
            )
            .build();
        }
    }
    Some(row)
}

/// `next.read` recovery: the symbol's match windows when a name anchored
/// the request (not the whole file), otherwise the file's default view.
pub(super) fn attach_recovery_next(value: &mut Value, query: &LspSearchQuery) {
    let path = lsp_search_query::path(query)
        .map(uri_to_path)
        .unwrap_or_default();
    let symbol = lsp_search_query::symbol_name(query).filter(|name| !name.trim().is_empty());
    // A directory (a workspaceSymbol scope) is searched, not fetched.
    if let Some(symbol) = symbol.filter(|_| std::path::Path::new(&path).is_dir()) {
        value["next"]["read"] = text_search_lead(symbol, &path, &[], None)
            .confidence(Confidence::Exact)
            .build();
        return;
    }
    let mut read = json!({ "path": path });
    if let Some(symbol) = symbol {
        read["matchString"] = json!(symbol);
        read["caseMode"] = json!("sensitive");
        read["contextLines"] = json!(3);
    }
    value["next"]["read"] = crate::response::result::Continuation::new(ToolId::LocalFetch, read)
        .confidence(Confidence::Exact)
        .build();
}

/// Lines read on each side of `line` when the symbol is not there.
const ANCHOR_READ_RADIUS: u32 = 5;
/// Lines scanned on each side of `line` for a near-miss name.
const SUGGESTION_RADIUS: usize = 20;

/// Recovery for a `symbolName` that did not resolve: the name elsewhere in
/// the file or the closest identifier near `line` as an executable
/// `next.findSimilar`; without any candidate, a read of the lines around
/// `line` (a `matchString` of the missing name cannot match).
pub(super) fn anchor_recovery(value: &mut Value, query: &LspSearchQuery, source: Option<&str>) {
    let (Some(name), Some(path)) = (
        lsp_search_query::symbol_name(query),
        lsp_search_query::path(query).map(uri_to_path),
    ) else {
        return;
    };
    let hint = lsp_search_query::anchor_line(query).filter(|line| *line > 0);
    if let (Some(source), Some(line)) = (source, hint)
        && order_hint_recovery(value, query, source, name, line)
    {
        return;
    }
    // The name itself elsewhere in the file (a stale `line`) first,
    // declarations before uses; then near-miss spellings. A line the grammar
    // places the name only in comments or strings is no anchor, and the
    // failing line itself would repeat this query.
    let mentions = source
        .map(|source| name_lines(source, name, hint))
        .unwrap_or_default();
    let code = source.and_then(|source| {
        octocode_engine::lsp::resolver::code_occurrence_lines(&path, source, name)
    });
    let exact = mentions
        .iter()
        .copied()
        .filter(|line| {
            hint.is_none_or(|hint| *line != hint as usize)
                && code
                    .as_ref()
                    .is_none_or(|lines| lines.contains(&(line - 1)))
        })
        .collect::<Vec<_>>();
    let near = source
        .map(|source| near_names(&path, source, name, hint))
        .unwrap_or_default();
    let leads = exact
        .iter()
        .map(|line| (name.to_owned(), *line, Confidence::High))
        .chain(
            near.iter()
                .map(|(word, line)| (word.clone(), *line, Confidence::Medium)),
        )
        .take(crate::response::result::rows_per_call(ToolId::LspSearch))
        .collect::<Vec<_>>();
    if let Some(object) = value["next"].as_object_mut() {
        object.remove("read");
    }
    // Named here, but never as an anchorable symbol: the symbol, if any, is
    // declared elsewhere, so the lead searches the workspace for it.
    if leads.is_empty() && !mentions.is_empty() {
        value["next"]["findSimilar"] =
            crate::response::result::Continuation::new(ToolId::LspSearch, workspace_symbol(query))
                .confidence(Confidence::Medium)
                .build();
        return;
    }
    if leads.is_empty()
        && let Some(line) = hint
    {
        // A `line` past the end reads the file's last lines: a range wholly
        // past the end is an error, not a read.
        let last = source.map_or(u32::MAX, |source| {
            u32_of_usize(source.lines().count().max(1))
        });
        let line = line.min(last);
        value["next"]["read"] = crate::response::result::Continuation::new(
            ToolId::LocalFetch,
            json!({
                "path": path,
                "ranges": [format!(
                    "{}-{}",
                    line.saturating_sub(ANCHOR_READ_RADIUS).max(1),
                    line.saturating_add(ANCHOR_READ_RADIUS).min(last)
                )]
            }),
        )
        .confidence(Confidence::Exact)
        .build();
    }
    for (index, (symbol, line, confidence)) in leads.iter().enumerate() {
        let mut retry = lsp_search_query::to_row(query);
        if let Some(object) = retry.as_object_mut() {
            object.insert("symbolName".into(), json!(symbol));
            object.insert("line".into(), json!(line));
            object.remove("orderHint");
        }
        let key = match index {
            0 => "findSimilar".to_owned(),
            n => format!("findSimilar{}", n + 1),
        };
        value["next"][key] = crate::response::result::Continuation::new(ToolId::LspSearch, retry)
            .confidence(*confidence)
            .build();
    }
    // Candidates the leads already carry are not repeated; only lines and
    // names beyond one admitted batch are stated.
    let offered = leads.len();
    let mut notes = Vec::new();
    let uncovered_exact = exact.iter().skip(offered).collect::<Vec<_>>();
    if !uncovered_exact.is_empty() {
        let lines = uncovered_exact
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        notes.push(format!("`{name}` appears on line {lines}."));
    }
    let uncovered_near = near
        .iter()
        .skip(offered.saturating_sub(exact.len()))
        .collect::<Vec<_>>();
    if !uncovered_near.is_empty() {
        let listed = uncovered_near
            .iter()
            .map(|(name, line)| format!("`{name}` (line {line})"))
            .collect::<Vec<_>>()
            .join(", ");
        notes.push(format!("Closest names: {listed}."));
    }
    if let Some(hints) = value["hints"].as_array_mut() {
        hints.extend(notes.into_iter().map(Value::from));
    }
}

/// A `workspaceSymbol` search for the query's name from the same file and
/// server: the fields that operation takes, nothing anchor-specific.
fn workspace_symbol(query: &LspSearchQuery) -> Value {
    let mut search = json!({"operation": "workspaceSymbol"});
    let asked = lsp_search_query::to_row(query);
    for key in ["path", "symbolName", "workspaceRoot", "rustContext"] {
        if let Some(value) = asked.get(key).filter(|value| !value.is_null()) {
            search[key] = value.clone();
        }
    }
    search
}

/// An `orderHint` past the occurrences its line holds: the one lead is the
/// same anchor at the last occurrence (`orderHint` is zero-based), not
/// other lines of the file. `false` when the line does not spell the name
/// (the anchor is stale: the general recovery applies).
fn order_hint_recovery(
    value: &mut Value,
    query: &LspSearchQuery,
    source: &str,
    name: &str,
    line: u32,
) -> bool {
    let Some(order_hint) = lsp_search_query::order_hint(query) else {
        return false;
    };
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let Some(text) = source.lines().nth(line as usize - 1) else {
        return false;
    };
    let count = text
        .match_indices(name)
        .filter(|(at, _)| {
            !text[..*at].chars().next_back().is_some_and(is_word)
                && !text[at + name.len()..].chars().next().is_some_and(is_word)
        })
        .count();
    let Some(last) = u32::try_from(count)
        .ok()
        .and_then(|count| count.checked_sub(1))
    else {
        return false;
    };
    if order_hint <= last {
        return false;
    }
    let mut retry = lsp_search_query::to_row(query);
    retry["orderHint"] = json!(last);
    if let Some(object) = value["next"].as_object_mut() {
        object.remove("read");
    }
    value["next"]["findSimilar"] =
        crate::response::result::Continuation::new(ToolId::LspSearch, retry)
            .confidence(Confidence::High)
            .build();
    true
}

/// Declaration keywords that may precede a declared name on its line.
const DECLARATION_WORDS: &[&str] = &[
    "function",
    "class",
    "interface",
    "type",
    "enum",
    "const",
    "let",
    "var",
    "fn",
    "struct",
    "trait",
    "impl",
    "mod",
    "def",
    "func",
    "static",
    "namespace",
    "module",
];

/// One-based lines that spell `name` as a word: declaration lines first
/// (a declaration keyword just before the name), then the rest, each
/// group nearest `hint` first. One admitted batch becomes suggestions;
/// remaining candidates are stated as line numbers.
fn name_lines(source: &str, name: &str, hint: Option<u32>) -> Vec<usize> {
    if name.trim().is_empty() {
        return Vec::new();
    }
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let hint = hint.map_or(0, |line| line as usize);
    let mut found: Vec<(bool, usize, usize)> = Vec::new();
    for (index, text) in source.lines().enumerate() {
        let declares = text.match_indices(name).any(|(at, _)| {
            let bounded = !text[..at].chars().next_back().is_some_and(is_word)
                && !text[at + name.len()..].chars().next().is_some_and(is_word);
            bounded
                && text[..at]
                    .split(|c: char| !is_word(c))
                    .rfind(|word| !word.is_empty())
                    .is_some_and(|word| DECLARATION_WORDS.contains(&word))
        });
        let mentions = declares
            || text.match_indices(name).any(|(at, _)| {
                !text[..at].chars().next_back().is_some_and(is_word)
                    && !text[at + name.len()..].chars().next().is_some_and(is_word)
            });
        if mentions {
            let line = index + 1;
            found.push((!declares, line.abs_diff(hint), line));
        }
    }
    found.sort_unstable();
    found.into_iter().map(|(_, _, line)| line).collect()
}

/// Up to three distinct identifiers within a small edit distance of `name`,
/// nearest `line` first (the whole file without a hint). A word the
/// grammar places only in comments or strings is no symbol to anchor on.
fn near_names(path: &str, source: &str, name: &str, hint: Option<u32>) -> Vec<(String, usize)> {
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
            let distance = crate::contracts::levenshtein(word, name);
            if distance <= limit && !found.iter().any(|entry| entry.2 == word) {
                let offset = hint.map_or(0, |hint| line.abs_diff(hint));
                found.push((distance, offset, word.to_owned(), line));
            }
        }
    }
    found.sort();
    found
        .into_iter()
        .filter(|(_, _, word, line)| {
            octocode_engine::lsp::resolver::code_occurrence_lines(path, source, word)
                .is_none_or(|lines| lines.contains(&(line - 1)))
        })
        .take(3)
        .map(|(_, _, word, line)| (word, line))
        .collect()
}

/// Partial reasons about *files* the server's project may not see (or, for
/// dynamic dispatch, files it could not tie to the symbol). When every file
/// that spells the name is in the answer (`textOnlyFiles == 0`), such a
/// reason is moot and is not flagged. Cfg-gated code can hide sites inside
/// listed files, so file agreement never clears it.
pub(super) const FILE_LEVEL_REASONS: [&str; 9] = [
    super::inferred_project::REASON,
    super::importers::CAPPED_REASON,
    super::inferred_project::FAILED_REASON,
    super::recovery::ALIAS_SCAN_CAPPED_REASON,
    super::inferred_project::NO_COMPILE_DATABASE_REASON,
    super::server_coverage::BUILD_SCRIPTS_REASON,
    super::server_coverage::OUTSIDE_INCLUDE_REASON,
    super::server_coverage::SHARED_MODULE_REASON,
    super::server_coverage::DYNAMIC_REASON,
];
/// File-level reasons that are fixed per-request caps (a terminal limit).
/// The importer cap is not one: `next.nextImporterPage` verifies the next
/// window of candidates.
const CAPPED_REASONS: [&str; 1] = [super::recovery::ALIAS_SCAN_CAPPED_REASON];
/// At most this many text-only files are listed as one bulk lead.
pub(super) const LISTED_TEXT_FILES: usize = 5;

/// Mark an incoming-direction row partial: coverage reason, a warning, and
/// (when the query names its symbol) a lexical `findText` lead over the
/// request's search scope. `coverage.textOnlyFiles` counts the files that
/// spell the name but are not in the answer; up to five are the lead's rows.
pub(super) fn flag_partial(
    row: &mut Value,
    query: &LspSearchQuery,
    reason: &str,
    warning: Option<&str>,
    scope: &super::scope::Scope,
) {
    let name = lsp_search_query::symbol_name(query).filter(|name| !name.trim().is_empty());
    let text_only = name.and_then(|name| scope.text_only_files(name));
    let agrees = text_only.as_ref().is_some_and(Vec::is_empty);
    // The lead also reads the anchor file when it uses the name on lines
    // the answer misses; it is answered, so it is not counted.
    let lead_files = text_only.clone().map(|mut files| {
        if let Some(anchor) = name.and_then(|name| scope.anchor_gap(name))
            && !files.contains(&anchor)
        {
            files.push(anchor);
        }
        files
    });
    if let Some(payload) = row.as_object_mut() {
        if let Some(files) = &text_only {
            payload.entry("coverage").or_insert_with(|| json!({}))["textOnlyFiles"] =
                json!(files.len());
        }
        if agrees && FILE_LEVEL_REASONS.contains(&reason) {
            return;
        }
        // A named row states its reason once, in `partialReasons`.
        if name.is_none() {
            payload.entry("coverage").or_insert_with(|| json!({}))["reason"] = json!(reason);
        }
    }
    let warnings = warning.map(str::to_owned).into_iter().collect::<Vec<_>>();
    // A warning, not a hint: the hint policy keeps hints for empty/error rows
    // only, and this caveat matters most when the row looks complete.
    match name {
        // Partial only with an executable recovery.
        Some(name) => {
            // A fixed per-request cap: no page or retry reaches the
            // unchecked files, only the lexical lead below.
            if CAPPED_REASONS.contains(&reason) {
                crate::response::paging::terminal_limit(row, reason, &warnings);
            } else {
                crate::response::paging::mark_partial(row, reason, &warnings);
            }
            row["next"]["findText"] =
                text_search_lead(name, &scope.root, &scope.include, lead_files.as_deref()).build();
        }
        // A position anchor has no name to search for: coverage reason and
        // warning only.
        None => crate::response::paging::push_warnings(row, warnings),
    }
}

/// Whether [`flag_partial`] already flagged `row` with `reasons`' reason:
/// the coverage reason of an unnamed row, or a `partialReasons` entry.
pub(super) fn flagged_with(row: &Value, reasons: &[&str]) -> bool {
    row.pointer("/coverage/reason").is_some()
        || row
            .get("partialReasons")
            .and_then(Value::as_array)
            .is_some_and(|listed| {
                listed
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|reason| reasons.contains(&reason))
            })
}

/// How a coverage warning names the lexical check: by the search itself (the
/// `findText` lead, which a partial row without a page always keeps, names
/// the files).
pub(super) const TEXT_CHECK: &str = "a localSearch of the name";

/// Recover known ignored paths and, when unfinished, the unsearched scope.
/// If the combined queries exceed the batch limit, an unfiltered root search
/// covers the known paths, including nested roots and files outside the family.
pub(super) fn ignored_text_lead(
    name: &str,
    scope: &super::scope::Scope,
    files: &[String],
    unfinished: bool,
) -> Value {
    let mut rows = Vec::new();
    for file in files {
        let row = text_search_row(name, file);
        if !rows.contains(&row) {
            rows.push(row);
        }
    }
    let mut root = text_search_row(name, &scope.root);
    if !scope.include.is_empty() {
        root["include"] = json!(scope.include);
    }
    if (unfinished || rows.is_empty()) && !rows.contains(&root) {
        rows.push(root.clone());
    }
    let broad = rows.len() > LISTED_TEXT_FILES;
    if broad {
        let mut recovery = text_search_row(name, &scope.root);
        // A root walk must reach paths that explicit file/root queries reach.
        recovery["defaultExcludes"] = json!(false);
        recovery["includeHidden"] = json!(true);
        rows = vec![recovery];
        if unfinished && !rows.contains(&root) {
            rows.push(root);
        }
    }
    for row in &mut rows {
        row["includeIgnored"] = json!(true);
    }
    crate::response::result::Continuation::input(ToolId::LocalSearch, json!({"queries": rows}))
        .confidence(if broad || unfinished || files.is_empty() {
            Confidence::Medium
        } else {
            Confidence::High
        })
        .build()
}

fn text_search_row(name: &str, path: &str) -> Value {
    json!({
        "path": path,
        "matchString": super::render::word_pattern(name),
        "regex": "rust",
        "caseMode": "sensitive",
    })
}

/// The one lexical lead every lspSearch row builds: one row per listed file
/// when there are a few, otherwise `root` with the language family's globs.
/// Paths are the caller's (workspace-relative as the response names them).
/// The match is the symbol, whole-word and case-sensitive like the scans it
/// stands in for; the word-bounded pattern is a `rust` regex, as an unset
/// `regex` would search its `\b` spelling as literal text. Unbuilt, so a
/// caller can lower its confidence.
pub(super) fn text_search_lead(
    name: &str,
    root: &str,
    include: &[String],
    files: Option<&[String]>,
) -> crate::response::result::Continuation {
    let row = |path: &str| text_search_row(name, path);
    if let Some(files) = files.filter(|files| (1..=LISTED_TEXT_FILES).contains(&files.len())) {
        let rows = files.iter().map(|file| row(file)).collect::<Vec<_>>();
        return crate::response::result::Continuation::input(
            ToolId::LocalSearch,
            json!({ "queries": rows }),
        )
        .confidence(Confidence::High);
    }
    let mut search = row(root);
    if !include.is_empty() {
        search["include"] = json!(include);
    }
    crate::response::result::Continuation::new(ToolId::LocalSearch, search)
        .confidence(Confidence::Medium)
}

/// `files` named as the response names them: relative to the workspace when
/// inside it, as given otherwise.
pub(super) fn shown_paths(
    paths: &crate::policy::path::PathPolicy,
    files: &[String],
) -> Vec<String> {
    files
        .iter()
        .map(|file| {
            paths
                .workspace_relative(file)
                .unwrap_or_else(|| file.clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::cancel::NeverCancel;
    use crate::security::ContentSecurity;
    use crate::tools::local_search::{LocalSearchQuery, execute_local_search};

    /// The lexical lead is a call as given: each of its rows runs through
    /// localSearch and finds the whole word, not the word's `\b` spelling.
    fn run_rows(lead: &Value, root: &std::path::Path) -> Vec<usize> {
        let policy = crate::tools::test_support::workspace_policy(root);
        lead["query"]["queries"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|row| {
                let query: LocalSearchQuery =
                    serde_json::from_value(row.clone()).expect("lead row is a localSearch query");
                execute_local_search(
                    &query,
                    &policy,
                    &ContentSecurity::new(),
                    &NeverCancel,
                    None,
                    None,
                    None,
                )
                .expect("lead runs")
                .files
                .iter()
                .map(|file| file.matches.as_ref().map_or(0, Vec::len))
                .sum()
            })
            .collect()
    }

    #[test]
    fn ignored_recovery_deduplicates_queries_and_reserves_room_for_unfinished_scope() {
        let scope = super::super::scope::Scope::new("/r".into(), vec!["*.ts".into()]);
        let files = (0..4).map(|i| format!("/r/f{i}.js")).collect::<Vec<_>>();
        let mut repeated = files.clone();
        repeated.extend(files.clone());
        let lead = ignored_text_lead("x", &scope, &repeated, true);
        let rows = lead["query"]["queries"].as_array().expect("queries");
        assert_eq!(rows.len(), LISTED_TEXT_FILES);
        for (row, path) in rows.iter().zip(&files) {
            assert_eq!(row["path"], *path);
            assert!(row.get("include").is_none());
            assert_eq!(row["includeIgnored"], true);
        }
        assert_eq!(rows[4]["path"], "/r");
        assert_eq!(rows[4]["include"], json!(["*.ts"]));
        assert_eq!(rows[4]["includeIgnored"], true);

        let scope = super::super::scope::Scope::new("/r".into(), Vec::new());
        let lead = ignored_text_lead("x", &scope, &["/r".into(), "/r".into()], true);
        assert_eq!(
            lead["query"]["queries"].as_array().expect("queries").len(),
            1
        );
    }

    #[test]
    fn ignored_recovery_overflow_keeps_out_of_family_and_nested_paths_searchable() {
        let root = tempfile::tempdir().expect("root");
        std::fs::create_dir(root.path().join(".git")).expect("repository");
        std::fs::write(root.path().join(".gitignore"), "*.js\n").expect("ignore rules");
        let mut files = Vec::new();
        for relative in ["a.js", "b.js", ".hidden/c.js", "dist/d.js"] {
            let path = root.path().join(relative);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            std::fs::write(&path, "needle();\n").expect("source");
            files.push(path.to_string_lossy().into_owned());
        }
        let nested = root.path().join("repos/child");
        std::fs::create_dir_all(nested.join(".git")).expect("nested repository");
        std::fs::write(nested.join("e.js"), "needle();\n").expect("nested source");
        files.push(nested.to_string_lossy().into_owned());
        let scope = super::super::scope::Scope::new(
            root.path().to_string_lossy().into_owned(),
            vec!["*.ts".into()],
        );
        let exact = ignored_text_lead("needle", &scope, &files, false);
        assert_eq!(
            exact["query"]["queries"].as_array().expect("queries").len(),
            5
        );
        assert_eq!(run_rows(&exact, root.path()), vec![1; 5]);

        let unfinished = ignored_text_lead("needle", &scope, &files, true);
        let rows = unfinished["query"]["queries"].as_array().expect("queries");
        assert_eq!(rows.len(), 2);
        assert!(rows[0].get("include").is_none());
        assert_eq!(rows[0]["defaultExcludes"], false);
        assert_eq!(rows[0]["includeHidden"], true);
        assert_eq!(rows[1]["include"], json!(["*.ts"]));
        assert_eq!(run_rows(&unfinished, root.path()), vec![5, 0]);

        let sixth = root.path().join("f.js");
        std::fs::write(&sixth, "needle();\n").expect("sixth source");
        files.push(sixth.to_string_lossy().into_owned());
        let complete = ignored_text_lead("needle", &scope, &files, false);
        assert_eq!(
            complete["query"]["queries"]
                .as_array()
                .expect("queries")
                .len(),
            1
        );
        assert_eq!(run_rows(&complete, root.path()), vec![6]);
    }

    #[test]
    fn the_text_lead_finds_the_word_in_a_scope_and_in_listed_files() {
        let root = tempfile::tempdir().expect("root");
        let file = root.path().join("a.py");
        std::fs::write(&file, "get(1)\ngetter = 2\nx.get\n").expect("source");
        let scope = super::super::scope::Scope::new(
            root.path().to_string_lossy().into_owned(),
            vec!["*.py".into()],
        );
        let wide = text_search_lead("get", &scope.root, &scope.include, None).build();
        assert_eq!(run_rows(&wide, root.path()), vec![2], "{wide}");
        let files = vec![file.to_string_lossy().into_owned()];
        let listed = text_search_lead("get", &scope.root, &scope.include, Some(&files)).build();
        assert_eq!(run_rows(&listed, root.path()), vec![2], "{listed}");
        // A name whose edge is not a word character has no `\b` there.
        std::fs::write(&file, "$store = 1\n$stored = 2\n").expect("source");
        let dollar = text_search_lead("$store", &scope.root, &scope.include, None).build();
        assert_eq!(run_rows(&dollar, root.path()), vec![1], "{dollar}");
    }
}
