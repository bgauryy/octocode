//! The language-server prewarm after `initialize`, over real stdio.

use super::server::Server;
use crate::support::Workspace;
use serde_json::json;
use std::time::Duration;

/// A workspace with one TypeScript file whose language server is a script
/// that writes a marker file when it starts, and a `targeted` server over it.
fn marker_server() -> (Server, std::path::PathBuf, std::path::PathBuf, Workspace) {
    use std::os::unix::fs::PermissionsExt;
    let workspace = Workspace::new();
    let root = workspace.workspace.clone();
    std::fs::write(root.join("package.json"), "{}\n").unwrap();
    std::fs::write(root.join("a.ts"), "export function target() {}\n").unwrap();
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
    let mut server = Server::start_with(&workspace, &[("OCTOCODE_LSP_PREWARM", "targeted")]);
    server.request(
        "initialize",
        json!({"protocolVersion": "2025-11-25", "capabilities": {},
               "clientInfo": {"name": "test", "version": "1"}}),
    );
    server.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    (server, marker, root, workspace)
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

/// Targeted prewarm follows the response the agent receives: a lead the
/// response stage stripped (the matched symbol is already stated by the
/// request) has no visible cause, so it starts no language server.
#[test]
fn a_stripped_lead_starts_no_language_server() {
    let (mut server, marker, root, _workspace) = marker_server();
    let evidence = search(&mut server, &root, json!({"matchString": ["target"]}));
    let data = &evidence["results"][0]["data"];
    assert_eq!(data["files"][0]["path"], "a.ts", "{evidence}");
    assert!(!serves_lsp_lead(&evidence), "{evidence}");
    assert!(
        !started_within(&marker, Duration::from_secs(3)),
        "no lspSearch lead is served, so no server starts"
    );
    assert!(server.finish().success());
}

/// Positive control: a served lspSearch lead (the symbol it names is not
/// stated by the request, which names it after a declaration keyword) starts its server before any
/// lspSearch call.
#[test]
fn a_served_lead_starts_its_language_server_before_a_following_lookup() {
    let (mut server, marker, root, _workspace) = marker_server();
    let evidence = search(
        &mut server,
        &root,
        json!({"matchString": ["function target"]}),
    );
    assert!(serves_lsp_lead(&evidence), "{evidence}");
    assert!(
        started_within(&marker, Duration::from_secs(3)),
        "the served lead's server starts"
    );
    assert!(server.finish().success());
}

/// `lsp.prewarm` all starts the workspace's language server in the background
/// after `initialize`, before any call names a file. `targeted` waits for a
/// result that offers an lspSearch call: a session that never needs one starts
/// no whole-project index. `off` starts none.
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
            json!({"protocolVersion": "2025-11-25", "capabilities": {},
                   "clientInfo": {"name": "test", "version": "1"}}),
        );
        assert_eq!(init["result"]["serverInfo"]["name"], "octocode-mcp");
        server.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        let deadline = std::time::Instant::now() + Duration::from_secs(if starts { 30 } else { 3 });
        while !marker.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(marker.exists(), starts, "{prewarm}");
        assert_eq!(server.request("ping", json!({}))["result"], json!({}));
        assert!(server.finish().success(), "{prewarm}");
    }
}
