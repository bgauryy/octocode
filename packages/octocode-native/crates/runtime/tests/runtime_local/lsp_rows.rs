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
