//! lspSearch server discovery and astSearch row paths through the runtime.

use crate::support::{Workspace, call, row_data, row_status};
use serde_json::json;

#[tokio::test]
async fn lsp_public_paths_and_paged_reads_preserve_provider_identity() {
    use crate::support::Run;
    assert!(
        std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_ok_and(|run| run.status.success()),
        "Node is required for the LSP process fixture"
    );
    let workspace = Workspace::new();
    let source = format!("function needle() {{}}\n{}", "needle();\n".repeat(20));
    let path = workspace.write("src/lib.ts", &source);
    let script = workspace.write("server.cjs", r#"
const fs = require('node:fs');
const { fileURLToPath } = require('node:url');
let buffer = Buffer.alloc(0);
function send(id, result) {
  const body = JSON.stringify({jsonrpc:'2.0', id, result});
  process.stdout.write('Content-Length: ' + Buffer.byteLength(body) + '\r\n\r\n' + body);
}
function handle(msg) {
  if (msg.method === 'initialize') return send(msg.id, {capabilities:{definitionProvider:true,referencesProvider:true,hoverProvider:true,textDocumentSync:1}});
  if (msg.method === 'exit') return process.exit(0);
  if (!msg.id) return;
  const uri = msg.params?.textDocument?.uri;
  if (uri) fs.appendFileSync(__dirname + '/uris.log', uri + '\n');
  const location = line => ({uri,range:{start:{line,character:line ? 0 : 9},end:{line,character:line ? 6 : 15}}});
  if (msg.method === 'textDocument/definition') return send(msg.id, [location(0)]);
  if (msg.method === 'textDocument/references') return send(msg.id, Array.from({length:21}, (_,line)=>location(line)));
  if (msg.method === 'textDocument/hover') return send(msg.id, {contents:{kind:'markdown',value:fileURLToPath(uri)}});
  send(msg.id, null);
}
process.stdin.on('data', chunk => {
  buffer = Buffer.concat([buffer,chunk]);
  for (;;) {
    const split = buffer.indexOf('\r\n\r\n');
    if (split < 0) return;
    const length = Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0,split).toString())[1]);
    if (buffer.length < split + 4 + length) return;
    const msg = JSON.parse(buffer.subarray(split+4,split+4+length));
    buffer = buffer.subarray(split+4+length);
    handle(msg);
  }
});
"#);
    let config = workspace.write(
        "config/lsp.json",
        json!({"languageServers":{".ts":{
            "command":"node","args":[script],"languageId":"plaintext"
        }}})
        .to_string(),
    );
    let runtime =
        workspace.runtime(&[("OCTOCODE_LSP_CONFIG", config.to_string_lossy().into_owned())]);
    let query =
        |operation: &str| json!({"path":path,"symbolName":"needle","line":1,"operation":operation});
    for view in ["content", "files"] {
        let mut request = query("references");
        request["view"] = json!(view);
        let output = runtime
            .run_mcp(
                format!("paths-{view}"),
                "lspSearch".into(),
                json!({"queries":[request]}),
            )
            .await
            .expect("MCP references");
        let row = &output["structuredContent"]["results"][0]["data"];
        assert_eq!(row["path"], "src/lib.ts", "{output}");
        let items = row[if view == "files" { "files" } else { "matches" }]
            .as_array()
            .expect("locations");
        assert!(
            items.iter().all(|item| item["path"] == "src/lib.ts"),
            "{output}"
        );
    }
    let walked = crate::support::walk(
        &runtime,
        "lspSearch",
        json!({"queries":[query("references")],"responseChars":600}),
        |output| {
            output["results"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|row| row["data"]["matches"].as_array().into_iter().flatten())
                .map(|location| {
                    assert_eq!(location["path"], "src/lib.ts");
                    format!("{}", location["displayRange"]["startLine"])
                })
                .collect()
        },
    )
    .await;
    assert_eq!(walked.rows.len(), 21, "every reference survives paging");
    let hover = call(&runtime, "lspSearch", query("hover"))
        .await
        .expect("hover");
    assert_eq!(row_data(&hover)["path"], "src/lib.ts");
    assert_eq!(
        row_data(&hover)["hover"]["contents"]["value"],
        path.to_string_lossy().as_ref(),
        "source-like hover text stays verbatim"
    );
    let mut invalid_occurrence = query("hover");
    invalid_occurrence["occurrence"] = json!(1);
    let failed = call(&runtime, "lspSearch", invalid_occurrence)
        .await
        .expect("anchor error");
    assert_eq!(row_status(&failed), "error");
    assert_eq!(row_data(&failed)["path"], "src/lib.ts");
    let read = crate::support::checks::related_call(row_data(&failed), "localFetch", |_| true);
    // A related call's visible input names workspace paths relative to
    // the workspace, as every other output path; it still executes.
    assert_eq!(read["input"]["queries"][0]["path"], "src/lib.ts");
    let recovered = call(&runtime, "localFetch", read["input"].clone())
        .await
        .expect("source recovery");
    assert!(
        row_data(&recovered)["content"]
            .as_str()
            .expect("source")
            .contains("function needle() {}")
    );
    let uris =
        std::fs::read_to_string(workspace.workspace.join("uris.log")).expect("provider requests");
    assert!(
        uris.lines()
            .all(|uri| uri.starts_with("file://") && uri.ends_with("/src/lib.ts")),
        "{uris}"
    );
    runtime.close().await;
}

/// The markdown server is a missing command, so the row does not depend on
/// which language servers this machine has installed.
#[tokio::test]
async fn lsp_search_returns_a_typed_row_when_no_server_is_configured() {
    let workspace = Workspace::new();
    let path = workspace.write("notes.md", "# title\n");
    // Inside the workspace: anchored operations read the server config
    // through the path policy.
    let config = workspace.write(
        "config/lsp.json",
        json!({"languageServers": {".md": {
            "command": "octocode-test-missing-lsp", "args": [], "languageId": "markdown"
        }}})
        .to_string(),
    );
    let runtime =
        workspace.runtime(&[("OCTOCODE_LSP_CONFIG", config.to_string_lossy().into_owned())]);
    let outcome = call(
        &runtime,
        "lspSearch",
        json!({
            "operation": "documentSymbols",
            "path": path
        }),
    )
    .await
    .expect("lspSearch");
    assert_eq!(
        row_status(&outcome),
        "error",
        "{}",
        outcome.structured_content
    );
    assert_eq!(
        row_data(&outcome)["errorCode"],
        "serverUnavailable",
        "{}",
        outcome.structured_content
    );
    assert!(
        row_data(&outcome)["hints"][0]
            .as_str()
            .expect("recovery hint")
            .contains("semantic results remain unavailable")
    );
    let outline = crate::support::checks::related_call(row_data(&outcome), "localFetch", |input| {
        input["queries"][0]["view"] == "symbols"
    });
    let headings = call(&runtime, "localFetch", outline["input"].clone())
        .await
        .expect("outline recovery");
    assert!(
        row_data(&headings)["content"]
            .as_str()
            .expect("headings")
            .contains("# title")
    );
    let symbol_failure = call(
        &runtime,
        "lspSearch",
        json!({
            "operation": "definition",
            "path": path,
            "symbolName": "title",
            "line": 1
        }),
    )
    .await
    .expect("symbol recovery output satisfies its contract");
    assert_eq!(row_status(&symbol_failure), "error");
    assert_eq!(
        row_data(&symbol_failure)["errorCode"],
        "serverUnavailable",
        "{}",
        symbol_failure.structured_content
    );
    let read =
        crate::support::checks::related_call(row_data(&symbol_failure), "localFetch", |input| {
            input["queries"][0]["path"] == "notes.md"
        });
    let recovered = call(&runtime, "localFetch", read["input"].clone())
        .await
        .expect("related read executes");
    assert!(
        row_data(&recovered)["content"]
            .as_str()
            .expect("content")
            .contains("title")
    );
    runtime.close().await;
}

#[tokio::test]
async fn unsupported_ast_batch_keeps_indices_hints_and_executable_document_recovery() {
    use crate::support::Run;
    let workspace = Workspace::new();
    let path = workspace.write(
        "instructions.md",
        "# Instructions\nRead this.\n## Details\nMore text.\n",
    );
    let runtime = workspace.runtime(&[]);
    let response = runtime
        .run_mcp(
            "unsupported-batch".into(),
            "astSearch".into(),
            json!({"queries":[
                {"operation":"notAnOperation","path":path},
                {"operation":"syntaxTree","path":path}
            ]}),
        )
        .await
        .expect("MCP error batch");
    assert_eq!(response["isError"], true);
    let rows = &response["structuredContent"]["results"];
    assert_eq!(rows[0]["index"], 0);
    assert_eq!(rows[1]["index"], 1);
    assert_eq!(rows[0]["data"]["errorCode"], "invalidInput");
    assert!(rows[0]["data"]["hints"][0].is_string());
    let data = &rows[1]["data"];
    assert_eq!(data["errorCode"], "languageUnsupported");
    assert!(
        data["hints"][0]
            .as_str()
            .expect("hint")
            .contains("localFetch")
    );
    let read = crate::support::checks::related_call(data, "localFetch", |_| true);
    let recovered = call(&runtime, "localFetch", read["input"].clone())
        .await
        .expect("outline read");
    assert!(row_data(&recovered).get("view").is_none());
    let content = row_data(&recovered)["content"].as_str().expect("headings");
    assert!(
        content.contains("# Instructions") && content.contains("## Details"),
        "{content}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn resolved_lsp_config_path_reaches_engine_discovery() {
    let workspace = Workspace::new();
    let path = workspace.write("source.custom", "symbol\n");
    let config_path = workspace.write(
        ".octocode/lsp.json",
        r#"{"languageServers":{".custom":{"command":"missing-custom-lsp","args":[],"languageId":"custom"}}}"#,
    );
    let runtime = workspace.runtime(&[(
        "OCTOCODE_LSP_CONFIG",
        config_path.to_string_lossy().into_owned(),
    )]);
    let outcome = call(
        &runtime,
        "lspSearch",
        json!({
            "operation": "documentSymbols",
            "path": path
        }),
    )
    .await
    .expect("typed configured server failure");
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(
        !rendered.contains("No language server is configured"),
        "resolved config was ignored: {rendered}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn ast_search_match_and_symbol_paths_are_executable() {
    let workspace = Workspace::new();
    let source = workspace.write("pkg/src/lib.rs", "pub fn needle() { helper(1); }\n");
    let scope = source.parent().expect("scope").to_path_buf();
    let runtime = workspace.runtime(&[]);

    let matched = call(
        &runtime,
        "astSearch",
        json!({"operation":"match","path":scope,"parser":"rust","rule":{"pattern":"helper($A)"}}),
    )
    .await
    .expect("astSearch match");
    let path = row_data(&matched)["files"][0]["path"]
        .as_str()
        .unwrap_or_else(|| panic!("match row path: {}", matched.structured_content));
    assert!(
        workspace.workspace.join(path).is_file(),
        "base + match path must resolve: {}",
        matched.structured_content
    );

    let symbols = call(
        &runtime,
        "astSearch",
        json!({"operation":"symbols","path":scope}),
    )
    .await
    .expect("astSearch symbols");
    let path = row_data(&symbols)["files"][0]["path"]
        .as_str()
        .unwrap_or_else(|| panic!("symbol row path: {}", symbols.structured_content));
    assert!(
        workspace.workspace.join(path).is_file(),
        "base + symbol path must resolve: {}",
        symbols.structured_content
    );
    runtime.close().await;
}

#[tokio::test]
async fn workspace_root_symbol_queries_infer_the_server_from_project_markers() {
    let workspace = Workspace::new();
    workspace.write("Cargo.toml", "[package]\nname = \"demo\"\n");
    let config_path = workspace.write(
        ".octocode/lsp.json",
        r#"{"languageServers":{".rs":{"command":"missing-rust-lsp-for-root-test","args":[],"languageId":"rust"}}}"#,
    );
    let runtime = workspace.runtime(&[(
        "OCTOCODE_LSP_CONFIG",
        config_path.to_string_lossy().into_owned(),
    )]);
    let outcome = call(
        &runtime,
        "lspSearch",
        json!({
            "operation": "workspaceSymbol",
            "workspaceRoot": workspace.workspace,
            "symbolQuery": "main"
        }),
    )
    .await
    .expect("typed workspace-root result");
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(
        !rendered.contains("No language server is configured")
            && !rendered.contains("could be inferred"),
        "workspace root must select a server from Cargo.toml: {rendered}"
    );
    assert!(
        rendered.contains("missing-rust-lsp-for-root-test"),
        "the Rust route for the root should have been attempted: {rendered}"
    );
    runtime.close().await;
}
