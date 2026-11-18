//! Server-written text as lspSearch serves it: a hover is judged against the
//! file it is about, and shows what a read of that file shows. All secrets
//! are fake.

use super::*;
use octocode_runtime_base::execution::cancel::NeverCancel;
use serde_json::{Value, json};

/// A fake language server whose hover is `HOVER`, as markdown.
const SERVER: &str = r#"#!/usr/bin/env node
/*CONFIG*/
let buf = Buffer.alloc(0);
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
function handle(msg) {
  if (msg.method === 'initialize') {
    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { hoverProvider: true, textDocumentSync: 1 } } });
  } else if (msg.method === 'textDocument/hover') {
    send({ jsonrpc: '2.0', id: msg.id, result: { contents: { kind: 'markdown', value: HOVER } } });
  } else if (msg.method === 'exit') {
    process.exit(0);
  } else if (msg.id !== undefined && msg.method) {
    send({ jsonrpc: '2.0', id: msg.id, result: null });
  }
}
process.stdin.on('data', (d) => {
  buf = Buffer.concat([buf, d]);
  for (;;) {
    const i = buf.indexOf('\r\n\r\n');
    if (i < 0) return;
    const m = /Content-Length: (\d+)/i.exec(buf.slice(0, i).toString());
    const n = Number(m[1]);
    if (buf.length < i + 4 + n) return;
    const msg = JSON.parse(buf.slice(i + 4, i + 4 + n).toString());
    buf = buf.slice(i + 4 + n);
    handle(msg);
  }
});
"#;

/// The served hover text of `f` declared at line 2 of `a.c` (`source`), when
/// the server answers `hover`.
fn served_hover(source: &str, hover: &str) -> (Value, String) {
    let dir = octocode_runtime_base::test_support::Fixture::new();
    let root = dir.0.canonicalize().expect("canonical root");
    std::fs::create_dir_all(root.join(".git")).expect("repository");
    std::fs::write(root.join("a.c"), source).expect("source");
    let script = root.join("fake-lsp.js");
    let config = format!(
        "const HOVER = {};",
        serde_json::to_string(hover).expect("hover literal")
    );
    write_executable(&script, SERVER.replace("/*CONFIG*/", &config).as_bytes());
    std::fs::write(
        root.join("lsp-servers.json"),
        json!({"languageServers": {".c": {
            "command": script.to_string_lossy(), "args": [], "languageId": "c"
        }}})
        .to_string(),
    )
    .expect("config");
    let execution = crate::lsp_search::LspExecutionConfig {
        octocode_home: Some(root.clone()),
        ..crate::lsp_search::LspExecutionConfig::with_path()
    };
    octocode_engine::stamp::settle_written_so_far();
    let pool = fake_server_pool();
    let paths = octocode_runtime_base::test_support::workspace_policy(&root);
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let row = runtime
        .block_on(crate::lsp_search::execute(
            query(json!({
                "operation": "hover",
                "path": root.join("a.c").to_string_lossy(),
                "symbolName": "f",
                "line": 2
            })),
            &NeverCancel,
            &pool,
            &paths,
            &execution,
            None,
            crate::lsp_search::test_answers(),
        ))
        .expect("row");
    runtime.block_on(pool.clear_all());
    let served = public_row(&row);
    let data = served
        .pointer("/results/0/data")
        .cloned()
        .unwrap_or_else(|| served.clone());
    let text = data
        .pointer("/hover/contents/value")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("a hover in {served}"))
        .to_owned();
    (data, text)
}

/// The KELVIN SIGN, which clangd's markdown escaping does not count as a word
/// character: an `_` before it is written `\_`.
const KELVIN: char = '\u{212A}';

/// A doc comment holding a labelled secret: the hover shows its key as the
/// server wrote it (clangd escapes the `_` as markdown, `secret\_Key`) and
/// only the value redacted, as a read of the file shows the line.
#[test]
fn a_hover_redacts_the_value_and_keeps_the_key_a_server_escaped() {
    if !octocode_runtime_base::test_support::require_node() {
        return;
    }
    let value = "Hv90128Qz7Wy";
    let source = format!("/** {{\"secret_{KELVIN}ey\":\"{value}\"}} */\nint f(void);\n");
    let hover = format!(
        "### function `f`  \n\n---\n→ `int`  \n{{\"secret\\_{KELVIN}ey\":\"{value}\"}}  \n\n---\n```cpp\nint f(void)\n```"
    );
    assert_read_hides(&source, value);
    let (row, text) = served_hover(&source, &hover);
    assert!(!row.to_string().contains(value), "{row}");
    assert!(
        text.contains(&format!("{{\"secret\\_{KELVIN}ey\":\"[REDACTED-")),
        "the key stays, the value is redacted: {text}"
    );
    assert_eq!(row["redacted"], true, "{row}");
}

/// What no judged file holds stays withheld, escaped or not: a literal the
/// server quotes from elsewhere, an escaped name the file does not spell,
/// and a secret value the server escaped.
#[test]
fn a_hover_withholds_what_no_judged_file_holds_escaped_or_not() {
    if !octocode_runtime_base::test_support::require_node() {
        return;
    }
    let secret = "Fake_Value_9876xyz";
    let source = format!("/** {{\"secret_key\":\"{secret}\"}} */\nint f(void);\n");
    assert_read_hides(&source, secret);
    let escaped = secret.replace('_', "\\_");
    let hover = format!(
        "### function `f`\n{{\"secret\\_key\":\"{escaped}\"}}\nconst p: \"Zq81ElsewhereOnly\" | \"other\\_name\""
    );
    let (row, text) = served_hover(&source, &hover);
    for hidden in ["Fake", "Value", "Zq81ElsewhereOnly", "other"] {
        assert!(!text.contains(hidden), "{hidden} is withheld: {text}");
    }
    assert!(
        text.contains("\"secret\\_key\""),
        "the file's key stays: {text}"
    );
    assert_eq!(row["redacted"], true, "{row}");
}

/// The case holds only if a read of `source` hides `value`.
fn assert_read_hides(source: &str, value: &str) {
    let judged = octocode_runtime_base::security::JudgedText::judge(
        source,
        std::path::Path::new("a.c"),
        &octocode_runtime_base::security::ContentSecurity::new(),
    );
    let read = judged.redaction().map_or(source, |(read, _)| read);
    assert!(!read.contains(value), "a read hides {value}: {read}");
}
