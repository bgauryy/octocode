//! The language-server prewarm after `initialize`, over real stdio.

use super::server::Server;
use crate::support::Workspace;
use serde_json::json;
use std::time::Duration;

/// An offered lspSearch lead is not a request for a language server: under
/// `targeted`, a search whose result offers one starts no whole-project index.
#[test]
fn targeted_prewarm_ignores_an_offered_lead_until_lsp_search_runs() {
    use std::os::unix::fs::PermissionsExt;
    let workspace = Workspace::new();
    let root = &workspace.workspace;
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
    let config = root.join("lsp-servers.json");
    std::fs::write(
        &config,
        json!({"languageServers": {".ts": {
            "command": script.to_string_lossy(), "args": [], "languageId": "typescript"
        }}})
        .to_string(),
    )
    .unwrap();
    let config = config.to_string_lossy().into_owned();
    let mut server = Server::start_with(
        &workspace,
        &[
            ("OCTOCODE_LSP_CONFIG", config.as_str()),
            ("OCTOCODE_LSP_PREWARM", "targeted"),
        ],
    );
    server.request(
        "initialize",
        json!({"protocolVersion": "2025-11-25", "capabilities": {},
               "clientInfo": {"name": "test", "version": "1"}}),
    );
    server.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let found = server.request(
        "tools/call",
        json!({"name": "localSearch", "arguments": {"queries": [{
            "path": root.to_string_lossy(), "matchString": ["target"]
        }]}}),
    );
    let offered = found["result"].to_string();
    assert!(
        offered.contains("lspSearch"),
        "the search offers a lead: {offered}"
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !marker.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!marker.exists(), "no lspSearch ran, so no server starts");
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
        let config = root.join("lsp-servers.json");
        std::fs::write(
            &config,
            json!({"languageServers": {".ts": {
                "command": script.to_string_lossy(), "args": [], "languageId": "typescript"
            }}})
            .to_string(),
        )
        .unwrap();
        let config = config.to_string_lossy().into_owned();
        let mut server = Server::start_with(
            &workspace,
            &[
                ("OCTOCODE_LSP_CONFIG", config.as_str()),
                ("OCTOCODE_LSP_PREWARM", prewarm),
            ],
        );
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
