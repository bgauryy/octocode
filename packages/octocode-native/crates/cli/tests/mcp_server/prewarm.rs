//! The language-server prewarm after `initialize`, over real stdio.

use super::server::Server;
use crate::support::Workspace;
use octocode_test_support::json_fixture;
use serde_json::json;
use std::time::Duration;

/// A language server script that writes `marker` `start_delay` seconds after
/// it starts. The delay stands in for a host too loaded to start it at once.
fn marker_script(
    root: &std::path::Path,
    name: &str,
    start_delay: u32,
) -> (std::path::PathBuf, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let marker = root.join(format!("{name}-started"));
    let script = root.join(format!("{name}-lsp.sh"));
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nsleep {start_delay}\necho started > '{}'\nexec cat > /dev/null\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    (script, marker)
}

/// The start markers of [`marker_server`]'s two languages.
struct Markers {
    ts: std::path::PathBuf,
    py: std::path::PathBuf,
}

/// A workspace with one TypeScript file (`target`) and one Python file
/// (`other`), each language served by a [`marker_script`], and a server under
/// `prewarm` over it.
fn marker_server(
    prewarm: &str,
    start_delay: u32,
) -> (Server, Markers, std::path::PathBuf, Workspace) {
    let workspace = Workspace::new();
    let root = workspace.workspace.clone();
    std::fs::write(root.join("package.json"), "{}\n").unwrap();
    std::fs::write(root.join("pyproject.toml"), "[project]\nname = \"b\"\n").unwrap();
    std::fs::write(root.join("a.ts"), "export function target() {}\n").unwrap();
    std::fs::write(root.join("b.py"), "def other():\n    pass\n").unwrap();
    let (ts_script, ts) = marker_script(&root, "ts", start_delay);
    let (py_script, py) = marker_script(&root, "py", start_delay);
    let config = workspace.home.join("lsp-servers.json");
    std::fs::write(
        &config,
        json!({"languageServers": {
            ".ts": {"command": ts_script.to_string_lossy(), "args": [], "languageId": "typescript"},
            ".py": {"command": py_script.to_string_lossy(), "args": [], "languageId": "python"}
        }})
        .to_string(),
    )
    .unwrap();
    let mut server = Server::start_with(&workspace, &[("OCTOCODE_LSP_PREWARM", prewarm)]);
    server.request(
        "initialize",
        json_fixture(
            r#"{
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1"}
            }"#,
        ),
    );
    server.send(&json_fixture(
        r#"{"jsonrpc": "2.0", "method": "notifications/initialized"}"#,
    ));
    (server, Markers { ts, py }, root, workspace)
}

/// One localSearch over `root`, answered as its evidence JSON.
fn search(
    server: &mut Server,
    root: &std::path::Path,
    query: serde_json::Value,
) -> serde_json::Value {
    let mut query = query;
    query["path"] = json!(root.to_string_lossy());
    let found = server.request(
        "tools/call",
        json!({"name": "localSearch", "arguments": {"queries": [query]}}),
    );
    serde_json::from_str(
        found["result"]["content"][0]["text"]
            .as_str()
            .expect("tool text"),
    )
    .expect("tool JSON")
}

/// Waits for the server's start marker, however long a loaded host takes to
/// start it: the test runner's per-test deadline bounds a start that never
/// comes.
fn started(marker: &std::path::Path) {
    while !marker.exists() {
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Whether the marker appears within `wait`: a negative check, where any
/// window can only miss a late start, never fail on a slow one.
fn started_within(marker: &std::path::Path, wait: Duration) -> bool {
    let deadline = std::time::Instant::now() + wait;
    while !marker.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    marker.exists()
}

fn serves_lsp_lead(evidence: &serde_json::Value) -> bool {
    evidence["results"][0]["data"]["related"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|call| call["tool"] == "lspSearch")
}

/// Targeted prewarm warms the language of the file a search hit names, as an
/// editor starts a server when a file of its language opens: the TypeScript
/// server starts before any lspSearch call, even with no lspSearch lead in the
/// response, and the Python server, which no row names, does not. A later hit
/// in the Python file starts its server: both are configured.
#[test]
fn a_search_hit_starts_its_files_language_server_and_no_other() {
    let (mut server, markers, root, _workspace) = marker_server("targeted", 4);
    let evidence = search(
        &mut server,
        &root,
        json_fixture(r#"{"matchString": ["target"]}"#),
    );
    assert_eq!(
        evidence["results"][0]["data"]["files"][0]["path"], "a.ts",
        "{evidence}"
    );
    assert!(!serves_lsp_lead(&evidence), "{evidence}");
    started(&markers.ts);
    assert!(
        !started_within(&markers.py, Duration::from_secs(3)),
        "no row names a Python file, so its server stays down"
    );
    let evidence = search(
        &mut server,
        &root,
        json_fixture(r#"{"matchString": ["other"]}"#),
    );
    assert_eq!(
        evidence["results"][0]["data"]["files"][0]["path"], "b.py",
        "{evidence}"
    );
    started(&markers.py);
    assert!(server.finish().success());
}

/// `off` starts no server for a search hit.
#[test]
fn prewarm_off_starts_no_language_server_for_a_search_hit() {
    let (mut server, markers, root, _workspace) = marker_server("off", 0);
    let evidence = search(
        &mut server,
        &root,
        json_fixture(r#"{"matchString": ["target"]}"#),
    );
    assert_eq!(
        evidence["results"][0]["data"]["files"][0]["path"], "a.ts",
        "{evidence}"
    );
    assert!(
        !started_within(&markers.ts, Duration::from_secs(3)),
        "prewarm off starts no server"
    );
    assert!(server.finish().success());
}

/// `lsp.prewarm` all starts the workspace's language server in the background
/// after `initialize`, before any call names a file. `targeted` waits for a
/// result that names a file or offers an lspSearch call: a session that never
/// reads source starts no whole-project index. `off` starts none.
#[test]
fn only_prewarm_all_starts_the_workspace_language_server_at_initialize() {
    use std::os::unix::fs::PermissionsExt;
    for (prewarm, starts) in [("all", true), ("targeted", false), ("off", false)] {
        let workspace = Workspace::new();
        let root = &workspace.workspace;
        std::fs::write(root.join("package.json"), "{}\n").unwrap();
        std::fs::write(root.join("a.ts"), "export const a = 1;\n").unwrap();
        let marker = root.join("server-started");
        let script = root.join("fake-lsp.sh");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\necho started > '{}'\nexec cat > /dev/null\n",
                marker.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let config = workspace.home.join("lsp-servers.json");
        std::fs::write(
            &config,
            json!({"languageServers": {".ts": {
                "command": script.to_string_lossy(), "args": [], "languageId": "typescript"
            }}})
            .to_string(),
        )
        .unwrap();
        let mut server = Server::start_with(&workspace, &[("OCTOCODE_LSP_PREWARM", prewarm)]);
        let init = server.request(
            "initialize",
            json_fixture(
                r#"{
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": {"name": "test", "version": "1"}
                }"#,
            ),
        );
        assert_eq!(init["result"]["serverInfo"]["name"], "octocode-mcp");
        server.send(&json_fixture(
            r#"{"jsonrpc": "2.0", "method": "notifications/initialized"}"#,
        ));
        let deadline = std::time::Instant::now() + Duration::from_secs(if starts { 30 } else { 3 });
        while !marker.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(marker.exists(), starts, "{prewarm}");
        assert_eq!(server.request("ping", json!({}))["result"], json!({}));
        assert!(server.finish().success(), "{prewarm}");
    }
}
