//! Typed failures, empty rows, partial marking, and executable `next.*`
//! continuations shared by every `lspSearch` operation.

use super::LspSearchQuery;
use super::render::uri_to_path;
use crate::tools::id::ToolId;
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
            code: "invalidInput",
            message: message.into(),
            retryable: false,
            hint: "Correct the lspSearch fields (see `octocode schema lspSearch`), then retry.",
        }
    }

    /// The caller cancelled or the deadline expired; the dispatcher reports
    /// the runtime cancellation outcome instead of this row.
    pub(super) fn cancelled(message: String) -> Self {
        Self {
            code: "cancelled",
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
                crate::tools::output::SANDBOX_HINT
            }
            _ => crate::tools::output::LIST_FILES_HINT,
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
                code: "timeout",
                message,
                retryable: true,
                hint: "Retry once after indexing settles; narrow workspaceRoot if the timeout persists.",
            },
            ErrorKind::ConnectionClosed => Self {
                code: "serverCrashed",
                message,
                retryable: true,
                hint: "The language server exited or its connection failed; retry once (it restarts), then fall back to astSearch/localSearch.",
            },
            ErrorKind::Rpc(rpc) => match rpc.code {
                ErrorCode::MethodNotFound => Self {
                    code: "capabilityUnavailable",
                    message,
                    retryable: false,
                    hint: "Use the advertised LSP operations, or fall back to astSearch/localSearch and exact source reads.",
                },
                ErrorCode::ContentModified
                | ErrorCode::ServerCancelled
                | ErrorCode::RequestCancelled
                | ErrorCode::ServerNotInitialized => Self {
                    code: "requestFailed",
                    message,
                    retryable: true,
                    hint: RETRY_OR_FALL_BACK,
                },
                ErrorCode::InvalidParams | ErrorCode::InvalidRequest | ErrorCode::ParseError => {
                    Self {
                        code: "requestFailed",
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
            code: "requestFailed",
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
        "serverUnavailable" if query.operation() == "workspaceSymbol" && query.path().is_none() => {
            "Provide path for a representative workspace source file so Octocode can select its language server."
        }
        "serverUnavailable" => {
            "Use astSearch symbols/match or localSearch for candidates, then localFetch exact source."
        }
        "capabilityUnavailable"
            if matches!(query.operation().as_str(), "supertypes" | "subtypes") =>
        {
            "This server cannot prove type hierarchy; inspect declarations with astSearch and confirm exact source."
        }
        "capabilityUnavailable" => {
            "Use the advertised LSP operations, or fall back to astSearch/localSearch and exact source reads."
        }
        "anchorUnresolved" => {
            "Read the source, then pass the exact symbolName with its 1-based lineHint (orderHint for a repeat)."
        }
        "timeout" => {
            "Retry once after indexing settles; narrow workspaceRoot if the timeout persists."
        }
        "fileTooLarge" => {
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
        "lsp": { "serverAvailable": server_available },
        "hints": [failure_hint(query, code)]
    });
    value["path"] = json!(super::render::uri_to_path(canonical_uri));
    if code == "timeout" {
        value["next"]["retry"] = continuation(query_value(query));
    }
    if query.path().is_some() {
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
        "path": query.path(),
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
    crate::tools::result::Continuation::new(ToolId::LspSearch, query)
        .confidence("exact")
        .build()
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
        // The continuation states the next page and the snapshot; `pageSize`
        // stays a page fact, so every page has one key set (B12).
        if let Some(pagination) = value.get_mut("pagination").and_then(Value::as_object_mut) {
            for key in ["nextPage", "snapshot"] {
                pagination.shift_remove(key);
            }
        }
    } else if value.pointer("/payload/kind").and_then(Value::as_str) == Some("empty") {
        value["status"] = json!("empty");
        // A workspaceRoot-only query has no file to fall back to reading, and
        // reading source cannot stand in for missing diagnostics.
        let category = value.pointer("/payload/category").and_then(Value::as_str);
        if query.path().is_some()
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

/// `next.read` recovery: the symbol's match windows when a name anchored
/// the request (not the whole file), otherwise the file's default view.
pub(super) fn attach_recovery_next(value: &mut Value, query: &LspSearchQuery) {
    let path = query.path().map(uri_to_path).unwrap_or_default();
    let mut read = json!({ "path": path });
    if let Some(symbol) = query.symbol_name().filter(|name| !name.trim().is_empty()) {
        read["matchString"] = json!(symbol);
        read["caseMode"] = json!("sensitive");
        read["contextLines"] = json!(3);
    }
    value["next"]["read"] = crate::tools::result::Continuation::new(ToolId::LocalFetch, read)
        .why("Read the source directly to confirm the symbol and its anchor.")
        .confidence("exact")
        .build();
}

/// Lines read on each side of `lineHint` when the symbol is not there.
const ANCHOR_READ_RADIUS: u32 = 5;
/// Lines scanned on each side of `lineHint` for a near-miss name.
const SUGGESTION_RADIUS: usize = 20;

/// Recovery for a `symbolName` that did not resolve: read the lines around
/// `lineHint` (a `matchString` of the missing name cannot match), and offer
/// the closest identifier near the hint as an executable `next.didYouMean`.
pub(super) fn anchor_recovery(value: &mut Value, query: &LspSearchQuery, source: Option<&str>) {
    let (Some(name), Some(path)) = (query.symbol_name(), query.path().map(uri_to_path)) else {
        return;
    };
    let hint = query.line_hint().filter(|line| *line > 0);
    if let Some(line) = hint {
        value["next"]["read"] = crate::tools::result::Continuation::new(
            ToolId::LocalFetch,
            json!({
                "path": path,
                "ranges": [format!(
                    "{}-{}",
                    line.saturating_sub(ANCHOR_READ_RADIUS).max(1),
                    line.saturating_add(ANCHOR_READ_RADIUS)
                )]
            }),
        )
        .why(format!(
            "`{name}` was not found near line {line}; read the lines around it."
        ))
        .confidence("exact")
        .build();
    }
    // The name itself elsewhere in the file (a stale `lineHint`) first,
    // declarations before uses; then near-miss spellings.
    let exact = source
        .map(|source| name_lines(source, name, hint))
        .unwrap_or_default();
    let near = source
        .map(|source| near_names(source, name, hint))
        .unwrap_or_default();
    let leads = exact
        .iter()
        .map(|line| (name.to_owned(), *line, "high"))
        .chain(
            near.iter()
                .map(|(word, line)| (word.clone(), *line, "medium")),
        )
        .take(MAX_ANCHOR_LEADS)
        .collect::<Vec<_>>();
    for (index, (symbol, line, confidence)) in leads.iter().enumerate() {
        let mut retry = query_value(query);
        if let Some(object) = retry.as_object_mut() {
            object.insert("symbolName".into(), json!(symbol));
            object.insert("lineHint".into(), json!(line));
            object.remove("orderHint");
        }
        let key = match index {
            0 => "didYouMean".to_owned(),
            n => format!("didYouMean{}", n + 1),
        };
        value["next"][key] = crate::tools::result::Continuation::new(ToolId::LspSearch, retry)
            .confidence(*confidence)
            .build();
    }
    let mut notes = Vec::new();
    if !exact.is_empty() {
        let lines = exact
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        notes.push(format!("`{name}` appears on line {lines}."));
    }
    if !near.is_empty() {
        let listed = near
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

/// Re-anchor leads one unresolved anchor offers.
const MAX_ANCHOR_LEADS: usize = 3;

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
/// group nearest `hint` first; at most [`MAX_ANCHOR_LEADS`].
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
    found
        .into_iter()
        .take(MAX_ANCHOR_LEADS)
        .map(|(_, _, line)| line)
        .collect()
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
        .take(3)
        .map(|(_, _, word, line)| (word, line))
        .collect()
}

/// Partial reasons about *files* the server's project may not see. When
/// every file that spells the name is in the answer (`textOnlyFiles == 0`),
/// such a reason is moot and is not flagged. Site-level reasons (dynamic
/// dispatch, cfg-gated code) can hide sites inside listed files, so file
/// agreement never clears them.
pub(super) const FILE_LEVEL_REASONS: [&str; 5] = [
    "inferredProject",
    "importerScanCapped",
    "importerScanFailed",
    "aliasScanCapped",
    "noCompileDatabase",
];
/// File-level reasons that are fixed per-request caps (a terminal limit).
/// The importer cap is not one: `next.nextImporterPage` verifies the next
/// window of candidates.
const CAPPED_REASONS: [&str; 1] = ["aliasScanCapped"];
/// At most this many text-only files are listed as one bulk lead.
const LISTED_TEXT_FILES: usize = 5;

/// Mark an incoming-direction row partial: coverage reason, a warning, and
/// (when the query names its symbol) a lexical `textSearch` lead over the
/// request's search scope. `coverage.textOnlyFiles` counts the files that
/// spell the name but are not in the answer; up to five are the lead's rows.
pub(super) fn flag_partial(
    row: &mut Value,
    query: &LspSearchQuery,
    reason: &str,
    warning: &str,
    scope: &super::scope::Scope,
) {
    let name = query.symbol_name().filter(|name| !name.trim().is_empty());
    let text_only = name.and_then(|name| scope.text_only_files(name));
    let agrees = text_only.as_ref().is_some_and(Vec::is_empty);
    if let Some(payload) = row.get_mut("payload").and_then(Value::as_object_mut) {
        let coverage = payload
            .entry("coverage")
            .or_insert_with(|| json!({"scope":"languageServer","exhaustive":false}));
        coverage["exhaustive"] = json!(false);
        if let Some(files) = &text_only {
            coverage["textOnlyFiles"] = json!(files.len());
        }
        if agrees && FILE_LEVEL_REASONS.contains(&reason) {
            return;
        }
        coverage["reason"] = json!(reason);
    }
    // A warning, not a hint: the hint policy keeps hints for empty/error rows
    // only, and this caveat matters most when the row looks complete.
    match name {
        // Partial only with an executable recovery.
        Some(name) => {
            push_reason(row, reason, &[warning.to_owned()]);
            row["next"]["textSearch"] = text_search_lead(name, scope, text_only.as_deref());
            if CAPPED_REASONS.contains(&reason) {
                // A fixed per-request cap: no page or retry reaches the
                // unchecked files, only the lexical lead above.
                row["terminalLimit"] = json!(true);
            }
        }
        // A position anchor has no name to search for: coverage reason and
        // warning only.
        None => match row.get_mut("warnings").and_then(Value::as_array_mut) {
            Some(warnings) => warnings.push(json!(warning)),
            None => row["warnings"] = json!([warning]),
        },
    }
}

/// The lexical lead: one row per text-only file when there are a few,
/// otherwise the whole scope with the language family's globs.
fn text_search_lead(
    name: &str,
    scope: &super::scope::Scope,
    text_only: Option<&[String]>,
) -> Value {
    let pattern = super::render::word_pattern(name);
    if let Some(files) = text_only.filter(|files| (1..=LISTED_TEXT_FILES).contains(&files.len())) {
        let rows = files
            .iter()
            .map(|file| json!({ "path": file, "matchString": pattern }))
            .collect::<Vec<_>>();
        return crate::tools::result::Continuation::input(
            ToolId::LocalSearch,
            json!({ "queries": rows }),
        )
        .why("Search the files that spell the name but are missing from the language server's answer.")
        .confidence("high")
        .build();
    }
    let mut search = json!({ "path": scope.root, "matchString": pattern });
    if !scope.include.is_empty() {
        search["include"] = json!(scope.include);
    }
    crate::tools::result::Continuation::new(ToolId::LocalSearch, search)
        .why("Find textual uses the language server's project cannot see.")
        .confidence("medium")
        .build()
}
