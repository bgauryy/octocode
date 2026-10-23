use super::{fetch_allowlisted, provision_server, uninstall_server};
use octocode_engine::lsp::config::{
    LspDiscoveryOptions, default_server_for_file, detect_language_id, is_command_available,
};
use octocode_engine::lsp::managed::{manifest, platform_id, resolve_cached_server};
use octocode_native::config::LspAutoInstall;
use octocode_native::runtime::ToolRuntime;
use std::future::Future;
use std::path::{Path, PathBuf};

/// The runtime's resolved language-server discovery settings, so status and
/// installs see what `lspSearch` sees.
pub(super) fn discovery(runtime: &ToolRuntime) -> LspDiscoveryOptions {
    let execution = runtime.lsp_execution_config();
    execution.discovery(execution.config_path.as_deref().map(PathBuf::from))
}

/// Resolve how `file_path`'s language server would launch, through the same
/// discovery `lspSearch` uses. Without a file, list the managed installs.
pub(super) fn run_status(
    discovery: &LspDiscoveryOptions,
    root: &Path,
    platform: &str,
    file_path: Option<&str>,
    json: bool,
) -> u8 {
    let Some(path) = file_path else {
        let installed: Vec<serde_json::Value> = manifest()
            .keys()
            .filter_map(|name| {
                resolve_cached_server(root, name, platform)
                    .map(|bin| serde_json::json!({ "name": name, "path": bin }))
            })
            .collect();
        if json {
            return super::super::write_json(
                &serde_json::json!({ "managedRoot": root, "installed": installed }),
            );
        }
        println!("LSP status");
        println!("  managed installs: {}", root.display());
        for row in &installed {
            println!(
                "    {}  {}",
                row["name"].as_str().unwrap_or_default(),
                row["path"].as_str().unwrap_or_default()
            );
        }
        println!("  Pass a file path to see how its language server resolves.");
        return 0;
    };

    let workspace =
        octocode_engine::lsp::workspace::resolve_workspace_root_for_file(path.to_owned())
            .unwrap_or_else(|_| {
                std::env::current_dir()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| ".".to_owned())
            });
    let language_id = detect_language_id(path);
    let config = default_server_for_file(path, &workspace, discovery);
    let (server_available, probe_error) =
        match config.as_ref().map(|c| is_command_available(&c.command)) {
            Some(Ok(available)) => (available, None),
            Some(Err(error)) => (false, Some(error)),
            None => (false, None),
        };
    let server_command: Option<String> = config.map(|c| c.command);
    let lang = language_id.as_deref().unwrap_or("unknown");

    // A path has a separator or an extension that contains no separator;
    // anything else is probably a server name passed by mistake.
    let looks_like_path = path.contains('/') || path.contains('\\') || {
        path.rfind('.')
            .is_some_and(|i| i + 1 < path.len() && !path[i + 1..].contains('/'))
    };
    let unresolved = !server_available
        && language_id
            .as_deref()
            .is_none_or(|l| l.is_empty() || l == "plaintext");

    if json {
        return super::super::write_json(&serde_json::json!({
            "filePath": path,
            "languageId": lang,
            "serverAvailable": server_available,
            "serverCommand": server_command,
            "probeError": probe_error,
        }));
    }

    if unresolved && !looks_like_path {
        eprintln!(
            "  '{path}' looks like a server name, not a file. \
             status resolves a FILE's language \u{2192} server (e.g. src/main.rs); \
             run `lsp-server list` to see supported servers."
        );
    }

    println!("LSP status for {path}");
    println!("  language:  {lang}");
    if server_available {
        let cmd = server_command.as_deref().unwrap_or("unknown");
        println!("  resolved:  {cmd}");
    } else {
        println!("  resolved:  unavailable");
        if let Some(error) = &probe_error {
            println!("  Could not check the server command: {error}");
        }
        if let Some(lang_id) = &language_id {
            println!(
                "  No language server is available for this file (language: {lang_id}). \
                 Install a matching language server or add an lsp-servers.json entry."
            );
        } else {
            println!("  Could not determine language for this file.");
        }
    }
    0
}

/// `octocode lsp-server`: the managed (auto-downloadable) language servers.
#[derive(clap::Subcommand)]
pub(in super::super) enum LspServerCommand {
    /// List the auto-downloadable servers and whether each is installed.
    List,
    /// Download and install servers into the managed cache.
    Install {
        /// Server names (e.g. `rust-analyzer`, `clangd`).
        names: Vec<String>,
        /// Install every auto-downloadable server.
        #[arg(long)]
        all: bool,
        /// Download even when `lsp.autoInstall` is `prompt` or `off`.
        #[arg(long)]
        force: bool,
    },
    /// Remove installed servers from the managed cache.
    Uninstall {
        /// Server names.
        names: Vec<String>,
    },
    /// Delete the whole managed cache (a dry run without `--yes`).
    Clean {
        /// Confirm the deletion.
        #[arg(long)]
        yes: bool,
    },
    /// Show the server a file (default: the workspace) routes to, and whether it resolves.
    Status {
        /// File whose language decides the server.
        file: Option<String>,
    },
}

/// Dispatch the `lsp-server` subcommand.
pub(in super::super) async fn run(
    runtime: &ToolRuntime,
    command: LspServerCommand,
    json: bool,
) -> u8 {
    let discovery = discovery(runtime);
    let Some(root) = discovery.managed_root() else {
        eprintln!("The Octocode home is unavailable; cannot locate managed language servers.");
        return 5;
    };
    let platform = platform_id();
    match command {
        LspServerCommand::List => run_list(&root, &platform, json),
        LspServerCommand::Install { names, all, force } => {
            let targets: Vec<String> = if all {
                manifest().keys().map(|k| (*k).to_owned()).collect()
            } else {
                names
            };
            let mode = if force {
                LspAutoInstall::Auto
            } else {
                runtime.config().resolved.lsp.auto_install
            };
            run_install(&root, &platform, targets, mode, json, fetch_allowlisted).await
        }
        LspServerCommand::Uninstall { names } => run_uninstall(&root, &platform, names, json),
        LspServerCommand::Clean { yes } => run_clean(&root, yes, json),
        LspServerCommand::Status { file } => {
            run_status(&discovery, &root, &platform, file.as_deref(), json)
        }
    }
}

pub(super) fn run_list(root: &Path, platform: &str, json: bool) -> u8 {
    let servers = manifest();
    if json {
        let rows: Vec<serde_json::Value> = servers
            .iter()
            .map(|(name, server)| {
                let installed = resolve_cached_server(root, name, platform).is_some();
                serde_json::json!({
                    "name": name,
                    "languageId": server.language_id,
                    "releaseTag": server.release_tag,
                    "repo": server.repo,
                    "status": if installed { "installed" } else { "not installed" },
                })
            })
            .collect();
        return super::super::write_json(&serde_json::json!({ "servers": rows }));
    }
    println!("Auto-downloadable language servers");
    for (name, server) in servers {
        let installed = resolve_cached_server(root, name, platform).is_some();
        let status = if installed {
            "installed (managed cache)"
        } else {
            "not installed"
        };
        println!("  {:<12} {:<16} {status}", server.language_id, name);
    }
    0
}

pub(super) async fn run_install<F, Fut>(
    root: &Path,
    platform: &str,
    targets: Vec<String>,
    mode: LspAutoInstall,
    json: bool,
    fetch: F,
) -> u8
where
    F: Fn(&'static str, tokio::fs::File) -> Fut,
    Fut: Future<Output = Result<tokio::fs::File, String>>,
{
    if targets.is_empty() {
        eprintln!("Specify a server to install, or use --all.");
        return 2;
    }
    let mut results = Vec::new();
    let mut worst: u8 = 0;
    for name in targets {
        let outcome = provision_server(root, &name, platform, mode, &fetch).await;
        if outcome.ok {
            if !json {
                println!(
                    "{name}: {} -> {}",
                    outcome.source.unwrap_or("ok"),
                    outcome.path.clone().unwrap_or_default()
                );
            }
        } else {
            worst = 3;
            if !json {
                println!("{name}: {}", outcome.error.clone().unwrap_or_default());
            }
        }
        results.push(serde_json::json!({
            "name": name,
            "ok": outcome.ok,
            "source": outcome.source,
            "path": outcome.path,
            "error": outcome.error,
        }));
    }
    if json {
        super::super::write_json(&serde_json::json!({ "install": results }));
    }
    worst
}

pub(super) fn run_uninstall(root: &Path, platform: &str, names: Vec<String>, json: bool) -> u8 {
    if names.is_empty() {
        eprintln!("Specify a server to uninstall.");
        return 2;
    }
    let mut results = Vec::new();
    for name in &names {
        let removed = uninstall_server(root, name, platform);
        if !json {
            if removed {
                println!("{name}: removed from managed cache");
            } else {
                println!("{name}: not in managed cache (nothing to remove)");
            }
        }
        results.push(serde_json::json!({ "name": name, "removed": removed }));
    }
    if json {
        super::super::write_json(&serde_json::json!({ "uninstall": results }));
    }
    0
}

pub(super) fn run_clean(root: &Path, yes: bool, json: bool) -> u8 {
    let root_display = root.to_string_lossy().into_owned();
    if !yes {
        if json {
            super::super::write_json(
                &serde_json::json!({ "clean": "dry-run", "root": root_display }),
            );
        } else {
            println!(
                "Would remove the managed LSP cache at {root_display}. Re-run with --yes to confirm."
            );
        }
        return 0;
    }
    // A missing cache is already clean; any other failure must not be
    // reported as success.
    if let Err(error) = std::fs::remove_dir_all(root)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        if json {
            super::super::write_json(
                &serde_json::json!({ "clean": "failed", "root": root_display, "error": error.to_string() }),
            );
        } else {
            eprintln!("Could not remove the managed LSP cache at {root_display}: {error}");
        }
        return 5;
    }
    if json {
        super::super::write_json(&serde_json::json!({ "clean": "done", "root": root_display }));
    } else {
        println!("cleaned: {root_display}");
    }
    0
}
