//! Failure rows: server errors on the anchor line, typed failure codes,
//! setup failures and the tip the response stage states for each.

use super::*;

/// The tip the response stage states for `failure` on a row of `query`:
/// the one hint hook, keyed by the failure's code (shared codes first, then
/// lspSearch's own), never a tip the failure carries.
fn tip(failure: &LspFailure, query: &serde_json::Value) -> &'static str {
    crate::response::rows::error_code_hint(crate::id::ToolId::LspSearch, failure.code, query)
        .unwrap_or_else(|| panic!("no tip for {:?}", failure.code))
}

/// An empty answer on a line the server itself failed to analyze is not
/// absence: the row is partial and names that error. Errors on other lines,
/// warnings, and answers with results are left alone.
#[test]
fn an_empty_row_names_the_server_error_on_its_anchor_line() {
    let q = query(serde_json::json!({
        "operation": "definition", "path": "/repo/k.cu",
        "symbolName": "scale", "line": 7
    }));
    let (diagnostics, _) = diagnostic_items(Some(serde_json::json!([
        {"severity": 1, "source": "clang", "message": "Use of undeclared identifier 'cudaConfigureCall'\n\nnote: here",
            "range": {"start": {"line": 6, "character": 2}, "end": {"line": 6, "character": 7}}},
        {"severity": 1, "message": "Unknown type name 'dim3'",
            "range": {"start": {"line": 5, "character": 0}, "end": {"line": 7, "character": 1}}},
        {"severity": 2, "message": "unused variable",
            "range": {"start": {"line": 6, "character": 0}, "end": {"line": 6, "character": 1}}},
        {"severity": 1, "message": "elsewhere",
            "range": {"start": {"line": 1, "character": 0}, "end": {"line": 1, "character": 1}}}
    ])));
    let mut row = empty(
        &q,
        "noLocations",
        "definitionProvider returned no locations",
        true,
    );
    mark_anchor_line_errors(&mut row, &diagnostics, 7);
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["anchorLineError"]),
        "{row}"
    );
    let warnings = row["warnings"].as_array().expect("warnings");
    assert_eq!(warnings.len(), 1, "{row}");
    let warning = warnings[0].as_str().expect("text");
    // The first error on the line, whole (its first line), with its source;
    // the other error spanning the line is counted.
    assert!(warning.contains("line 7"), "{warning}");
    assert!(
        warning.contains("Use of undeclared identifier 'cudaConfigureCall' (clang)"),
        "{warning}"
    );
    assert!(warning.contains("1 more error"), "{warning}");
    assert!(!warning.contains("note: here"), "{warning}");
    assert!(
        !warning.contains("unused variable") && !warning.contains("elsewhere"),
        "{warning}"
    );
    // The whole list stays one call away.
    let lead = &row["next"]["diagnostics"];
    assert_eq!(lead["tool"], "lspSearch", "{row}");
    assert_eq!(
        lead["query"]["queries"][0],
        serde_json::json!({"operation": "diagnostic", "path": "/repo/k.cu"}),
        "{row}"
    );
    // `next.diagnostics` is not a kind the contract declares yet (a core
    // rename to `viewDiagnostics` is pending), so the published-row name
    // check is not run here.
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("the marked empty row satisfies the output contract");

    // No error on the anchor line: the row is untouched.
    let mut clean = empty(
        &q,
        "noLocations",
        "definitionProvider returned no locations",
        true,
    );
    let before = clean.clone();
    mark_anchor_line_errors(&mut clean, &diagnostics, 4);
    assert_eq!(clean, before);
    // A row with results is never marked.
    let mut found = serde_json::json!({"kind": "definition", "matches": [{"path": "/repo/k.cu"}]});
    let before = found.clone();
    mark_anchor_line_errors(&mut found, &diagnostics, 7);
    assert_eq!(found, before);
}

#[test]
fn path_policy_denials_are_file_access_failures_not_missing_servers() {
    let denied = LspFailure::path_denied(crate::policy::PolicyError::new(
        crate::policy::PolicyErrorCode::OutsideAllowedRoots,
        "Path '/etc/hosts' is outside allowed directories",
    ));
    assert_eq!(denied.code, ErrorCode::OutsideAllowedRoots);
    assert!(!denied.retryable);
    assert!(
        tip(&denied, &serde_json::Value::Null).contains("ALLOWED_PATHS"),
        "{}",
        tip(&denied, &serde_json::Value::Null)
    );
    let missing = LspFailure::path_denied(crate::policy::PolicyError::new(
        crate::policy::PolicyErrorCode::NotFound,
        "missing",
    ));
    assert_eq!(missing.code, ErrorCode::PathNotFound);
    // Every other refusal sends the caller to verify the path, as local
    // reads do; none is told to fall back to a search for candidates.
    for code in [
        crate::policy::PolicyErrorCode::NotFound,
        crate::policy::PolicyErrorCode::NotRegular,
        crate::policy::PolicyErrorCode::BinaryContent,
        crate::policy::PolicyErrorCode::Io,
    ] {
        let refused =
            LspFailure::path_denied(crate::policy::PolicyError::new(code, "refused"));
        let text = tip(&refused, &serde_json::Value::Null);
        assert!(
            text.contains("structureSearch") && text.contains("retry the exact path"),
            "{code:?}: {text}"
        );
    }
}

#[test]
fn engine_errors_map_to_typed_failures_not_one_unavailable_code() {
    use octocode_engine::error::{Error, RpcError};
    let rpc = |code: i64, data: serde_json::Value| {
        LspFailure::from(Error::rpc(RpcError::from_value(serde_json::json!({
            "code": code,
            "message": "server said no",
            "data": data
        }))))
    };
    let timeout = LspFailure::from(Error::timeout("request timed out"));
    assert_eq!(
        (timeout.code, timeout.retryable),
        (ErrorCode::Timeout, true)
    );
    let closed = LspFailure::from(Error::connection_closed("LSP connection closed"));
    assert_eq!(
        (closed.code, closed.retryable),
        (ErrorCode::ServerCrashed, true)
    );
    let missing = rpc(-32601, serde_json::Value::Null);
    assert_eq!(
        (missing.code, missing.retryable),
        (ErrorCode::CapabilityUnavailable, false)
    );
    for code in [-32801, -32802] {
        let stale = rpc(code, serde_json::Value::Null);
        assert_eq!(
            (stale.code, stale.retryable),
            (ErrorCode::RequestFailed, true)
        );
    }
    let invalid = rpc(-32602, serde_json::Value::Null);
    assert_eq!(
        (invalid.code, invalid.retryable),
        (ErrorCode::RequestFailed, false)
    );
    let other = LspFailure::from(engine_error("boom"));
    assert_eq!(
        (other.code, other.retryable),
        (ErrorCode::RequestFailed, true)
    );
    assert!(timeout.message.contains("timed out"));
}

/// clangd answers `invalid AST` (-32001) for a file it holds no compile
/// command for. Asking again repeats it, so it is a typed setup failure whose
/// hint names the project files that fix it, not a retryable generic one;
/// any other -32001 stays what it was.
#[test]
fn clangd_invalid_ast_is_a_non_retryable_setup_failure_naming_the_fix() {
    use octocode_engine::error::{Error, RpcError};
    let rpc = |message: &str| {
        LspFailure::from(Error::rpc(RpcError::from_value(serde_json::json!({
            "code": -32001,
            "message": message
        }))))
    };
    let invalid = rpc("invalid AST");
    assert_eq!(
        (invalid.code, invalid.retryable, invalid.lexical_fallback),
        (ErrorCode::MissingConfiguration, false, false)
    );
    let hint = tip(&invalid, &serde_json::Value::Null);
    assert!(!hint.contains("Retry once"), "{hint}");
    for fix in ["compile_commands.json", "compile_flags.txt", ".clangd"] {
        assert!(hint.contains(fix), "{fix}: {hint}");
    }
    assert!(
        invalid.message.contains("invalid AST"),
        "{}",
        invalid.message
    );

    let other = rpc("something else broke");
    assert_eq!(
        (other.code, other.retryable),
        (ErrorCode::RequestFailed, true)
    );
    let other_code = LspFailure::from(Error::rpc(RpcError::from_value(serde_json::json!({
        "code": -32603,
        "message": "invalid AST"
    }))));
    assert_eq!(
        (other_code.code, other_code.retryable),
        (ErrorCode::RequestFailed, true),
        "only clangd's -32001 is the invalid-AST answer"
    );
}

/// A failure's tip names only fields the caller's query can carry: the
/// schema's `workspaceRoot` is for `workspaceSymbol` without a path, so a
/// tip for any other operation must not send the caller to narrow it.
#[test]
fn failure_tips_never_name_workspace_root_outside_workspace_symbol() {
    use octocode_engine::error::Error;
    let anchored = query(serde_json::json!({
        "operation": "implementation", "path": "/repo/src/lib.ts",
        "symbolName": "IDisposable", "line": 3
    }));
    let engine = [
        Error::timeout("request timed out"),
        Error::connection_closed("LSP connection closed"),
        Error::resource_exhausted("server stopped for its memory"),
    ];
    let row = lsp_search_query::to_row(&anchored);
    let mut tips = engine
        .iter()
        .map(|error| tip(&LspFailure::from_engine(error), &row).to_owned())
        .collect::<Vec<_>>();
    tips.push(tip(&LspFailure::overdue("no answer".into()), &row).to_owned());
    for code in [
        ErrorCode::Timeout,
        ErrorCode::ServerUnavailable,
        ErrorCode::CapabilityUnavailable,
        ErrorCode::FileTooLarge,
        // Any other code: the generic tip.
        ErrorCode::RequestFailed,
        ErrorCode::AnchorUnresolved,
    ] {
        tips.extend(
            crate::id::ToolId::LspSearch
                .output()
                .error_hint(code, &row)
                .map(str::to_owned),
        );
    }
    for tip in tips {
        assert!(!tip.contains("workspaceRoot"), "{tip}");
    }
}

/// A timeout's tip leads somewhere runnable: the retry it names, and the
/// tools that locate and read the candidates without the server.
#[test]
fn a_timeout_tip_names_a_retry_and_the_tools_that_work_without_the_server() {
    let timeout = LspFailure::from_engine(&octocode_engine::error::Error::timeout("slow"));
    let text = tip(&timeout, &serde_json::Value::Null);
    for tool in ["Retry", "astSearch", "localSearch", "localFetch"] {
        assert!(text.contains(tool), "{text}");
    }
}

/// One code stands for retryable and final causes alike (a stale request,
/// a rejected one, a relation the source could not establish; a closed
/// connection, a memory kill), so its one tip names the row's `retryable`
/// flag, the retry it allows, and the reads that work without the server.
/// Every typed failure gets a tip of its own code, never the generic one.
#[test]
fn each_failure_code_gets_its_own_tip_naming_retry_and_the_fallback() {
    use octocode_engine::error::{Error, RpcError};
    let rpc = |code: i64, message: &str| {
        LspFailure::from(Error::rpc(RpcError::from_value(serde_json::json!({
            "code": code, "message": message
        }))))
    };
    let generic = crate::id::ToolId::LspSearch
        .output()
        .error_hint(ErrorCode::ExecutionFailed, &serde_json::Value::Null)
        .expect("generic tip");
    let failures = [
        LspFailure::invalid_query("lspSearch requires path or workspaceRoot"),
        LspFailure::unestablished("base type unresolved"),
        rpc(-32801, "content modified"),
        rpc(-32602, "bad params"),
        LspFailure::from(engine_error("boom")),
        LspFailure::from(Error::connection_closed("LSP connection closed")),
        LspFailure::from(Error::resource_exhausted("memory cap exceeded")),
        rpc(-32001, "invalid AST"),
        rpc(-32601, "method not found"),
        LspFailure::overdue("no answer".into()),
    ];
    for failure in &failures {
        let text = tip(failure, &serde_json::Value::Null);
        assert_ne!(text, generic, "{failure:?}");
        assert!(
            text.chars().count() <= crate::response::rows::MAX_GUIDANCE_CHARS,
            "{text}"
        );
    }
    for failure in failures.iter().filter(|failure| {
        matches!(failure.code, ErrorCode::RequestFailed | ErrorCode::ServerCrashed)
    }) {
        let text = tip(failure, &serde_json::Value::Null);
        for word in ["Retry once if retryable", "astSearch", "localFetch"] {
            assert!(text.contains(word), "{word}: {text}");
        }
    }
    let invalid = tip(&failures[0], &serde_json::Value::Null);
    assert!(invalid.contains("octocode schema lspSearch"), "{invalid}");
    assert!(!invalid.contains("Retry once"), "{invalid}");
}

/// A language server killed for its memory is a final answer, not a retry:
/// the text names no setting a caller cannot pass, and the row leaves a
/// bounded fallback, an unverified whole-word text search of the symbol.
#[test]
fn a_memory_exhausted_server_is_final_and_leaves_an_unverified_text_lead() {
    let error = octocode_engine::error::Error::resource_exhausted(
        "language server exceeded memory cap (4200 MiB resident > 4096 MiB); it was killed.",
    );
    let failure = LspFailure::from_engine(&error);
    assert_eq!(failure.code, ErrorCode::ServerCrashed);
    assert!(!failure.retryable, "{failure:?}");
    let hint = tip(&failure, &serde_json::Value::Null);
    for text in [failure.message.as_str(), hint] {
        assert!(!text.contains("maxMemoryMb"), "{text}");
    }
    assert!(hint.contains("unverified"), "{hint}");

    let query = query(serde_json::json!({
        "operation": "callers", "path": "/repo/src/lib.rs", "symbolName": "greet", "line": 3,
        "graphDepth": 3, "importerPage": 2, "page": 2, "pageSize": 4,
        "snapshot": "caller-snapshot", "workspaceRoot": "/repo", "orderHint": 1
    }));
    let bare = failure.row(&query, "file:///repo/src/lib.rs");
    assert!(bare.get("hints").is_none(), "the tip is the stage's: {bare}");
    let row = with_stage_hints(&query, &bare);
    assert_eq!(row["errorCode"], "serverCrashed", "{row}");
    assert_eq!(
        row["hints"],
        serde_json::json!([tip(&failure, &lsp_search_query::to_row(&query))]),
        "{row}"
    );
    assert!(row["next"].get("retry").is_none(), "{row}");
    let text = &row["next"]["findText"];
    assert_eq!(text["tool"], "localSearch", "{row}");
    assert_eq!(text["confidence"], "low", "{row}");
    let search = &text["query"]["queries"][0];
    assert_eq!(search["matchString"], "\\bgreet\\b", "{row}");
    assert_eq!(search["regex"], "rust", "{row}");
    assert_eq!(search["caseMode"], "sensitive", "{row}");
    // callers also leads to references of the same anchor, which a server
    // that cannot walk the call hierarchy can often still answer.
    let references = &row["next"]["findReferences"];
    assert_eq!(references["tool"], "lspSearch", "{row}");
    let rerun = &references["query"]["queries"][0];
    assert_eq!(rerun["operation"], "references", "{row}");
    assert_eq!(rerun["symbolName"], "greet", "{row}");
    assert_eq!(rerun["line"], 3, "{row}");
    assert_eq!(rerun["path"], "/repo/src/lib.rs", "{row}");
    assert_eq!(rerun["workspaceRoot"], "/repo", "{row}");
    assert_eq!(rerun["orderHint"], 1, "{row}");
    assert_eq!(rerun["importerPage"], 2, "{row}");
    assert_eq!(rerun["pageSize"], 4, "{row}");
    for field in ["graphDepth", "page", "snapshot", "candidatePage"] {
        assert!(rerun.get(field).is_none(), "{field}: {row}");
    }
    // Every lead the row hands out is valid input of its tool.
    public_row(&row);
}
