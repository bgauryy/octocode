//! Type-hierarchy and implementation answers when the server cannot give them:
//! the labelled heritage-clause scan.

use super::*;

/// A server with no `typeHierarchyProvider` still answers supertypes and
/// subtypes: subtypes are its implementation answer (never the queried
/// declaration itself), supertypes are the definitions of the heritage
/// clause, and each row names its source.
#[test]
fn type_hierarchy_falls_back_when_the_server_has_no_type_hierarchy() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::lsp_search::ops::Operation;
    use crate::tools::lsp_search::source::{SourceCache, snippet_policy};
    use serde_json::json;
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    std::fs::write(root.join("b.ts"), "export interface Base {}\n").expect("base");
    std::fs::write(root.join("a.ts"), "export class Child extends Base {}\n").expect("child");
    let server = FAKE_SERVER
        .replace(
            "hoverProvider: true",
            "implementationProvider: true, definitionProvider: true",
        )
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            r#"  } else if (msg.method === 'textDocument/implementation') {
    const dir = msg.params.textDocument.uri.slice(0, msg.params.textDocument.uri.lastIndexOf('/') + 1);
    const at = (name, character, length) => ({ uri: dir + name, range: { start: { line: 0, character }, end: { line: 0, character: character + length } } });
    send({ jsonrpc: '2.0', id: msg.id, result: [at('a.ts', 13, 5), at('b.ts', 17, 4)] });
  } else if (msg.method === 'textDocument/definition') {
    const dir = msg.params.textDocument.uri.slice(0, msg.params.textDocument.uri.lastIndexOf('/') + 1);
    send({ jsonrpc: '2.0', id: msg.id, result: [{ uri: dir + 'b.ts', range: { start: { line: 0, character: 17 }, end: { line: 0, character: 21 } } }] });"#,
        );
    std::fs::write(&script, server).expect("fixture server");
    let paths = crate::tools::test_support::workspace_policy(&root);
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &script, None));
        client.start().await.expect("server starts");
        for (operation, source, file, symbol, character, expected) in [
            ("subtypes", "implementation", "b.ts", "Base", 17, "a.ts"),
            ("supertypes", "heritage", "a.ts", "Child", 13, "b.ts"),
        ] {
            let path = root.join(file).to_string_lossy().into_owned();
            let query = serde_json::from_value(json!({
                "operation":operation, "path":path, "symbolName":symbol, "line":1
            }))
            .expect("query");
            let mut sources = SourceCache::new(&paths);
            let policy = snippet_policy(&paths);
            let row = Operation {
                client: &client,
                query: &query,
                sources: &mut sources,
                snippet_policy: &policy,
                cancel: &NeverCancel,
                path: &path,
                scope: &crate::tools::lsp_search::scope::Scope::new(
                    root.to_string_lossy().into_owned(),
                    Vec::new(),
                ),
                root_only: false,
                line: 0,
                character,
                language_id: Some("typescript"),
                budget: 16_000,
                response_window: None,
            }
            .run()
            .await
            .unwrap_or_else(|error| panic!("{operation} fails without a hierarchy: {error:?}"));
            assert_eq!(row["lsp"]["source"], source, "{row}");
            let paths = row["matches"]
                .as_array()
                .expect("matches")
                .iter()
                .map(|found| found["path"].as_str().unwrap_or_default().to_owned())
                .collect::<Vec<_>>();
            assert!(
                paths.iter().all(|found| found.ends_with(expected)),
                "{operation} lists only other declarations: {row}"
            );
            assert_eq!(paths.len(), 1, "{row}");
            assert!(row.get("isPartial").is_none(), "{row}");
        }
        client.stop().await.expect("fixture server closed");
    });
}

/// When the implementation request cannot be answered (here a server error;
/// a timeout takes the same path), `subtypes` is the heritage-clause scan
/// of every workspace file spelling the base: labelled rows, never a bare
/// failure.
#[test]
fn subtypes_fall_back_to_a_labelled_heritage_scan_when_implementation_fails() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::lsp_search::ops::Operation;
    use crate::tools::lsp_search::source::{SourceCache, snippet_policy};
    use serde_json::json;
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    std::fs::write(root.join("b.ts"), "export interface Base {}\n").expect("base");
    std::fs::write(root.join("a.ts"), "export class Child extends Base {}\n").expect("child");
    std::fs::write(
        root.join("c.ts"),
        "export interface Wide extends Other, Base {}\n",
    )
    .expect("wide");
    std::fs::write(root.join("d.ts"), "export class Skip extends Other {}\n").expect("other");
    std::fs::write(root.join("e.ts"), "// Base\nconst x: Base = {};\n").expect("mention");
    let server = FAKE_SERVER
        .replace("hoverProvider: true", "implementationProvider: true")
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            r#"  } else if (msg.method === 'textDocument/implementation') {
    send({ jsonrpc: '2.0', id: msg.id, error: { code: -32603, message: 'Debug Failure' } });"#,
        );
    std::fs::write(&script, server).expect("fixture server");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let path = root.join("b.ts").to_string_lossy().into_owned();
    let uri = octocode_engine::lsp::uri::path_to_uri(&path).expect("uri");
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &script, None));
        client.start().await.expect("server starts");
        let query = serde_json::from_value(json!({
            "operation":"subtypes", "path":uri, "symbolName":"Base", "line":1
        }))
        .expect("query");
        let mut sources = SourceCache::new(&paths);
        let policy = snippet_policy(&paths);
        let row = Operation {
            client: &client,
            query: &query,
            sources: &mut sources,
            snippet_policy: &policy,
            cancel: &NeverCancel,
            path: &path,
            scope: &crate::tools::lsp_search::scope::Scope::new(
                root.to_string_lossy().into_owned(),
                vec!["*.ts".into()],
            ),
            root_only: false,
            line: 0,
            character: 17,
            language_id: Some("typescript"),
            budget: 16_000,
            response_window: None,
        }
        .run()
        .await
        .unwrap_or_else(|error| panic!("a failed implementation request falls back: {error:?}"));
        assert_eq!(row["lsp"]["source"], "native-graph-facts", "{row}");
        let found = row["matches"]
            .as_array()
            .expect("matches")
            .iter()
            .map(|found| {
                Path::new(found["path"].as_str().unwrap_or_default())
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        assert_eq!(found, ["a.ts", "c.ts"], "{row}");
        assert!(
            row["warnings"].to_string().contains("unverified"),
            "the rows are labelled unverified: {row}"
        );
        let query = serde_json::to_value(&query).expect("query json");
        assert_row_contract(ToolId::LspSearch, &query, &row);
        // The scan is a labelled stand-in: the same query, retried, asks the
        // server again.
        assert_eq!(row["next"]["retry"]["tool"], "lspSearch", "{row}");
        assert_eq!(row["isPartial"], true, "{row}");
        client.stop().await.expect("fixture server closed");
    });
}

/// Runs `operation` on `file`'s symbol (zero-based `line`/`character`)
/// against a server whose `textDocument/implementation` fails, over a
/// workspace where `Child` extends `Base` and `Impl` implements it.
fn implementation_failing_row(
    operation: &str,
    anchor: (&str, &str, u32, u32),
) -> Result<serde_json::Value, crate::tools::lsp_search::LspFailure> {
    implementation_row(
        "error: { code: -32603, message: 'Debug Failure' }",
        operation,
        anchor,
    )
}

/// [`implementation_failing_row`] against a server whose
/// `textDocument/implementation` replies with `reply` (a JSON-RPC
/// `result: …` or `error: …` member, as JavaScript).
fn implementation_row(
    reply: &str,
    operation: &str,
    (file, symbol, line, character): (&str, &str, u32, u32),
) -> Result<serde_json::Value, crate::tools::lsp_search::LspFailure> {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::lsp_search::ops::Operation;
    use crate::tools::lsp_search::source::{SourceCache, snippet_policy};
    use serde_json::json;
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    std::fs::write(
        root.join("b.ts"),
        "export interface Base {\n  run(): void;\n}\nexport class Plain {\n  go(): void {}\n}\n",
    )
    .expect("base");
    std::fs::write(root.join("a.ts"), "export class Child extends Base {}\n").expect("child");
    std::fs::write(
        root.join("c.ts"),
        "export class Impl implements Other, Base { run(): void {} }\n",
    )
    .expect("impl");
    std::fs::write(root.join("e.ts"), "// Base\nconst x: Base = {};\n").expect("mention");
    let server = FAKE_SERVER
        .replace("hoverProvider: true", "implementationProvider: true")
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            &format!(
                "  }} else if (msg.method === 'textDocument/implementation') {{\n    send({{ jsonrpc: '2.0', id: msg.id, {reply} }});"
            ),
        );
    std::fs::write(&script, server).expect("fixture server");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let path = root.join(file).to_string_lossy().into_owned();
    let uri = octocode_engine::lsp::uri::path_to_uri(&path).expect("uri");
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &script, None));
        client.start().await.expect("server starts");
        let query = serde_json::from_value(json!({
            "operation": operation, "path": uri, "symbolName": symbol, "line": line + 1
        }))
        .expect("query");
        let mut sources = SourceCache::new(&paths);
        let policy = snippet_policy(&paths);
        let outcome = Operation {
            client: &client,
            query: &query,
            sources: &mut sources,
            snippet_policy: &policy,
            cancel: &NeverCancel,
            path: &path,
            scope: &crate::tools::lsp_search::scope::Scope::new(
                root.to_string_lossy().into_owned(),
                vec!["*.ts".into()],
            ),
            root_only: false,
            line,
            character,
            language_id: Some("typescript"),
            budget: 16_000,
            response_window: None,
        }
        .run()
        .await;
        client.stop().await.expect("fixture server closed");
        outcome
    })
}

/// Beside a server answer, the scan's extra implementations are counted
/// once as a notice (said on page 1 only); the meaning of their `source`
/// label lives in the contract, not on every page.
#[test]
fn scan_only_implementations_are_counted_in_a_first_page_notice() {
    let row = implementation_row("result: []", "implementation", ("b.ts", "Base", 0, 17))
        .unwrap_or_else(|error| panic!("the server answers: {error:?}"));
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["scanOnlyImplementations"]),
        "{row}"
    );
    assert!(row.get("warnings").is_none(), "{row}");
    assert_eq!(
        row["notices"],
        serde_json::json!([
            "2 implementation(s) are source:\"native-graph-facts\" (matched by name)."
        ]),
        "{row}"
    );
}

/// `implementation` on a type is answered by the same labelled heritage
/// scan as `subtypes` when the server cannot answer in time (a failed
/// request stands in for the timeout): rows by name, labelled unverified,
/// with a retry that asks the server again.
#[test]
fn implementation_on_a_type_falls_back_to_the_labelled_heritage_scan() {
    let row = implementation_failing_row("implementation", ("b.ts", "Base", 0, 17))
        .unwrap_or_else(|error| panic!("a type anchor falls back: {error:?}"));
    assert_eq!(row["lsp"]["source"], "native-graph-facts", "{row}");
    assert_eq!(row["kind"], "implementation", "{row}");
    let files = row["matches"]
        .as_array()
        .expect("matches")
        .iter()
        .map(|found| {
            Path::new(found["path"].as_str().unwrap_or_default())
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    assert_eq!(files, ["a.ts", "c.ts"], "{row}");
    let warnings = row["warnings"].to_string();
    assert!(warnings.contains("unverified"), "{row}");
    assert!(
        warnings.contains("implementations") && !warnings.contains("subtypes are"),
        "the note speaks of the operation asked: {row}"
    );
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(row["next"]["retry"]["tool"], "lspSearch", "{row}");
    assert_eq!(
        row["next"]["retry"]["input"]["queries"][0]["operation"], "implementation",
        "{row}"
    );
    let query = serde_json::json!({"operation": "implementation", "path": "b.ts", "symbolName": "Base", "line": 1});
    assert_row_contract(ToolId::LspSearch, &query, &row);
}

/// A member has no heritage clause to scan: `implementation` on it keeps
/// the server failure (a scan would claim "no implementations").
#[test]
fn implementation_on_a_member_keeps_the_server_failure() {
    let failure = implementation_failing_row("implementation", ("b.ts", "run", 1, 2))
        .expect_err("no scan can answer a member");
    assert_eq!(failure.code, ErrorCode::RequestFailed);
    let tip = crate::response::rows::error_code_hint(
        ToolId::LspSearch,
        failure.code,
        &serde_json::json!({"operation": "implementation", "path": "b.ts", "symbolName": "run", "line": 1}),
    )
    .expect("a request failure's tip");
    assert!(!tip.contains("workspaceRoot"), "{tip}");
}
