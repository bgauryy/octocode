//! lspSearch rows as the runtime serves them, against a fake language server:
//! how a warning names a file outside the workspace.

use crate::support::{Run, Workspace};
use serde_json::json;

/// A fake language server: `references` lists `SITES` (file under `ROOT`,
/// zero-based line and character, each six characters long), and
/// `definition` places every spelling on `DECL`.
const SERVER: &str = r#"
/*CONFIG*/
let buffer = Buffer.alloc(0);
function send(id, result) {
  const body = JSON.stringify({jsonrpc:'2.0', id, result});
  process.stdout.write('Content-Length: ' + Buffer.byteLength(body) + '\r\n\r\n' + body);
}
const site = (s) => ({uri: 'file://' + ROOT + '/' + s[0], range: {start: {line: s[1], character: s[2]}, end: {line: s[1], character: s[2] + 1}}});
function handle(msg) {
  if (msg.method === 'initialize') return send(msg.id, {capabilities:{definitionProvider:true,referencesProvider:true,textDocumentSync:1}});
  if (msg.method === 'exit') return process.exit(0);
  if (msg.id === undefined || !msg.method) return;
  if (msg.method === 'textDocument/references') return send(msg.id, SITES.map(site));
  if (msg.method === 'textDocument/definition') return send(msg.id, [site(DECL)]);
  send(msg.id, null);
}
process.stdin.on('data', chunk => {
  buffer = Buffer.concat([buffer, chunk]);
  for (;;) {
    const split = buffer.indexOf('\r\n\r\n');
    if (split < 0) return;
    const length = Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0, split).toString())[1]);
    if (buffer.length < split + 4 + length) return;
    const msg = JSON.parse(buffer.subarray(split + 4, split + 4 + length));
    buffer = buffer.subarray(split + 4 + length);
    handle(msg);
  }
});
"#;

fn node_available() -> bool {
    std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|run| run.status.success())
}

/// Serve `extension` files under `root` with the fake server listing `sites`
/// and placing every definition at `declaration`.
fn fake_server(
    home: &Workspace,
    root: &std::path::Path,
    (extension, language): (&str, &str),
    sites: &[(&str, u32, u32)],
    declaration: (&str, u32, u32),
) {
    let config = format!(
        "const ROOT = {}; const SITES = {}; const DECL = {};",
        json!(root.to_string_lossy()),
        json!(sites),
        json!(declaration),
    );
    let script = home.write("server.cjs", SERVER.replace("/*CONFIG*/", &config));
    std::fs::write(
        home.home.join("lsp-servers.json"),
        json!({"languageServers":{(extension):{
            "command":"node","args":[script],"languageId":language
        }}})
        .to_string(),
    )
    .expect("lsp config");
}

/// Outside the workspace a row names its files from the root it states once;
/// a warning that quotes a file and line names it the same way, never by its
/// absolute path.
#[tokio::test]
async fn a_warning_names_a_file_outside_the_workspace_as_its_rows_do() {
    if !node_available() {
        return;
    }
    let workspace = Workspace::new();
    let outside = Workspace::new();
    outside.write("crate/Cargo.toml", "[package]\nname = \"c\"\n");
    let lib = outside.write(
        "crate/lib.rs",
        "fn helper() {}\nfn used() { helper(); }\n\n#[cfg(test)]\nmod tests {\n    fn t() { helper(); }\n}\n",
    );
    // The server lists the declaration and the first call, not the one in
    // the test module on line 6.
    fake_server(
        &workspace,
        &outside.workspace,
        (".rs", "rust"),
        &[("crate/lib.rs", 0, 3), ("crate/lib.rs", 1, 14)],
        ("crate/lib.rs", 0, 3),
    );
    let runtime = workspace.runtime(&[(
        "ALLOWED_PATHS",
        outside.workspace.to_string_lossy().into_owned(),
    )]);
    // Every fixture is written. The request runs on this thread (the test's
    // current-thread runtime): settled here, no fixture is read inside its
    // settle window, so the page never seals its revision as unsettled.
    octocode_engine::stamp::settle_written_so_far();
    let output = runtime
        .run(
            "outside-warning".into(),
            "lspSearch".into(),
            json!({"queries":[{"operation":"references","path":lib,
                "symbolName":"helper","line":1}]}),
        )
        .await
        .expect("references")
        .structured_content
        .into_value();
    runtime.close().await;
    let data = &output["results"][0]["data"];
    assert_eq!(data["root"], json!(outside.workspace), "{data}");
    let warnings = data["warnings"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .chain(output["warnings"].as_array().cloned().unwrap_or_default())
        .filter_map(|warning| warning.as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    let quoting = warnings
        .iter()
        .filter(|warning| warning.contains("lib.rs:6"))
        .collect::<Vec<_>>();
    assert!(
        !quoting.is_empty(),
        "a warning names the missed line: {output}"
    );
    let absolute = outside.workspace.to_string_lossy();
    for warning in quoting {
        assert!(warning.contains("crate/lib.rs:6"), "{warning}");
        assert!(!warning.contains(absolute.as_ref()), "{warning}");
    }
}

/// A fake language server that knows `helper`'s declaration (line 1 of the
/// asked file) until its first answer is out; then a load begins that
/// reports its work every 200 ms until the test writes `released` beside the
/// script, and answers `[]` while it lasts.
const LOADING_SERVER: &str = r#"
const fs = require('fs');
let buffer = Buffer.alloc(0);
let loaded = true;
let asked = 0;
function write(message) {
  const body = JSON.stringify(message);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(body) + '\r\n\r\n' + body);
}
const send = (id, result) => write({jsonrpc: '2.0', id, result});
const progress = (value) => write({jsonrpc: '2.0', method: '$/progress', params: {token: 'load', value}});
function load() {
  loaded = false;
  progress({kind: 'begin', title: 'Loading'});
  const ticks = setInterval(() => {
    if (fs.existsSync(__dirname + '/released')) {
      clearInterval(ticks);
      loaded = true;
      progress({kind: 'end'});
      return;
    }
    progress({kind: 'report', message: 'indexing'});
  }, 200);
}
function handle(msg) {
  if (msg.method === 'initialize') {
    fs.appendFileSync(__dirname + '/started.txt', 'started\n');
    return send(msg.id, {capabilities: {definitionProvider: true, referencesProvider: true, textDocumentSync: 1}});
  }
  if (msg.method === 'exit') return process.exit(0);
  if (msg.id === undefined || !msg.method) return;
  if (msg.method === 'textDocument/definition') {
    asked += 1;
    const known = loaded;
    if (asked === 1) setImmediate(load);
    return send(msg.id, known
      ? [{uri: msg.params.textDocument.uri, range: {start: {line: 0, character: 9}, end: {line: 0, character: 15}}}]
      : []);
  }
  send(msg.id, null);
}
process.stdin.on('end', () => process.exit(0));
process.stdin.on('data', chunk => {
  buffer = Buffer.concat([buffer, chunk]);
  for (;;) {
    const split = buffer.indexOf('\r\n\r\n');
    if (split < 0) return;
    const length = Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0, split).toString())[1]);
    if (buffer.length < split + 4 + length) return;
    const msg = JSON.parse(buffer.subarray(split + 4, split + 4 + length));
    buffer = buffer.subarray(split + 4 + length);
    handle(msg);
  }
});
"#;

/// A1, end to end: an MCP host gives up on a request after its own timeout,
/// and a language server may index for longer. An `lspSearch` row on the MCP
/// channel whose caller set no budget stops waiting for a server that keeps
/// reporting its load once the reply budget is spent
/// (`RuntimeLimits::mcp_reply_budget`): the row says the server is still
/// indexing and offers the same query again, and that retry reaches the same
/// server, which answers once its load is done. The load here ends only when
/// the test releases it, so a row that arrives at all was ended by the
/// budget. The server is started and answering before the bounded call: the
/// budget is spent on the load, never on the server's start.
#[tokio::test]
async fn an_mcp_row_reports_a_server_still_indexing_within_the_reply_budget() {
    use octocode_runtime_base::execution::RuntimeLimits;
    if !node_available() {
        return;
    }
    let workspace = Workspace::new();
    // A repository bounds the project: the row judges the server's answer.
    std::fs::create_dir_all(workspace.workspace.join(".git")).expect("repository");
    let source = workspace.write("a.ts", "function helper() {}\nhelper();\n");
    let script = workspace.write("server.cjs", LOADING_SERVER);
    let beside = script.parent().expect("the script's directory").to_owned();
    std::fs::write(
        workspace.home.join("lsp-servers.json"),
        json!({"languageServers":{".ts":{
            "command":"node","args":[script],"languageId":"shellscript"
        }}})
        .to_string(),
    )
    .expect("lsp config");
    let limits = RuntimeLimits {
        mcp_reply_budget: std::time::Duration::from_secs(3),
        ..crate::support::FIXTURE_LIMITS
    };
    // The default budget must not be what ends the wait below.
    let patience = RuntimeLimits::DEFAULT.mcp_reply_budget / 2;
    assert!(limits.mcp_reply_budget < patience);
    let runtime = workspace.runtime_with(&[], limits);
    octocode_engine::stamp::settle_written_so_far();
    let input = || {
        json!({"queries":[{"operation":"definition","path":source,
            "symbolName":"helper","line":2}]})
    };
    let located = |output: &serde_json::Value| {
        let row = &output["results"][0];
        assert_ne!(row["status"], "error", "{output}");
        assert_ne!(row["data"]["isPartial"], true, "{output}");
        assert!(
            output.to_string().contains("function helper"),
            "the declaration is served: {output}"
        );
    };
    // The CLI channel has no default: it would wait the load out, so it asks
    // while the server still knows the answer.
    let first = runtime
        .run("lsp-warm".into(), "lspSearch".into(), input())
        .await
        .expect("definition")
        .structured_content
        .into_value();
    located(&first);
    let bounded = tokio::time::timeout(
        patience,
        runtime.run_mcp("lsp-loading".into(), "lspSearch".into(), input()),
    )
    .await
    .expect("the reply budget ends the wait for the load")
    .expect("the indexing row");
    let payload = crate::support::mcp_payload(&bounded);
    let data = &payload["results"][0]["data"];
    assert_eq!(
        data["partialReasons"],
        json!(["languageServerIndexing"]),
        "{payload}"
    );
    let reruns = |call: &serde_json::Value| {
        call["tool"] == "lspSearch" && call["input"]["queries"][0]["symbolName"] == "helper"
    };
    assert!(
        data["related"]
            .as_array()
            .is_some_and(|related| related.iter().any(reruns))
            || payload["related"]
                .as_array()
                .is_some_and(|related| related.iter().any(reruns)),
        "the same query is offered again: {payload}"
    );
    std::fs::write(beside.join("released"), "").expect("release the load");
    // The retry, from a caller with no bound on its wait: the load ends and
    // the server settles in its own time.
    let again = runtime
        .run("lsp-loaded".into(), "lspSearch".into(), input())
        .await
        .expect("definition")
        .structured_content
        .into_value();
    located(&again);
    let started = std::fs::read_to_string(beside.join("started.txt")).unwrap_or_default();
    assert_eq!(
        started.lines().count(),
        1,
        "the retry reuses the loading server"
    );
    runtime.close().await;
}
