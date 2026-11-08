use super::*;
use crate::test_support::TmpDir;
#[cfg(any(unix, windows))]
use std::path::PathBuf;

mod call_scope;
mod document_sync;
mod memory_cap;
mod open_document_set;
mod open_readiness;
mod parse_waits;
#[cfg(any(unix, windows))]
mod process_environment;
mod process_lifecycle;
#[cfg(unix)]
mod process_readiness;
mod project_loads;
mod protocol;
mod provider_answers;
mod readiness;
mod request_outcomes;
mod snippet_reads;
mod stderr_ring;
mod waits;

fn ts_config() -> LspLanguageServerConfig {
    LspLanguageServerConfig {
        command: "typescript-language-server".into(),
        args: Some(vec!["--stdio".into()]),
        workspace_root: std::env::temp_dir().to_string_lossy().into_owned(),
        language_id: Some("typescript".into()),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    }
}

/// Minimal stdio LSP server (node). `FAKE_INIT_DELAY_MS` delays the
/// `initialize` reply; `FAKE_ALLOC_MB` allocates (and touches) that much
/// memory after `initialized`; `FAKE_INDEX_MS` reports one `$/progress`
/// indexing wave of that length after `initialized`; `FAKE_PID_FILE` records
/// the server pid; `FAKE_CHILD_PID_FILE` starts a child after `initialized`
/// and records its pid; `FAKE_DAEMON_PID_FILE` starts one in its own session
/// (as jdtls starts a Gradle daemon) and records its pid.
#[cfg(any(unix, windows))]
const CONFIGURABLE_SERVER: &str = r#"#!/usr/bin/env node
const fs = require('fs');
if (process.env.FAKE_PID_FILE) fs.writeFileSync(process.env.FAKE_PID_FILE, String(process.pid));
const hold = [];
let buf = Buffer.alloc(0);
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
function handle(msg) {
  if (msg.method === 'initialize') {
    const reply = () => send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { hoverProvider: true, textDocumentSync: 1 } } });
    setTimeout(reply, Number(process.env.FAKE_INIT_DELAY_MS || 0));
  } else if (msg.method === 'initialized') {
    if (process.env.FAKE_CHILD_PID_FILE) {
      const child = require('child_process').spawn(process.execPath, ['-e', 'setTimeout(() => {}, 60000)'], { stdio: 'ignore' });
      fs.writeFileSync(process.env.FAKE_CHILD_PID_FILE, String(child.pid));
    }
    if (process.env.FAKE_DAEMON_PID_FILE) {
      const daemon = require('child_process').spawn(process.execPath, ['-e', 'setTimeout(() => {}, 60000)'], { stdio: 'ignore', detached: true });
      daemon.unref();
      const file = process.env.FAKE_DAEMON_PID_FILE;
      fs.writeFileSync(file + '.tmp', String(daemon.pid));
      fs.renameSync(file + '.tmp', file);
    }
    const mb = Number(process.env.FAKE_ALLOC_MB || 0);
    for (let i = 0; i < mb; i++) hold.push(Buffer.alloc(1 << 20, 1));
    const indexMs = Number(process.env.FAKE_INDEX_MS || 0);
    if (indexMs > 0) {
      send({ jsonrpc: '2.0', method: '$/progress', params: { token: 'index', value: { kind: 'begin', title: 'Indexing' } } });
      setTimeout(() => send({ jsonrpc: '2.0', method: '$/progress', params: { token: 'index', value: { kind: 'end' } } }), indexMs);
    }
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

/// Write an executable fixture (a stand-in language server) that a test then
/// starts. The bytes go to a temporary file that is closed and renamed into
/// place, so no write descriptor is open on the file when it is executed: a
/// concurrent fork that inherited one would make the start fail with
/// `ETXTBSY` ("Text file busy"). Windows runs a script through node (its
/// shebang names node) and needs no mode.
#[cfg(any(unix, windows))]
fn write_executable(path: &std::path::Path, contents: impl AsRef<[u8]>) {
    octocode_io::atomic::write_atomic_mode(path, contents.as_ref(), 0o700, false)
        .expect("write an executable fixture");
}

#[cfg(any(unix, windows))]
fn node_available() -> bool {
    std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// A temp dir holding [`CONFIGURABLE_SERVER`] and a client config for it.
#[cfg(any(unix, windows))]
fn configurable_server(
    tag: &str,
    env: &[(&str, String)],
    max_memory_mb: Option<u32>,
) -> (TmpDir, PathBuf, LspLanguageServerConfig) {
    let tmp = TmpDir::new();
    let root = tmp.0.join(tag);
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let script = root.join("fake-lsp.js");
    write_executable(&script, CONFIGURABLE_SERVER);
    let config = LspLanguageServerConfig {
        command: script.to_string_lossy().into_owned(),
        args: Some(Vec::new()),
        workspace_root: root.to_string_lossy().into_owned(),
        language_id: Some("plaintext".into()),
        initialization_options: None,
        env: Some(
            env.iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect(),
        ),
        max_memory_mb,
        source: Default::default(),
    };
    (tmp, root, config)
}
