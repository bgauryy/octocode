//! `octocode lsp-server status --live`: start servers and complete `initialize`.
use super::status::{absolute_path, routes};
use octocode_engine::lsp::client::NativeLspClient;
use octocode_engine::lsp::config::{
    LspDiscoveryOptions, default_server_for_file, server_command_resolves,
};

/// Start the server of `file` (or, with none, of every route that resolves, in
/// an empty directory) with its effective arguments and complete `initialize`:
/// whether the flags and options really start this installed version, and which
/// operations it then offers. Exit 1 when one fails to start.
pub(super) async fn run_live(
    discovery: &LspDiscoveryOptions,
    file: Option<&str>,
    json: bool,
) -> u8 {
    let (report, failed) = match live_report(discovery, file).await {
        Ok(checked) => checked,
        Err(code) => return code,
    };
    if json {
        return super::super::write_json(&report) | u8::from(failed);
    }
    let rows = report["live"].as_array().cloned().unwrap_or_default();
    println!("LSP live check");
    for row in &rows {
        if row["ok"] == true {
            println!(
                "  ok    {} {} ({} ms): {}",
                row["target"].as_str().unwrap_or_default(),
                row["command"].as_str().unwrap_or_default(),
                row["ms"],
                row["capabilities"]
            );
        } else {
            println!(
                "  FAIL  {} {}: {}",
                row["target"].as_str().unwrap_or_default(),
                row["command"].as_str().unwrap_or_default(),
                row["error"].as_str().unwrap_or_default()
            );
        }
    }
    if rows.is_empty() {
        println!("  no server resolves");
    }
    u8::from(failed)
}

/// The live check's `{"live": rows}` report, through the output gate (a
/// failed start's error carries what the server wrote to stderr, which may
/// hold a secret), and whether a server failed to start. `Err` is the exit
/// code of a refused check.
pub(super) async fn live_report(
    discovery: &LspDiscoveryOptions,
    file: Option<&str>,
) -> Result<(serde_json::Value, bool), u8> {
    let scratch = tempfile::tempdir().ok();
    let mut targets = Vec::new();
    if let Some(path) = file {
        let absolute = absolute_path(path);
        let workspace = octocode_engine::lsp::config::workspace_root_for_file(&absolute, discovery)
            .unwrap_or_else(|_| ".".to_owned());
        if discovery.opens_fetched_code(&absolute, &workspace) {
            eprintln!("{}", octocode_engine::lsp::config::FETCHED_CODE_REFUSAL);
            return Err(1);
        }
        targets.extend(
            default_server_for_file(&absolute, &workspace, discovery)
                .map(|config| (path.to_owned(), config)),
        );
    } else if let Some(dir) = &scratch {
        let mut seen = std::collections::HashSet::new();
        for (extension, config) in routes(&dir.path().to_string_lossy(), discovery) {
            if let Some(config) = config.filter(server_command_resolves)
                && seen.insert((config.command.clone(), config.args.clone()))
            {
                targets.push((extension, config));
            }
        }
    }
    let mut failed = false;
    let mut rows = Vec::new();
    for (target, config) in targets {
        let started = std::time::Instant::now();
        let client = NativeLspClient::new(config.clone());
        let outcome = client.start().await;
        let elapsed = started.elapsed().as_millis();
        let row = match outcome {
            Ok(()) => {
                let offered = LIVE_CAPABILITIES
                    .iter()
                    .filter(|capability| client.has_capability((**capability).to_owned()))
                    .copied()
                    .collect::<Vec<_>>();
                let _ = client.stop().await;
                serde_json::json!({
                    "target": target, "command": config.command, "ok": true,
                    "ms": elapsed, "capabilities": offered,
                })
            }
            Err(error) => {
                failed = true;
                serde_json::json!({
                    "target": target, "command": config.command, "ok": false,
                    "ms": elapsed, "error": error.to_string(),
                })
            }
        };
        rows.push(row);
    }
    let report =
        octocode_native::security::Sanitized::scrubbed(serde_json::json!({ "live": rows }))
            .into_value();
    Ok((report, failed))
}

/// The capabilities `lspSearch` operations need.
const LIVE_CAPABILITIES: [&str; 7] = [
    "definitionProvider",
    "referencesProvider",
    "callHierarchyProvider",
    "documentSymbolProvider",
    "workspaceSymbolProvider",
    "implementationProvider",
    "typeHierarchyProvider",
];
