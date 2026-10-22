//! Published rows: symbols, diagnostics, failures, call edges, cursors and
//! caller pages.

use super::*;
use octocode_contract_types::ErrorCode;

#[test]
fn workspace_symbols_are_flat_named_and_one_based() {
    let symbol = serde_json::json!({
        "name": "run",
        "kind": 12,
        "containerName": "",
        "location": {
            "uri": "file:///repo/a.ts",
            "range": {"start": {"line": 9, "character": 2}, "end": {"line": 12, "character": 1}}
        }
    });
    assert_eq!(
        public_workspace_symbol(&symbol),
        serde_json::json!({
            "symbolName": "run",
            "kind": "function",
            "path": "/repo/a.ts",
            "displayRange": {"startLine": 10, "startCharacter": 3, "endLine": 13}
        })
    );
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
fn empty_diagnostics_carry_no_read_recovery_and_symbol_reads_are_matched() {
    let q = query(
        serde_json::json!({"operation": "diagnostic", "mainGoal": "test", "reasoning": "test", "path": "/repo/a.ts"}),
    );
    let row = with_next(
        &q,
        items_payload(&q, "diagnostics", serde_json::json!([]), TEST_BUDGET),
    );
    assert_eq!(row["category"], "noDiagnostics");
    assert!(row.get("next").is_none(), "{row}");
    // The row states the per-call fact; no static advice repeats on it.
    assert_eq!(
        row["reason"],
        "The language server reported no diagnostics for this document."
    );
    assert!(row.get("hints").is_none(), "{row}");

    let q = query(serde_json::json!({
        "operation": "definition", "mainGoal": "test", "reasoning": "test",
        "path": "/repo/a.ts",
        "symbolName": "run",
        "line": 3
    }));
    let row = failure(
        &q,
        "file:///repo/a.ts",
        ErrorCode::AnchorUnresolved,
        "x",
        true,
    );
    let read = &row["next"]["read"]["query"]["queries"][0];
    assert_eq!(read["path"], "/repo/a.ts");
    assert_eq!(read["matchString"], "run");
}

#[test]
fn path_policy_denials_are_file_access_failures_not_missing_servers() {
    let denied = LspFailure::path_denied(crate::policy::PolicyError::new(
        crate::policy::PolicyErrorCode::OutsideAllowedRoots,
        "Path '/etc/hosts' is outside allowed directories",
    ));
    assert_eq!(denied.code, ErrorCode::OutsideAllowedRoots);
    assert!(!denied.retryable);
    assert!(denied.hint.contains("ALLOWED_PATHS"));
    let missing = LspFailure::path_denied(crate::policy::PolicyError::new(
        crate::policy::PolicyErrorCode::NotFound,
        "missing",
    ));
    assert_eq!(missing.code, ErrorCode::PathNotFound);
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
    assert!(!invalid.hint.contains("Retry once"), "{}", invalid.hint);
    for fix in ["compile_commands.json", "compile_flags.txt", ".clangd"] {
        assert!(invalid.hint.contains(fix), "{fix}: {}", invalid.hint);
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
    let mut tips = engine
        .iter()
        .map(|error| LspFailure::from_engine(error).hint.to_owned())
        .collect::<Vec<_>>();
    tips.push(LspFailure::overdue("no answer".into()).hint.to_owned());
    for code in [
        ErrorCode::Timeout,
        ErrorCode::ServerUnavailable,
        ErrorCode::CapabilityUnavailable,
        ErrorCode::FileTooLarge,
        // Any other code: the generic tip.
        ErrorCode::RequestFailed,
        ErrorCode::AnchorUnresolved,
    ] {
        tips.extend(failure_hint(&anchored, code).map(str::to_owned));
    }
    for tip in tips {
        assert!(!tip.contains("workspaceRoot"), "{tip}");
    }
}

/// A timeout's tip leads somewhere runnable: the retry it names, and the
/// tools that locate and read the candidates without the server.
#[test]
fn a_timeout_tip_names_a_retry_and_the_tools_that_work_without_the_server() {
    let tip = LspFailure::from_engine(&octocode_engine::error::Error::timeout("slow")).hint;
    for tool in ["Retry", "astSearch", "localSearch", "localFetch"] {
        assert!(tip.contains(tool), "{tip}");
    }
}

/// A definition reached through a symlink and through its target is
/// one definition (canonical-path identity, like the hierarchy walk).
#[cfg(unix)]
#[test]
fn definition_identity_treats_a_symlink_and_its_target_as_one() {
    let (_dir, root, _) = temp_workspace();
    let target = root.join("impl.ts");
    std::fs::write(&target, "export function f() {}\n").expect("target");
    let link = root.join("link.ts");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");
    let snippet = |path: &std::path::Path| octocode_engine::lsp::types::LspCodeSnippet {
        uri: octocode_engine::lsp::uri::path_to_uri(&path.to_string_lossy()).expect("uri"),
        range: octocode_engine::lsp::types::LspRange {
            start: octocode_engine::lsp::types::LspExactPosition {
                line: 0,
                character: 16,
            },
            end: octocode_engine::lsp::types::LspExactPosition {
                line: 0,
                character: 17,
            },
        },
        content: String::new(),
        symbol_kind: None,
        display_range: None,
    };
    assert_eq!(
        snippet_identity(&snippet(&link)),
        snippet_identity(&snippet(&target))
    );
    // A missing path still has an identity (its decoded form).
    let gone = root.join("gone.ts");
    assert!(snippet_identity(&snippet(&gone)).contains("gone.ts"));
}

#[test]
fn anchor_file_uri_is_stated_once_per_row() {
    let canonical = "file:///repo/src/util.ts";
    let mut symbol = serde_json::json!({"name":"greet","uri":canonical,"foundAtLine":1});
    super::super::receipt::drop_same_uri(&mut symbol, canonical);
    assert!(symbol.get("uri").is_none(), "{symbol}");
    // A payload whose locations all sit in ANOTHER file keeps its uri.
    let mut other = serde_json::json!({"kind":"references","uri":"file:///repo/src/index.ts"});
    super::super::receipt::drop_same_uri(&mut other, canonical);
    assert_eq!(other["uri"], "file:///repo/src/index.ts");
    // Percent-encoding differences still name the same file.
    let mut encoded = serde_json::json!({"uri":"file:///repo/src/a%20b.ts"});
    super::super::receipt::drop_same_uri(&mut encoded, "file:///repo/src/a b.ts");
    assert!(encoded.get("uri").is_none(), "{encoded}");
}

#[test]
fn page_two_query_hashes_like_page_one() {
    let first: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "path":"/tmp/a.rs","symbolName":"is_alive","line":1,"operation":"references",
        "pageSize":1,"mainGoal": "test", "reasoning":"r"
    }))
    .expect("page 1");
    // A continuation lists the same fields in another order.
    let second: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "pageSize":1,"page":2,"snapshot":"lsp-v1:abc","operation":"references",
        "line":1,"mainGoal": "test", "reasoning":"r","symbolName":"is_alive","path":"/tmp/a.rs"
    }))
    .expect("page 2");
    let items = [serde_json::json!({"uri":"file:///tmp/a.rs"})];
    assert_eq!(
        semantic_snapshot(&first, "references", &items),
        semantic_snapshot(&second, "references", &items),
        "first={} second={}",
        lsp_search_query::to_row(&first),
        lsp_search_query::to_row(&second)
    );
}

#[test]
fn unresolved_symbol_anchor_reads_the_hinted_lines_and_suggests_near_names() {
    let q = query(serde_json::json!({
        "operation": "references", "mainGoal": "test", "reasoning": "test",
        "path": "/repo/lib.rs",
        "symbolName": "is_invalid_input_codeX",
        "line": 12
    }));
    let source = (1..=20)
        .map(|line| match line {
            12 => "pub fn is_invalid_input_code(code: &str) -> bool {".to_owned(),
            13 => "    is_input(code)".to_owned(),
            _ => format!("// line {line}"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut row = failure(
        &q,
        "file:///repo/lib.rs",
        ErrorCode::AnchorUnresolved,
        "x",
        true,
    );
    anchor_recovery(&mut row, &q, Some(&source));
    // A resolved candidate is the recovery: no read beside it, and no
    // static prose.
    assert!(row["next"].get("read").is_none(), "{row}");
    // The lead names the one near name; no sentence repeats it.
    assert!(row["hints"].as_array().is_some_and(Vec::is_empty), "{row}");
    // Without a candidate, the lines around the hint are read.
    let mut blank = failure(
        &q,
        "file:///repo/lib.rs",
        ErrorCode::AnchorUnresolved,
        "x",
        true,
    );
    let unrelated = (1..=20)
        .map(|line| format!("// line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    anchor_recovery(&mut blank, &q, Some(&unrelated));
    // A `line` past the end reads the file's last lines.
    let past = requery(&q, serde_json::json!({"line": 99_999_999}));
    let mut beyond = failure(
        &past,
        "file:///repo/lib.rs",
        ErrorCode::AnchorUnresolved,
        "x",
        true,
    );
    anchor_recovery(&mut beyond, &past, Some(&unrelated));
    assert_eq!(
        beyond["next"]["read"]["query"]["queries"][0]["ranges"],
        serde_json::json!(["15-20"]),
        "{beyond}"
    );
    let read = &blank["next"]["read"];
    assert_eq!(
        read["query"]["queries"][0]["path"], "/repo/lib.rs",
        "{blank}"
    );
    assert_eq!(
        read["query"]["queries"][0]["ranges"],
        serde_json::json!(["7-17"]),
        "{blank}"
    );
    assert!(
        read["query"]["queries"][0].get("startLine").is_none(),
        "{blank}"
    );
    assert!(
        read["query"]["queries"][0].get("matchString").is_none(),
        "{blank}"
    );
    assert!(blank["next"].get("findSimilar").is_none(), "{blank}");
    assert!(
        blank
            .get("hints")
            .is_none_or(|hints| hints == &serde_json::json!([])),
        "{blank}"
    );
    let retry = &row["next"]["findSimilar"];
    assert_eq!(retry["tool"], "lspSearch", "{row}");
    assert_eq!(
        retry["query"]["queries"][0]["symbolName"], "is_invalid_input_code",
        "{row}"
    );
    assert_eq!(retry["query"]["queries"][0]["line"], 12, "{row}");
}

#[test]
fn call_sites_that_differ_only_in_end_column_publish_once() {
    let edge = HierarchyEdge {
        node: node("callee"),
        parent: None,
        level: 1,
        sites: vec![
            serde_json::json!({"start": {"line": 870, "character": 10}, "end": {"line": 870, "character": 22}}),
            serde_json::json!({"start": {"line": 870, "character": 10}, "end": {"line": 870, "character": 17}}),
            serde_json::json!({"start": {"line": 871, "character": 2}, "end": {"line": 871, "character": 5}}),
        ],
    };
    let public = public_edge(Expansion::OutgoingCalls, &edge, None);
    assert_eq!(
        public["fromRanges"],
        serde_json::json!([
            {"startLine": 871, "startCharacter": 11, "endLine": 871},
            {"startLine": 872, "startCharacter": 3, "endLine": 872}
        ])
    );
}

#[test]
fn typescript_builtin_lib_declarations_are_recognized_only_under_typescript_lib() {
    let at = |uri: &str| serde_json::json!({"uri": uri});
    assert!(is_builtin_lib_declaration(&at(
        "file:///repo/node_modules/typescript/lib/lib.es5.d.ts"
    )));
    assert!(is_builtin_lib_declaration(&at(
        "file:///repo/node_modules/typescript/lib/lib.dom.d.ts"
    )));
    assert!(!is_builtin_lib_declaration(&at(
        "file:///repo/node_modules/@types/node/lib.d.ts"
    )));
    assert!(!is_builtin_lib_declaration(&at(
        "file:///repo/src/lib.utils.d.ts"
    )));
    assert!(!is_builtin_lib_declaration(&at(
        "file:///repo/node_modules/typescript/lib/typescript.d.ts"
    )));
    assert!(!is_builtin_lib_declaration(&serde_json::json!({})));
}

#[tokio::test(flavor = "current_thread")]
async fn builtin_lib_callees_are_listed_not_counted() {
    let mut graph = graph(&[("root", &["toUpperCase", "map", "map"])]);
    graph.uri = Some("file:///repo/node_modules/typescript/lib/lib.es5.d.ts".into());
    let walk = walk(&graph, 1).await;
    assert!(walk.edges.is_empty());
    assert_eq!(
        walk.builtin_lib,
        vec!["toUpperCase".to_owned(), "map".to_owned()]
    );
    assert_eq!(walk.out_of_policy, 0, "built-ins are not policy failures");
    let q = query(serde_json::json!({
        "operation": "callees", "mainGoal": "test", "reasoning": "test",
        "path": "file:///repo/a.ts",
        "symbolName": "foo", "line": 1
    }));
    let mut row = serde_json::json!({"status": "hasResults", "kind": "callees", "items": []});
    mark_truncation(&mut row, &q, &[(Expansion::OutgoingCalls, &walk)]);
    assert!(row.get("warnings").is_none(), "{row}");
    assert_eq!(row["builtinLib"], serde_json::json!(["toUpperCase", "map"]));
    // A container (`detail`) qualifies the name.
    let mut node = node_at("toUpperCase", Some("file:///x/typescript/lib/lib.es5.d.ts"));
    node["detail"] = serde_json::json!("String");
    assert_eq!(builtin_name(&node), "String.toUpperCase");
}

#[test]
fn diagnostic_severity_is_published_by_name() {
    let (items, _) = diagnostic_items(Some(serde_json::json!([
        {"severity": 1, "message": "e", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}}},
        {"severity": 2, "message": "w"},
        {"severity": 3, "message": "i"},
        {"severity": 4, "message": "h"},
        {"severity": 9, "message": "unknown"},
        {"message": "none"}
    ])));
    let severities = items
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item.get("severity").cloned())
        .collect::<Vec<_>>();
    assert_eq!(
        severities,
        vec![
            Some(serde_json::json!("error")),
            Some(serde_json::json!("warning")),
            Some(serde_json::json!("information")),
            Some(serde_json::json!("hint")),
            Some(serde_json::json!(9)),
            None,
        ]
    );
    assert_eq!(
        items[0]["displayRange"],
        serde_json::json!({"startLine": 1, "startCharacter": 1, "endLine": 1})
    );
}

#[test]
fn long_declaration_content_is_capped_with_a_marker_naming_the_omitted_lines() {
    let body = (0..200)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut location = serde_json::json!({
        "uri": "file:///repo/a.ts",
        "content": body,
        "displayRange": {"startLine": 426, "endLine": 625}
    });
    let rest = cap_declaration_content(&mut location).expect("a read of the omitted lines");
    let content = location["content"].as_str().expect("content");
    let lines = content.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), MAX_DECLARATION_CONTENT_LINES + 1);
    assert_eq!(lines[0], "line 0");
    assert_eq!(lines[MAX_DECLARATION_CONTENT_LINES - 1], "line 59");
    assert_eq!(
        lines[MAX_DECLARATION_CONTENT_LINES],
        "… 140 more lines omitted (source lines 486-625)."
    );
    // The omitted lines are one executable read, each line once.
    assert_eq!(rest["tool"], "localFetch");
    assert_eq!(
        rest["query"]["queries"][0],
        serde_json::json!({"path": "/repo/a.ts", "ranges": ["486-625"]})
    );
    let short = "a\nb\nc";
    let mut small =
        serde_json::json!({"content": short, "displayRange": {"startLine": 1, "endLine": 3}});
    assert!(cap_declaration_content(&mut small).is_none());
    assert_eq!(small["content"], short, "short bodies are untouched");
}

#[test]
fn hover_range_is_published_as_a_one_based_display_range() {
    let hover = public_hover(serde_json::json!({
        "contents": {"kind": "markdown", "value": "```ts\nconst a: number\n```"},
        "range": {"start": {"line": 3, "character": 8}, "end": {"line": 4, "character": 2}}
    }));
    assert!(hover.get("range").is_none(), "{hover}");
    assert_eq!(
        hover["displayRange"],
        serde_json::json!({"startLine": 4, "startCharacter": 9, "endLine": 5}),
        "{hover}"
    );
    assert_eq!(hover["contents"]["kind"], "markdown", "{hover}");
    let bare = public_hover(serde_json::json!({"contents": "text"}));
    assert_eq!(bare, serde_json::json!({"contents": "text"}));
    crate::contracts::validate_output(
        "lspSearch",
        &public_row(&serde_json::json!({
            "lsp": {"serverAvailable": true, "provider": "hoverProvider"},
            "kind": "hover", "hover": hover
        })),
    )
    .expect("published hover satisfies the internal output contract");
}

#[test]
fn call_edges_compact_to_per_file_call_rows() {
    let caller = |path: &str,
                  name: &str,
                  kind: &str,
                  start: u64,
                  end: u64,
                  sites: &[(u64, u64)]| {
        serde_json::json!({
            "from": {"symbolName": name, "kind": kind, "uri": format!("file://{path}"),
                "displayRange": {"startLine": start, "startCharacter": 7, "endLine": end}},
            "fromRanges": sites.iter().map(|(line, column)| serde_json::json!({"startLine": line, "startCharacter": column, "endLine": line})).collect::<Vec<_>>(),
            "level": 1
        })
    };
    let mut method = caller(
        "/repo/App.tsx",
        "renderEmbeddables",
        "method",
        1833,
        2120,
        &[(1981, 32)],
    );
    method["from"]["detail"] = serde_json::json!("App");
    let mut recovered = caller("/repo/lib.ts", "load", "function", 718, 718, &[(729, 24)]);
    recovered["source"] = serde_json::json!("recoveredFromReferences");
    // A detail that only restates the name (`fn other()`) is dropped; a
    // signature is kept only to tell apart listed nodes sharing a name; a
    // container name (`App`) is kept.
    let mut render = caller(
        "/repo/svg.ts",
        "render",
        "function",
        97,
        848,
        &[(359, 38), (402, 5)],
    );
    render["from"]["detail"] = serde_json::json!("pub fn render(\n    ctx: &Ctx,\n) -> Svg");
    let mut other = caller("/repo/svg.ts", "other", "function", 900, 950, &[(910, 3)]);
    other["from"]["detail"] = serde_json::json!("pub(crate) async fn other()");
    let mut row = serde_json::json!({"kind": "callers", "matches": [
        render,
        method,
        recovered,
        other,
    ]});
    compact_calls(&mut row, &[], &repo_policy());
    assert!(row.get("matches").is_none(), "{row}");
    assert_eq!(
        row["files"],
        serde_json::json!([
            {"path": "/repo/svg.ts", "matches": [
                {"symbolName": "render", "kind": "function", "line": 97, "endLine": 848,
                    "sites": [{"line": 359, "column": 38}, {"line": 402, "column": 5}]},
                {"symbolName": "other", "kind": "function", "line": 900, "endLine": 950,
                    "sites": [{"line": 910, "column": 3}]}
            ]},
            {"path": "/repo/App.tsx", "matches": [
                {"symbolName": "renderEmbeddables", "kind": "method", "line": 1833, "endLine": 2120,
                    "sites": [{"line": 1981, "column": 32}], "detail": "App"}
            ]},
            {"path": "/repo/lib.ts", "matches": [
                {"symbolName": "load", "kind": "function", "line": 718,
                    "sites": [{"line": 729, "column": 24}], "source": "recoveredFromReferences"}
            ]}
        ]),
        "{row}"
    );
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("compact callers satisfy the output contract");
    // A module caller is top-level code named by its file; that name
    // anchors no lspSearch call, so its row has no symbolName.
    let mut module = serde_json::json!({"kind": "callers", "matches": [
        caller("/repo/public.test.ts", "public.test.ts", "module", 1, 40, &[(12, 3)]),
    ]});
    compact_calls(&mut module, &[], &repo_policy());
    let module_row = &module["files"][0]["matches"][0];
    assert_eq!(module_row["kind"], "module", "{module}");
    assert!(module_row.get("symbolName").is_none(), "{module}");
    assert_eq!(module_row["sites"][0]["line"], 12, "{module}");
    // Deeper and outgoing edges use the same rows: `to` names a callee (its
    // call sites are in the caller), and `via <name>@<line>` names the parent
    // node an edge below level 1 connects to.
    let via = |name: &str, path: &str, line: u64| serde_json::json!({"symbolName": name, "uri": format!("file://{path}"), "line": line, "character": 7});
    let mut level2 = caller("/repo/b.ts", "b", "function", 5, 9, &[(6, 3)]);
    level2["level"] = serde_json::json!(2);
    level2["via"] = via("a", "/repo/a.ts", 1);
    let mut callee = caller("/repo/c.ts", "c", "function", 10, 12, &[(2, 5)]);
    callee["to"] = callee["from"].take();
    callee.as_object_mut().expect("item").remove("from");
    // Two listed nodes share a name and line: the via names its file,
    // relative to the workspace like every row path.
    let mut ambiguous = caller("/repo/sub/z.ts", "z", "function", 40, 44, &[(41, 2)]);
    ambiguous["level"] = serde_json::json!(2);
    ambiguous["via"] = via("walk", "/repo/y.ts", 281);
    let items = serde_json::json!([
        caller("/repo/a.ts", "a", "function", 1, 3, &[(2, 1)]),
        caller("/repo/x.ts", "walk", "function", 281, 290, &[(285, 1)]),
        caller("/repo/y.ts", "walk", "function", 281, 299, &[(290, 1)]),
        level2,
        ambiguous,
        callee
    ]);
    let mut deep = serde_json::json!({"kind": "callers", "matches": items.clone()});
    compact_calls(&mut deep, items.as_array().expect("items"), &repo_policy());
    assert_eq!(
        deep["files"],
        serde_json::json!([
            {"path": "/repo/a.ts", "matches": [{"symbolName": "a", "kind": "function", "line": 1,
                "endLine": 3, "sites": [{"line": 2, "column": 1}]}]},
            {"path": "/repo/x.ts", "matches": [{"symbolName": "walk", "kind": "function", "line": 281,
                "endLine": 290, "sites": [{"line": 285, "column": 1}]}]},
            {"path": "/repo/y.ts", "matches": [{"symbolName": "walk", "kind": "function", "line": 281,
                "endLine": 299, "sites": [{"line": 290, "column": 1}]}]},
            {"path": "/repo/b.ts", "matches": [{"symbolName": "b", "kind": "function", "line": 5,
                "endLine": 9, "sites": [{"line": 6, "column": 3}], "via": {"symbolName": "a", "line": 1}}]},
            {"path": "/repo/sub/z.ts", "matches": [{"symbolName": "z", "kind": "function", "line": 40,
                "endLine": 44, "sites": [{"line": 41, "column": 2}],
                "via": {"symbolName": "walk", "line": 281, "path": "y.ts"}}]},
            {"path": "/repo/c.ts", "matches": [{"symbolName": "c", "kind": "function", "line": 10,
                "endLine": 12, "sites": [{"line": 2, "column": 5}]}]}
        ]),
        "{deep}"
    );
    crate::contracts::validate_output("lspSearch", &public_row(&deep))
        .expect("compact hierarchy rows satisfy the output contract");
    // Type-hierarchy items are nodes, not calls: they stay items.
    let mut types = serde_json::json!({"kind": "supertypes", "matches": [
        {"name": "Base", "kind": "class", "uri": "file:///repo/a.ts", "level": 1}
    ]});
    let before = types.clone();
    compact_calls(&mut types, &[], &repo_policy());
    assert_eq!(types, before);
}

/// hover reads one position: walk controls are rejected before any server
/// starts, with the operations they apply to.
#[test]
fn hover_rejects_walk_controls_with_the_operations_that_take_them() {
    for (field, value, applies) in [
        ("graphDepth", serde_json::json!(2), "callers"),
        ("groupByFile", serde_json::json!(true), "references"),
    ] {
        let mut row = serde_json::json!({"operation":"hover","path":"/tmp/a.rs","symbolName":"main","line":1});
        row[field] = value;
        let error = crate::contracts::validate("lspSearch", serde_json::json!({"queries":[row]}))
            .expect_err("hover with a walk control");
        let issue = error
            .issues
            .iter()
            .find(|issue| {
                issue.rule_id == "schema.unknown-field"
                    && issue.path.last().is_some_and(|part| part == field)
            })
            .expect("unsupported operation field");
        let guidance = issue.schema.as_ref().expect("sibling operation guidance");
        assert_eq!(guidance["siblingConflict"], true);
        assert!(
            guidance["siblingRequires"]
                .as_array()
                .expect("allowed operations")
                .iter()
                .any(|requirement| requirement
                    .as_str()
                    .is_some_and(|text| text.contains(applies))),
            "{guidance}"
        );
    }
}

/// A capped declaration body numbers its source lines; the trailing
/// omission marker is not a source line and stays unnumbered.
#[test]
fn capped_declaration_content_numbers_lines_but_not_the_marker() {
    let body = (1..=70).map(|n| format!("line {n}\n")).collect::<String>();
    let mut location = serde_json::json!({
        "uri": "file:///repo/src/lib.rs",
        "range": {"start": {"line": 9, "character": 0}, "end": {"line": 78, "character": 1}},
        "content": body,
        "displayRange": {"startLine": 10, "endLine": 79}
    });
    super::super::locations::cap_declaration_content(&mut location).expect("read of the rest");
    let public = public_location(location);
    let content = public["content"].as_str().expect("content");
    assert!(content.starts_with("10\tline 1\n"), "{content}");
    assert!(content.contains("\n69\tline 60\n… "), "{content}");
    assert!(
        content
            .lines()
            .last()
            .is_some_and(|marker| marker.starts_with("… ")),
        "{content}"
    );
}

/// The anchor file and workspace root are named by their canonical paths
/// before any snapshot is computed, so a continuation that spells them
/// relative to the workspace (as the response envelope does) replays the
/// same snapshot as the absolute first page.
#[test]
fn relative_and_absolute_anchors_share_one_query_identity() {
    let root = tempfile::tempdir().expect("fixture");
    let root = std::fs::canonicalize(root.path()).expect("canonical");
    std::fs::create_dir_all(root.join("src")).expect("dir");
    std::fs::write(root.join("src/a.ts"), "export const a = 1;\n").expect("file");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let base = |uri: String, workspace: &str| serde_json::json!({"operation":"documentSymbols","mainGoal":"t","reasoning":"t","path":uri,"workspaceRoot":workspace,"page":2,"pageSize":1});
    let mut absolute = query(base(
        root.join("src/a.ts").to_string_lossy().into_owned(),
        &root.to_string_lossy(),
    ));
    let mut relative = query(base("src/a.ts".into(), "."));
    let mut uri = query(base(
        format!("file://{}", root.join("src/a.ts").display()),
        ".",
    ));
    let canonical = root.join("src/a.ts").to_string_lossy().into_owned();
    for query in [&mut absolute, &mut relative, &mut uri] {
        assert_eq!(resolve_paths(query, &paths).expect("valid"), canonical);
    }
    assert_eq!(
        lsp_search_query::to_row(&absolute),
        lsp_search_query::to_row(&relative)
    );
    assert_eq!(
        lsp_search_query::to_row(&absolute),
        lsp_search_query::to_row(&uri)
    );
    let items = [serde_json::json!({"name":"a"})];
    let snapshot = |query: &LspSearchQuery| semantic_snapshot(query, "documentSymbols", &items);
    assert_eq!(snapshot(&absolute), snapshot(&relative));
}

/// The anchor receipt shows only what the request did not say: the line a
/// symbol moved to. A column alone is no receipt.
#[test]
fn resolved_symbol_receipt_shows_only_a_moved_anchor() {
    let uri = "file:///repo/a.ts";
    let named = query(serde_json::json!({
        "operation": "references", "path": uri, "symbolName": "foo", "line": 3
    }));
    let column = serde_json::json!({"uri": uri, "foundAtCharacter": 5, "orderHint": 1});
    let moved = serde_json::json!({"uri": uri, "foundAtCharacter": 5, "foundAtLine": 4, "lineDeviation": 1});
    assert_eq!(public_resolved_symbol(&named, Some(column), uri), None);
    assert_eq!(
        public_resolved_symbol(&named, Some(moved), uri),
        Some(serde_json::json!({"foundAtCharacter": 5, "foundAtLine": 4, "lineDeviation": 1}))
    );
}

/// An outgoing call site lies in the caller's file, so a callee row is filed
/// under the caller and names the callee's file beside its declaration range
/// when it differs. Incoming and outgoing rows never share a file entry.
#[test]
fn outgoing_call_sites_are_filed_under_the_caller() {
    let root = node_at("main", Some("file:///repo/index.ts"));
    let mut callee = node_at("helper", Some("file:///repo/types.d.ts"));
    callee["kind"] = serde_json::json!(12);
    let outgoing = HierarchyEdge {
        node: callee,
        parent: Some(root.clone()),
        level: 1,
        sites: vec![
            serde_json::json!({"start": {"line": 380, "character": 2}, "end": {"line": 380, "character": 8}}),
        ],
    };
    let incoming = HierarchyEdge {
        node: node_at("boot", Some("file:///repo/index.ts")),
        parent: Some(root),
        level: 1,
        sites: vec![
            serde_json::json!({"start": {"line": 3, "character": 4}, "end": {"line": 3, "character": 8}}),
        ],
    };
    let items = vec![
        public_edge(Expansion::IncomingCalls, &incoming, None),
        public_edge(Expansion::OutgoingCalls, &outgoing, None),
    ];
    assert!(
        items.iter().all(|item| item.get("via").is_none()),
        "level-1 edges name no parent: {items:?}"
    );
    let mut row = serde_json::json!({"kind": "callers", "matches": items});
    compact_calls(&mut row, &[], &repo_policy());
    let helper_line = name_line("helper") + 1;
    let boot_line = name_line("boot") + 1;
    assert_eq!(
        row["files"],
        serde_json::json!([
            {"path": "/repo/index.ts", "matches": [{"symbolName": "boot", "kind": "function",
                "line": boot_line, "endLine": boot_line + 3, "sites": [{"line": 4, "column": 5}]}]},
            {"path": "/repo/index.ts", "matches": [{"symbolName": "helper", "kind": "function",
                "line": helper_line, "endLine": helper_line + 3, "path": "types.d.ts",
                "sites": [{"line": 381, "column": 3}]}]}
        ]),
        "{row}"
    );
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("caller-filed callee rows satisfy the output contract");
}

/// A lead probes the configured server file only when the path policy
/// authorizes reading it.
#[test]
fn lead_discovery_drops_an_unauthorized_server_file() {
    let paths = PathPolicy::new(crate::policy::path::PathPolicyConfig::default()).expect("policy");
    let execution = LspExecutionConfig {
        env: std::collections::BTreeMap::from([("OCTOCODE_LEAD_PROBE".to_owned(), "x".to_owned())]),
        config_path: Some("/nonexistent/octocode-lsp.json".to_owned()),
        ..Default::default()
    };
    let discovery = execution.lead_discovery(&paths);
    assert_eq!(discovery.config_path, None);
    assert_eq!(
        discovery.env.get("OCTOCODE_LEAD_PROBE").map(String::as_str),
        Some("x")
    );
}

/// A callers or references success offers one read of the listed sites in
/// context: one localFetch row per file (first files first), merged ±6-line
/// windows, at most the batch limit of rows.
#[test]
fn site_rows_offer_one_read_of_the_sites_in_context() {
    let mut row = serde_json::json!({"kind": "callers", "files": [
        {"path": "/repo/a.ts", "matches": [{"symbolName": "render", "kind": "function", "line": 97,
            "endLine": 848, "sites": [{"line": 359, "column": 38}, {"line": 362, "column": 5}]}]},
        {"path": "/repo/b.ts", "matches": [
            {"symbolName": "load", "kind": "function", "line": 1, "endLine": 9, "sites": [{"line": 4, "column": 2}]},
            {"symbolName": "go", "kind": "function", "line": 30, "endLine": 50, "sites": [{"line": 40, "column": 1}]}
        ]}
    ]});
    attach_read_lead(&mut row);
    let read = &row["next"]["read"];
    assert_eq!(read["tool"], "localFetch", "{row}");
    assert_eq!(
        read["query"]["queries"],
        serde_json::json!([
            {"path": "/repo/a.ts", "ranges": ["353-368"]},
            {"path": "/repo/b.ts", "ranges": ["1-10", "34-46"]}
        ]),
        "{row}"
    );
    let files = (0..7)
        .map(|n| serde_json::json!({"path": format!("/repo/{n}.ts"), "matches": [{"line": n + 10, "column": 1, "value": "x"}]}))
        .collect::<Vec<_>>();
    let mut wide = serde_json::json!({"kind": "references", "files": files});
    attach_read_lead(&mut wide);
    assert_eq!(
        wide["next"]["read"]["query"]["queries"]
            .as_array()
            .map(Vec::len),
        Some(5)
    );
    let mut empty = serde_json::json!({"status": "empty", "kind": "empty"});
    attach_read_lead(&mut empty);
    assert!(empty.get("next").is_none());
}

/// Two listed callers share a name: their signatures tell them apart.
#[test]
fn signatures_are_kept_only_for_callers_that_share_a_name() {
    let caller = |path: &str, detail: &str, start: u64, site: u64| {
        serde_json::json!({
            "from": {"symbolName": "render", "kind": "function", "detail": detail, "uri": format!("file://{path}"),
                "displayRange": {"startLine": start, "startCharacter": 7, "endLine": start + 5}},
            "fromRanges": [{"startLine": site, "startCharacter": 3, "endLine": site}],
            "level": 1
        })
    };
    let mut row = serde_json::json!({"kind": "callers", "matches": [
        caller("/repo/a.rs", "fn render(ctx: &Ctx) -> Svg", 10, 12),
        caller("/repo/b.rs", "fn render(page: &Page)", 20, 22)
    ]});
    compact_calls(&mut row, &[], &repo_policy());
    assert_eq!(
        row["files"],
        serde_json::json!([
            {"path": "/repo/a.rs", "matches": [{"symbolName": "render", "kind": "function", "line": 10,
                "endLine": 15, "sites": [{"line": 12, "column": 3}], "detail": "fn render(ctx: &Ctx) -> Svg"}]},
            {"path": "/repo/b.rs", "matches": [{"symbolName": "render", "kind": "function", "line": 20,
                "endLine": 25, "sites": [{"line": 22, "column": 3}], "detail": "fn render(page: &Page)"}]}
        ]),
        "{row}"
    );
}

/// Only a file that spells a rename of the symbol (`symbol as x`, or a
/// destructured `symbol: x`) is parsed for aliasing imports.
#[test]
fn only_a_spelled_rename_is_parsed_for_aliases() {
    for renamed in [
        "import { foo as bar } from './a';",
        "import {\n  foo\n    as bar,\n} from './a';",
        "const { foo: bar } = require('./a');",
        "use crate::a::foo as bar;",
        "from a import (foo as bar)",
    ] {
        assert!(
            super::super::recovery::may_rename(renamed, "foo"),
            "{renamed}"
        );
    }
    for plain in [
        "import { foo } from './a';",
        "foo(1); foobar as x; x.foo::y;",
        "const food = { foo };",
    ] {
        assert!(!super::super::recovery::may_rename(plain, "foo"), "{plain}");
    }
}

/// tsserver reports a call in a constructor with the whole class as caller;
/// the row shows the constructor that holds the call site instead. The
/// class file is read once (policy-checked, bounded) for every class caller
/// it holds.
#[tokio::test(flavor = "current_thread")]
async fn class_callers_narrow_to_the_member_holding_the_call_site() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("canonical root");
    let file = root.join("editor.ts");
    std::fs::write(
        &file,
        "export class Editor {\n  value = 1;\n  constructor() {\n    mutate(this);\n  }\n  other() {}\n}\n",
    )
    .expect("source");
    let uri = format!("file://{}", file.display());
    let class = serde_json::json!({
        "name": "Editor", "kind": 5, "uri": uri,
        "range": {"start": {"line": 0, "character": 0}, "end": {"line": 6, "character": 1}},
        "selectionRange": {"start": {"line": 0, "character": 13}, "end": {"line": 0, "character": 19}}
    });
    let edge = |site: serde_json::Value| super::super::walk::HierarchyEdge {
        node: class.clone(),
        parent: None,
        level: 1,
        sites: vec![site],
    };
    let policy = crate::tools::test_support::workspace_policy(&root);
    let mut sources = super::super::source::SourceCache::new(&policy);
    let mut facts = super::super::walk::ClassFacts::default();
    let site = serde_json::json!({"start": {"line": 3, "character": 4}, "end": {"line": 3, "character": 10}});
    let narrowed = facts
        .narrow(&edge(site), &mut sources)
        .await
        .expect("constructor holds the call");
    // The whole narrowed item: the constructor's name, ranges and the
    // class as its detail.
    assert_eq!(
        narrowed,
        serde_json::json!({
            "name": "constructor", "kind": 9, "detail": "Editor", "uri": uri,
            "range": {"start": {"line": 2, "character": 2}, "end": {"line": 4, "character": 3}},
            "selectionRange": {"start": {"line": 2, "character": 2}, "end": {"line": 2, "character": 13}}
        }),
        "{narrowed}"
    );
    // A site outside every member keeps the class.
    let field = serde_json::json!({"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}});
    assert!(facts.narrow(&edge(field), &mut sources).await.is_none());
    assert_eq!(sources.reads(), 1, "one read for both class callers");
    // A class file outside the read policy is never read.
    let outside = tempfile::tempdir().expect("outside");
    let foreign = outside
        .path()
        .canonicalize()
        .expect("canonical")
        .join("x.ts");
    std::fs::write(
        &foreign,
        "export class X {\n  constructor() {\n    f();\n  }\n}\n",
    )
    .expect("foreign");
    let mut foreign_class = class.clone();
    foreign_class["uri"] = serde_json::json!(format!("file://{}", foreign.display()));
    let foreign_edge = super::super::walk::HierarchyEdge {
        node: foreign_class,
        parent: None,
        level: 1,
        sites: vec![
            serde_json::json!({"start": {"line": 2, "character": 4}, "end": {"line": 2, "character": 5}}),
        ],
    };
    assert!(facts.narrow(&foreign_edge, &mut sources).await.is_none());
    assert_eq!(sources.reads(), 1, "the refused file was not read");
}

/// The workspace policy of the `/repo` fixtures.
fn repo_policy() -> crate::policy::path::PathPolicy {
    crate::tools::test_support::workspace_policy(std::path::Path::new("/repo"))
}

#[test]
fn declared_in_paths_are_workspace_relative() {
    let root = node_at("main", Some("file:///repo/src/deep/a.ts"));
    let mut callee = node_at("map", Some("file:///repo/node_modules/x/index.d.ts"));
    callee["kind"] = serde_json::json!(12);
    let mut outside = node_at("ext", Some("file:///elsewhere/lib.d.ts"));
    outside["kind"] = serde_json::json!(12);
    let edge = |node: serde_json::Value, line: u64| HierarchyEdge {
        node,
        parent: Some(root.clone()),
        level: 1,
        sites: vec![
            serde_json::json!({"start": {"line": line, "character": 2}, "end": {"line": line, "character": 8}}),
        ],
    };
    let items = vec![
        public_edge(Expansion::OutgoingCalls, &edge(callee, 10), None),
        public_edge(Expansion::OutgoingCalls, &edge(outside, 11), None),
    ];
    let mut row = serde_json::json!({"kind": "callees", "matches": items});
    compact_calls(&mut row, &[], &repo_policy());
    let calls = row["files"][0]["matches"].as_array().expect("calls");
    let map_line = name_line("map") + 1;
    let ext_line = name_line("ext") + 1;
    assert_eq!(
        calls,
        &vec![
            serde_json::json!({"symbolName": "map", "kind": "function", "line": map_line,
                "endLine": map_line + 3, "path": "node_modules/x/index.d.ts",
                "sites": [{"line": 11, "column": 3}]}),
            serde_json::json!({"symbolName": "ext", "kind": "function", "line": ext_line,
                "endLine": ext_line + 3, "path": "/elsewhere/lib.d.ts",
                "sites": [{"line": 12, "column": 3}]}),
        ],
        "{row}"
    );
}

#[test]
fn file_level_partial_clears_when_text_files_match() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    std::fs::write(root.join("a.ts"), "export function foo() {}\n").expect("a");
    std::fs::write(root.join("b.ts"), "import { foo } from './a';\nfoo();\n").expect("b");
    let scope =
        super::super::scope::Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".into()]);
    let paths = crate::tools::test_support::workspace_policy(&root);
    tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(scope.text_files("foo", &paths, &crate::execution::cancel::NeverCancel))
        .expect("scan");
    let q = query(serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "foo", "line": 1
    }));
    let row = || serde_json::json!({"kind": "references"});
    // One file only the text scan sees: flagged, and the lead lists it.
    scope.answer([root.join("a.ts").to_string_lossy()]);
    let mut partial = row();
    flag_partial(
        &mut partial,
        &q,
        "importerScanCapped",
        Some("capped"),
        &scope,
    );
    assert_eq!(partial["isPartial"], true, "{partial}");
    // The importer cap is a window, not a terminal limit:
    // `next.nextImporterPage` verifies the unchecked candidates.
    assert!(partial.get("terminalLimit").is_none(), "{partial}");
    assert_eq!(partial["coverage"]["textOnlyFiles"], 1, "{partial}");
    let lead = &partial["next"]["findText"]["query"]["queries"];
    assert_eq!(lead.as_array().map(Vec::len), Some(1), "{partial}");
    assert_eq!(
        lead[0]["path"],
        root.join("b.ts").to_string_lossy().as_ref()
    );
    // Every file that spells the name is in the answer: the file-level
    // reason is moot.
    scope.answer([root.join("b.ts").to_string_lossy()]);
    let mut agreed = row();
    flag_partial(
        &mut agreed,
        &q,
        "importerScanCapped",
        Some("capped"),
        &scope,
    );
    assert!(agreed.get("isPartial").is_none(), "{agreed}");
    assert!(agreed.get("next").is_none(), "{agreed}");
    assert_eq!(agreed["coverage"]["textOnlyFiles"], 0, "{agreed}");
    assert!(agreed["coverage"].get("reason").is_none(), "{agreed}");
}

#[test]
fn site_level_partial_survives_matching_counts() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    std::fs::write(root.join("a.py"), "def get():\n    pass\nget()\n").expect("a");
    let scope =
        super::super::scope::Scope::new(root.to_string_lossy().into_owned(), vec!["*.py".into()]);
    let paths = crate::tools::test_support::workspace_policy(&root);
    tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(scope.text_files("get", &paths, &crate::execution::cancel::NeverCancel))
        .expect("scan");
    scope.answer([root.join("a.py").to_string_lossy()]);
    let q = query(serde_json::json!({
        "operation": "references", "path": root.join("a.py").to_string_lossy(),
        "symbolName": "get", "line": 1
    }));
    let mut row = serde_json::json!({"kind": "references"});
    flag_partial(&mut row, &q, "cfgGatedFiles", Some("gated"), &scope);
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(row["coverage"]["textOnlyFiles"], 0, "{row}");
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["cfgGatedFiles"]),
        "{row}"
    );
    // Dynamic dispatch is judged by files: every file that spells the name
    // is answered, so the Python row is complete.
    let mut python = serde_json::json!({"kind": "references"});
    flag_partial(&mut python, &q, "dynamicDispatch", Some("dynamic"), &scope);
    assert!(python.get("isPartial").is_none(), "{python}");
    assert!(row["coverage"].get("reason").is_none(), "{row}");
    // No file to list: the lead searches the whole scope.
    assert_eq!(
        row["next"]["findText"]["query"]["queries"][0]["path"],
        root.to_string_lossy().as_ref()
    );
}

#[test]
fn first_pages_of_incoming_walks_reuse_responses_keyed_by_scope_fingerprint() {
    let callers = query(serde_json::json!({
        "operation": "callers", "path": "/repo/a.ts", "symbolName": "f", "line": 1
    }));
    let reused = response_scope(&callers, "content".into(), Some("fp"));
    assert!(reused.reuse);
    assert_ne!(reused.generation, "content");
    // Another fingerprint (any file edit) is another generation.
    assert_ne!(
        response_scope(&callers, "content".into(), Some("fp2")).generation,
        reused.generation
    );
    // No fingerprint (walk past its bounds): first pages ask the server.
    let unbounded = response_scope(&callers, "content".into(), None);
    assert!(!unbounded.reuse);
    assert_eq!(unbounded.generation, "content");
    // Hover always asks; its continuation keying is unchanged.
    let hover = query(serde_json::json!({
        "operation": "hover", "path": "/repo/a.ts", "symbolName": "f", "line": 1
    }));
    assert!(!response_scope(&hover, "content".into(), Some("fp")).reuse);
    // A continuation page still reuses.
    let page_two = requery(
        &callers,
        serde_json::json!({"page": 2, "snapshot": "lsp-v1:x"}),
    );
    assert!(response_scope(&page_two, "content".into(), None).reuse);
    // So does a later importer window: its anchor answer is the one its
    // candidate digest was cut from.
    let window_two = requery(
        &callers,
        serde_json::json!({"importerPage": 2, "snapshot": "lsp-imp:x"}),
    );
    assert!(response_scope(&window_two, "content".into(), None).reuse);
}

#[test]
fn workspace_symbol_next_page_keeps_relative_root() {
    let mut row = serde_json::json!({"next": {"nextPage": {"tool": "lspSearch", "query": {"queries": [
        {"operation": "workspaceSymbol", "symbolName": "f", "workspaceRoot": "/abs/repo/packages", "page": 2}
    ]}}, "read": {"tool": "localFetch", "query": {"queries": [{"path": "/abs/repo/packages"}]}}}});
    restore_root_spelling(&mut row, Some("/abs/repo/packages"), Some("packages"));
    assert_eq!(
        row["next"]["nextPage"]["query"]["queries"][0]["workspaceRoot"],
        "packages"
    );
    assert_eq!(
        row["next"]["read"]["query"]["queries"][0]["path"],
        "/abs/repo/packages"
    );
}

#[test]
fn workspace_symbol_accepts_directory_path() {
    let (_dir, root, paths) = temp_workspace();
    std::fs::create_dir_all(root.join("src")).expect("src");
    let mut q = query(serde_json::json!({
        "operation": "workspaceSymbol", "symbolName": "f",
        "path": root.join("src").to_string_lossy()
    }));
    let resolved = resolve_paths(&mut q, &paths).expect("a directory path is the root");
    let src = root.join("src").canonicalize().expect("canonical");
    assert_eq!(resolved, src.to_string_lossy());
    assert_eq!(
        lsp_search_query::workspace_root(&q),
        Some(src.to_string_lossy().as_ref())
    );
    // Other operations still need a file.
    let mut refs = query(serde_json::json!({
        "operation": "references", "symbolName": "f", "line": 1,
        "path": root.join("src").to_string_lossy()
    }));
    assert!(resolve_paths(&mut refs, &paths).is_err());
}

#[tokio::test(flavor = "current_thread")]
async fn incoming_walk_answers_level_one_before_importer_roots_join() {
    let (_dir, root, paths) = temp_workspace();
    let file = root.join("a.ts");
    std::fs::write(&file, "x\n").expect("file");
    let uri = octocode_engine::lsp::uri::path_to_uri(&file.to_string_lossy()).expect("uri");
    let mut graph = graph(&[
        ("target", &["a", "b"]),
        ("importer", &["c", "a"]),
        ("a", &["d"]),
    ]);
    graph.uri = Some(uri.clone());
    let at = |name: &str| node_at(name, Some(&uri));
    let cancel = crate::execution::cancel::NeverCancel;
    let mut walk = Walk::new(&paths, &[at("target")], Expansion::IncomingCalls, 2);
    assert!(walk.step(&graph, &cancel).await.expect("level 1"));
    // The level-1 answer names its files before any importer is verified.
    let canonical = file.canonicalize().expect("canonical");
    assert!(
        walk.answered_files()
            .contains(canonical.to_string_lossy().as_ref())
    );
    let before = graph.requests.get();
    walk.add_roots(
        &graph,
        vec![at("importer"), at("target")],
        std::collections::HashSet::new(),
        &cancel,
    )
    .await
    .expect("importer roots");
    // Only the new root is expanded; the anchor root is not asked twice.
    assert_eq!(graph.requests.get(), before + 1);
    let finished = walk.finish(&graph, &cancel).await.expect("walk");
    let summary = edge_summary(&finished);
    // `c` joins at level 1 from the importer root; `a` is listed for both
    // roots but expanded once; `d` is level 2.
    assert!(
        summary
            .iter()
            .any(|(name, level, _)| name == "c" && *level == 1),
        "{summary:?}"
    );
    assert_eq!(
        summary
            .iter()
            .filter(|(name, level, _)| name == "d" && *level == 2)
            .count(),
        1,
        "{summary:?}"
    );
}

#[test]
fn unresolved_anchor_leads_to_real_symbol_lines() {
    let q = query(serde_json::json!({
        "operation": "callers", "path": "/repo/a.ts",
        "symbolName": "render", "line": 300
    }));
    let source = (1..=400)
        .map(|line| match line {
            40 => "  render();".to_owned(),
            312 => "export function render(scene: Scene) {".to_owned(),
            350 => "  return render(next);".to_owned(),
            _ => format!("// line {line}"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut row = failure(
        &q,
        "file:///repo/a.ts",
        ErrorCode::AnchorUnresolved,
        "x",
        true,
    );
    anchor_recovery(&mut row, &q, Some(&source));
    let lead = |key: &str| row["next"][key]["query"]["queries"][0].clone();
    // The declaration line first, then the nearest uses.
    assert_eq!(lead("findSimilar")["symbolName"], "render", "{row}");
    assert_eq!(lead("findSimilar")["line"], 312, "{row}");
    assert_eq!(lead("findSimilar2")["line"], 350, "{row}");
    // Every candidate fits the tool's batch limit and gets a runnable suggestion.
    assert_eq!(lead("findSimilar3")["line"], 40, "{row}");
    assert_eq!(row["next"]["findSimilar"]["confidence"], "high", "{row}");
    let hints = row["hints"].to_string();
    assert!(!hints.contains("`render` appears on line 40."), "{row}");
    assert!(!hints.contains("312") && !hints.contains("350"), "{row}");
}

/// An orderHint past its line's occurrences leads to the last one on the
/// same line, with no listing of the name's other lines.
#[test]
fn an_order_hint_past_the_line_leads_to_its_last_occurrence() {
    let q = query(serde_json::json!({
        "operation": "definition", "path": "/repo/lib.rs",
        "symbolName": "config", "line": 3, "orderHint": 2
    }));
    let source = "struct L { config: u8 }\nfn a(config: u8) {}\nfn f() -> L { L { config: target.config } }\nfn b(config: u8) {}\n";
    let mut row = failure(
        &q,
        "file:///repo/lib.rs",
        ErrorCode::AnchorUnresolved,
        "x",
        true,
    );
    anchor_recovery(&mut row, &q, Some(source));
    let lead = &row["next"]["findSimilar"]["query"]["queries"][0];
    assert_eq!(lead["orderHint"], 1, "{row}");
    assert_eq!(lead["line"], 3, "{row}");
    assert!(row["next"].get("findSimilar2").is_none(), "{row}");
    assert!(
        !row["hints"].to_string().contains("appears on line"),
        "{row}"
    );
}

/// A near-miss spelling that only a comment holds is no anchor.
#[test]
fn near_names_skip_comment_words() {
    let q = query(serde_json::json!({
        "operation": "hover", "path": "/repo/anchor.rs",
        "symbolName": "Anchor", "line": 1
    }));
    let source = "//! The anchor of a request.\nfn other() {}\n";
    let mut row = failure(
        &q,
        "file:///repo/anchor.rs",
        ErrorCode::AnchorUnresolved,
        "x",
        true,
    );
    anchor_recovery(&mut row, &q, Some(source));
    assert!(row["next"].get("findSimilar").is_none(), "{row}");
    assert!(!row["hints"].to_string().contains("Closest names"), "{row}");
}

/// A name the file holds only in a comment, or only on the failing line,
/// never leads back to the failing query: a code line elsewhere is the
/// lead; without one, a workspace search for the name.
#[test]
fn unresolved_anchor_never_leads_back_to_itself() {
    let lead_for = |source: &str| {
        let q = query(serde_json::json!({
            "operation": "hover", "path": "/repo/anchor.rs",
            "symbolName": "Request", "line": 1
        }));
        let mut row = failure(
            &q,
            "file:///repo/anchor.rs",
            ErrorCode::AnchorUnresolved,
            "x",
            true,
        );
        anchor_recovery(&mut row, &q, Some(source));
        assert!(row["next"].get("findSimilar2").is_none(), "{row}");
        assert!(row["next"].get("read").is_none(), "{row}");
        row["next"]["findSimilar"]["query"]["queries"][0].clone()
    };
    let search = lead_for("//! Request handling.\nfn other() {}\n");
    assert_eq!(
        search,
        serde_json::json!({
            "operation": "workspaceSymbol", "path": "/repo/anchor.rs",
            "symbolName": "Request"
        })
    );
    let quoted = lead_for("//! Request handling.\nfn other() { let s = \"Request\"; }\n");
    assert_eq!(quoted["operation"], "workspaceSymbol", "{quoted}");
    let later = lead_for("//! Request handling.\nfn other() {}\nstruct Request;\n");
    assert_eq!(later["operation"], "hover", "{later}");
    assert_eq!(later["line"], 3, "{later}");
}

/// Leads that cover every candidate line leave no sentence beside them.
#[test]
fn covered_anchor_candidates_add_no_text() {
    let q = query(serde_json::json!({
        "operation": "callers", "path": "/repo/a.ts",
        "symbolName": "render", "line": 300
    }));
    let source = (1..=400)
        .map(|line| match line {
            312 => "export function render(scene: Scene) {".to_owned(),
            _ => format!("// line {line}"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut row = failure(
        &q,
        "file:///repo/a.ts",
        ErrorCode::AnchorUnresolved,
        "x",
        true,
    );
    let before = row["hints"].clone();
    anchor_recovery(&mut row, &q, Some(&source));
    assert_eq!(
        row["next"]["findSimilar"]["query"]["queries"][0]["line"],
        312
    );
    assert_eq!(row["hints"], before, "{row}");
}

#[test]
fn builtin_lib_and_text_only_counts_satisfy_the_output_contract() {
    let mut row = serde_json::json!({
        "lsp": {"serverAvailable": true},

            "kind": "callees",
            "files": [{"path": "/repo/a.ts", "matches": [{"symbolName": "f", "kind": "function", "line": 10, "endLine": 12, "sites": [{"line": 2, "column": 3}]}]}],
            "builtinLib": ["String.toUpperCase", "map"],
            "coverage": {"textOnlyFiles": 3, "reason": "importerScanCapped"}

    });
    stamp_coverage(&mut row);
    assert_eq!(row["coverage"]["state"], "partial", "{row}");
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("builtinLib and textOnlyFiles are contract-valid");
}

/// tsserver files calls made inside a top-level callback (`describe(() =>
/// …)`, `it(…)`) under the whole module; each site is named by the
/// innermost function symbol around it, as reference-derived callers are.
#[test]
fn module_callers_name_the_innermost_enclosing_callback() {
    let range = |start: u64, end: u64| serde_json::json!({"start": {"line": start, "character": 0}, "end": {"line": end, "character": 2}});
    let module = serde_json::json!({
        "name": "\"/repo/a.test\"", "kind": 2, "uri": "file:///repo/a.test.ts",
        "range": range(0, 40), "selectionRange": range(0, 40)
    });
    let site = |line: u64| serde_json::json!({"start": {"line": line, "character": 4}, "end": {"line": line, "character": 9}});
    let edge = HierarchyEdge {
        node: module,
        parent: None,
        level: 1,
        sites: vec![site(3), site(10), site(30)],
    };
    let symbols = serde_json::json!([{
        "name": "describe('x') callback", "kind": 12, "range": range(1, 12), "selectionRange": range(1, 1),
        "children": [{"name": "it('y')   callback", "kind": 12, "range": range(2, 5), "selectionRange": range(2, 2)}]
    }]);
    let split = split_module_callers(&edge, &symbols);
    let named = split
        .iter()
        .map(|edge| {
            (
                edge.node["name"].as_str().unwrap_or_default().to_owned(),
                edge.sites.len(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        named,
        vec![
            ("it('y') callback".to_owned(), 1),
            ("describe('x') callback".to_owned(), 1),
            ("\"/repo/a.test\"".to_owned(), 1),
        ]
    );
    assert_eq!(split[0].node["uri"], "file:///repo/a.test.ts");
    // A function caller is kept as the server named it.
    let mut function = edge;
    function.node["kind"] = serde_json::json!(12);
    assert_eq!(split_module_callers(&function, &symbols).len(), 1);
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
    for text in [failure.message.as_str(), failure.hint] {
        assert!(!text.contains("maxMemoryMb"), "{text}");
    }
    assert!(failure.hint.contains("unverified"), "{}", failure.hint);

    let query = query(serde_json::json!({
        "operation": "callers", "path": "/repo/src/lib.rs", "symbolName": "greet", "line": 3,
        "graphDepth": 3, "importerPage": 2, "page": 2, "pageSize": 4,
        "snapshot": "caller-snapshot", "workspaceRoot": "/repo", "orderHint": 1
    }));
    let row = failure.row(&query, "file:///repo/src/lib.rs");
    assert_eq!(row["errorCode"], "serverCrashed", "{row}");
    assert_eq!(row["hints"], serde_json::json!([failure.hint]), "{row}");
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

/// `callers` on a class lists the symbols that use it; the row says so.
#[test]
fn callers_of_a_class_are_labelled_as_uses() {
    let class = serde_json::json!({"name": "Widget", "kind": 5});
    let function = serde_json::json!({"name": "run", "kind": 12});
    let note = class_uses_warning("callers", std::slice::from_ref(&class)).expect("class note");
    assert!(
        note.contains("uses") && note.contains("not calls"),
        "{note}"
    );
    assert_eq!(
        class_uses_warning("callers", std::slice::from_ref(&function)),
        None
    );
    assert_eq!(
        class_uses_warning("callees", std::slice::from_ref(&class)),
        None
    );
    assert_eq!(
        class_uses_warning("subtypes", std::slice::from_ref(&class)),
        None
    );
}

/// Names of a page's rows, whichever key the kind lists them under.
fn page_names(row: &serde_json::Value) -> Vec<String> {
    ["matches", "symbols"]
        .iter()
        .filter_map(|key| row[*key].as_array())
        .flatten()
        .filter_map(|item| {
            item.get("name")
                .or_else(|| item.get("symbolName"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .collect()
}

/// Walk a result set copying each `next.nextPage` unchanged, under a
/// different response budget on every page (a continuation run in another
/// batch or window): the walk reaches every row exactly once, in order.
fn walk_across_budgets(
    first: &LspSearchQuery,
    budgets: &[usize],
    page: impl Fn(&LspSearchQuery, usize) -> serde_json::Value,
) -> Vec<String> {
    let mut q = first.clone();
    let mut seen = Vec::new();
    for call in 0.. {
        assert!(call < 1_000, "the walk must end");
        let row = with_next(&q, page(&q, budgets[call % budgets.len()]));
        assert!(row.get("errorCode").is_none(), "page {call}: {row}");
        assert_ne!(row["pagination"]["outOfRange"], true, "page {call}: {row}");
        let names = page_names(&row);
        assert!(!names.is_empty(), "page {call} moved: {row}");
        seen.extend(names);
        let Some(next) = row.pointer("/next/nextPage/query/queries/0") else {
            break;
        };
        q = serde_json::from_value(next.clone()).expect("executable continuation");
    }
    seen
}

#[test]
fn a_copied_continuation_resumes_at_the_next_row_under_any_budget() {
    // Rows of uneven size, so different budgets cut at different rows.
    let items = (0..60)
        .map(|index| {
            serde_json::json!({"name": format!("s{index:02}"), "detail": "x".repeat(index % 7 * 40)})
        })
        .collect::<Vec<_>>();
    let expected = items
        .iter()
        .map(|item| item["name"].as_str().expect("name").to_owned())
        .collect::<Vec<_>>();
    let q = query(serde_json::json!({
        "operation": "workspaceSymbol", "path": "file:///repo", "symbolName": "s"
    }));
    for budgets in [[600, 2_400, 900], [2_400, 600, 1_300]] {
        let seen = walk_across_budgets(&q, &budgets, |q, budget| {
            items_payload(q, "symbols", serde_json::json!(items), budget)
        });
        assert_eq!(seen, expected, "budgets {budgets:?}");
    }

    // documentSymbols pages top-level rows the same way.
    let range = |line: u64| serde_json::json!({"start": {"line": line, "character": 0}, "end": {"line": line, "character": 1}});
    let symbols = (0..60)
        .map(|index| {
            serde_json::json!({"name": format!("s{index:02}{}", "_".repeat(index as usize % 7 * 30)),
                "kind": 12, "range": range(index), "selectionRange": range(index)})
        })
        .collect::<Vec<_>>();
    let expected = symbols
        .iter()
        .map(|item| item["name"].as_str().expect("name").to_owned())
        .collect::<Vec<_>>();
    let q = query(serde_json::json!({
        "operation": "documentSymbols", "path": "file:///repo/src/lib.rs"
    }));
    for budgets in [[600, 2_400, 900], [2_400, 600, 1_300]] {
        let seen = walk_across_budgets(&q, &budgets, |q, budget| {
            items_payload(q, "documentSymbols", serde_json::json!(symbols), budget)
        });
        assert_eq!(seen, expected, "budgets {budgets:?}");
    }
}

#[test]
fn a_cursor_past_the_end_or_of_another_answer_never_skips_rows() {
    let items = (0..30)
        .map(|index| serde_json::json!({"name": format!("s{index:02}"), "detail": "x".repeat(80)}))
        .collect::<Vec<_>>();
    let q = query(serde_json::json!({
        "operation": "workspaceSymbol", "path": "file:///repo", "symbolName": "s"
    }));
    let first = with_next(
        &q,
        items_payload(&q, "symbols", serde_json::json!(items), 600),
    );
    let next: LspSearchQuery =
        serde_json::from_value(first["next"]["nextPage"]["query"]["queries"][0].clone())
            .expect("continuation");
    // The answer changed: the digest no longer matches, whatever the cursor.
    let mut changed = items.clone();
    changed.pop();
    let stale = items_payload(&next, "symbols", serde_json::json!(changed), 600);
    assert_eq!(stale["errorCode"], "staleSnapshot", "{stale}");
    // A cursor at the end of the set is out of range, not a clamped repeat.
    let pinned = lsp_search_query::snapshot(&next).expect("snapshot");
    let digest = pinned.split('@').next().expect("digest");
    let past = requery(
        &next,
        serde_json::json!({"snapshot": row_cursor(digest, 30, 2)}),
    );
    let row = items_payload(&past, "symbols", serde_json::json!(items), 600);
    assert_eq!(row["pagination"]["outOfRange"], true, "{row}");
    assert_eq!(row["matches"], serde_json::json!([]), "{row}");
}

/// A character-cut cursor binds its row and its page to the digest: an
/// edited row, or the cursor another page carries, is a stale snapshot
/// with the restart lead, never a page that silently skips or repeats rows.
#[test]
fn a_forged_or_misplaced_cursor_is_a_stale_snapshot() {
    let items = (0..30)
        .map(|index| serde_json::json!({"name": format!("s{index:02}"), "detail": "x".repeat(80)}))
        .collect::<Vec<_>>();
    let q = query(serde_json::json!({
        "operation": "workspaceSymbol", "path": "file:///repo", "symbolName": "s"
    }));
    let continuation = |row: &serde_json::Value| -> LspSearchQuery {
        serde_json::from_value(row["next"]["nextPage"]["query"]["queries"][0].clone())
            .expect("continuation")
    };
    let second = continuation(&with_next(
        &q,
        items_payload(&q, "symbols", serde_json::json!(items), 600),
    ));
    assert_eq!(lsp_search_query::page(&second), Some(2));
    let second_row = with_next(
        &second,
        items_payload(&second, "symbols", serde_json::json!(items), 600),
    );
    assert!(second_row.get("errorCode").is_none(), "{second_row}");
    let third = continuation(&second_row);
    assert_eq!(lsp_search_query::page(&third), Some(3));
    let stale = |pinned: &str| {
        let forged = requery(&second, serde_json::json!({"snapshot": pinned}));
        let row = items_payload(&forged, "symbols", serde_json::json!(items), 600);
        assert_eq!(row["errorCode"], "staleSnapshot", "{pinned}: {row}");
        let restart = &row["next"]["restart"]["query"]["queries"][0];
        for cursor in crate::response::result::CURSOR_FIELDS {
            assert!(restart.get(cursor).is_none(), "{cursor}: {row}");
        }
        assert!(row.get("matches").is_none(), "{row}");
    };
    // The row edited, the rest of the cursor kept.
    let pinned = lsp_search_query::snapshot(&second).expect("snapshot");
    let (digest, rest) = pinned.rsplit_once('@').expect("row cursor");
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    let row: usize = rest[..digits].parse().expect("row");
    stale(&format!("{digest}@{}{}", row + 1, &rest[digits..]));
    stale(&format!("{digest}@{}{}", row - 1, &rest[digits..]));
    // Page 3's cursor named on page 2.
    stale(lsp_search_query::snapshot(&third).expect("snapshot"));
}

/// A callers page is cut on the compact call rows it prints, so it fills
/// the budget instead of a third of it, and still reaches every edge once.
#[test]
fn caller_pages_fill_the_budget_with_compact_rows() {
    let items = (0..240)
        .map(|n| {
            serde_json::json!({
                "from": {"symbolName": format!("caller{n}"), "kind": "function",
                    "path": format!("/repo/src/module_{}/file.rs", n / 4),
                    "displayRange": {"startLine": n + 10, "startCharacter": 8, "endLine": n + 30, "endCharacter": 1}},
                "fromRanges": [{"startLine": n + 12, "startCharacter": 5, "endLine": n + 12, "endCharacter": 9}],
                "level": 1
            })
        })
        .collect::<Vec<_>>();
    let q = query(serde_json::json!({
        "operation": "callers", "path": "file:///repo/a.rs", "symbolName": "foo", "line": 1
    }));
    let budget = 4_000;
    let paths = repo_policy();
    let mut q = q;
    let mut seen = Vec::new();
    let mut fills = Vec::new();
    for call in 0.. {
        assert!(call < 100, "the walk must end");
        let row = with_next(&q, call_items_payload(&q, &items, None, budget, &paths));
        let chars = octocode_engine::json_size::utf16_units(&row["files"]).unwrap_or(0);
        assert!(chars <= budget, "page {call} is {chars} chars");
        fills.push(chars);
        seen.extend(
            row["files"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|file| file["matches"].as_array().into_iter().flatten())
                .map(|call| call["symbolName"].as_str().expect("name").to_owned()),
        );
        let Some(next) = row.pointer("/next/nextPage/query/queries/0") else {
            break;
        };
        q = serde_json::from_value(next.clone()).expect("continuation");
    }
    let expected = (0..240).map(|n| format!("caller{n}")).collect::<Vec<_>>();
    assert_eq!(seen, expected);
    // Every page but the last is at least 80% full.
    for fill in &fills[..fills.len() - 1] {
        assert!(*fill * 5 >= budget * 4, "{fills:?}");
    }
}

/// A directory named where a file anchors the request is `notAFile`, with
/// a lead into the directory: a text search for a named symbol, else its
/// file listing.
#[test]
fn a_directory_anchor_is_not_a_file_and_leads_into_it() {
    let root = tempfile::tempdir().expect("root");
    let root = root.path().canonicalize().expect("canonical root");
    std::fs::create_dir_all(root.join("src/tools")).expect("dir");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let named = query(serde_json::json!({
        "operation": "references", "path": "src/tools", "symbolName": "run", "line": 1
    }));
    let row = directory_anchor(&named, &paths).expect("a directory row");
    assert_eq!(row["status"], "error", "{row}");
    assert_eq!(row["errorCode"], "notAFile", "{row}");
    assert_eq!(
        row["error"], "src/tools is a directory, not a file.",
        "{row}"
    );
    assert!(row.get("hints").is_none(), "{row}");
    let lead = &row["next"]["findText"];
    assert_eq!(lead["tool"], "localSearch", "{row}");
    assert_eq!(
        lead["query"]["queries"][0],
        serde_json::json!({"path": "src/tools", "matchString": "\\brun\\b", "regex": "rust",
            "caseMode": "sensitive"}),
        "{row}"
    );
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("directory row satisfies the output contract");

    let unnamed = query(serde_json::json!({"operation": "documentSymbols", "path": "src/tools"}));
    let row = directory_anchor(&unnamed, &paths).expect("a directory row");
    let lead = &row["next"]["list"];
    assert_eq!(lead["tool"], "structureSearch", "{row}");
    assert_eq!(
        lead["query"]["queries"][0],
        serde_json::json!({"operation": "files", "path": "src/tools"}),
        "{row}"
    );
    // A file, or a path that does not exist, is not this failure.
    std::fs::write(root.join("src/a.rs"), "fn run() {}\n").expect("file");
    let file = query(serde_json::json!({"operation": "documentSymbols", "path": "src/a.rs"}));
    assert!(directory_anchor(&file, &paths).is_none());
    let missing = query(serde_json::json!({"operation": "documentSymbols", "path": "src/nope"}));
    assert!(directory_anchor(&missing, &paths).is_none());
    // A missing path names its error and the nearest listing, no prose.
    let failure = paths
        .validate_read(root.join("src/nope"))
        .map_err(LspFailure::path_denied)
        .expect_err("missing");
    let row = path_failure(&missing, &paths, failure).expect("a missing-path row");
    assert_eq!(row["errorCode"], "pathNotFound", "{row}");
    assert!(row.get("hints").is_none(), "{row}");
    assert_eq!(row["next"]["viewTree"]["tool"], "structureSearch", "{row}");
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("missing-path row satisfies the output contract");
}

/// The language server's root is spelled like every other row path:
/// relative to the workspace, `.` for the workspace itself.
#[test]
fn server_roots_are_workspace_relative() {
    let root = tempfile::tempdir().expect("root");
    let root = root.path().canonicalize().expect("canonical root");
    std::fs::create_dir_all(root.join("crates/runtime")).expect("dir");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let member = root.join("crates/runtime").to_string_lossy().into_owned();
    let mut row = serde_json::json!({
        "workspaceRoot": member,
        "lsp": {"receipt": {"workspaceRoot": member}}
    });
    relative_server_roots(&mut row, &paths);
    assert_eq!(row["workspaceRoot"], "crates/runtime", "{row}");
    assert_eq!(
        row["lsp"]["receipt"]["workspaceRoot"], "crates/runtime",
        "{row}"
    );
    let mut whole = serde_json::json!({"workspaceRoot": root.to_string_lossy()});
    relative_server_roots(&mut whole, &paths);
    assert_eq!(whole["workspaceRoot"], ".", "{whole}");
    // A root outside the workspace keeps its absolute spelling.
    let mut outside = serde_json::json!({"workspaceRoot": "/elsewhere/project"});
    relative_server_roots(&mut outside, &paths);
    assert_eq!(outside["workspaceRoot"], "/elsewhere/project", "{outside}");
}
