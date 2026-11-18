use super::live_check::run_live;
use super::status::run_status;
use super::upstream::{GitHub, Upstream};
use super::{ProvisionOutcome, provision_server, uninstall_server};
use octocode_engine::lsp::config::LspDiscoveryOptions;
use octocode_engine::lsp::managed::{manifest, platform_id, resolve_cached_server};
use octocode_native::runtime::{ErrorClass, ToolRuntime};
use octocode_runtime_base::config::LspAutoInstall;
use octocode_runtime_base::response::exit::ExitClass;
use std::path::Path;

/// The runtime's resolved language-server discovery settings, so status and
/// installs see what `lspSearch` sees.
pub(super) fn discovery(runtime: &ToolRuntime) -> LspDiscoveryOptions {
    runtime.lsp_execution_config().discovery()
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
    /// Check the language-server configuration: each `lsp-servers.json`, every route with its
    /// effective arguments, or one file's route. Exit 1 when a configuration has a problem.
    Status {
        /// File whose language decides the server.
        file: Option<String>,
        /// Also start the server (with no file, each one that resolves) and complete `initialize`.
        #[arg(long)]
        live: bool,
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
            let github = GitHub::new(runtime.github_com_token().await);
            run_install(&root, &platform, targets, mode, json, &github).await
        }
        LspServerCommand::Uninstall { names } => run_uninstall(&root, &platform, names, json),
        LspServerCommand::Clean { yes } => run_clean(&root, yes, json),
        LspServerCommand::Status { file, live } => {
            if live {
                run_live(&discovery, file.as_deref(), json).await
            } else {
                run_status(&discovery, &root, &platform, file.as_deref(), json)
            }
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

pub(super) async fn run_install(
    root: &Path,
    platform: &str,
    targets: Vec<String>,
    mode: LspAutoInstall,
    json: bool,
    upstream: &impl Upstream,
) -> u8 {
    if targets.is_empty() {
        eprintln!("Specify a server to install, or use --all.");
        return 2;
    }
    let mut results = Vec::new();
    let mut failed = Vec::new();
    for name in targets {
        let outcome = provision_server(root, &name, platform, mode, upstream).await;
        if outcome.ok {
            if !json {
                println!(
                    "{name}: {} -> {}",
                    outcome.source.unwrap_or("ok"),
                    outcome.path.clone().unwrap_or_default()
                );
                if let Some(verification) = &outcome.verification {
                    println!("  verified: {verification}");
                }
            }
        } else {
            failed.push(
                outcome
                    .failure
                    .as_ref()
                    .map_or(ErrorClass::Unservable, |failure| {
                        octocode_runtime_base::id::error_codes::class(failure.code)
                    }),
            );
            if !json {
                let error = outcome
                    .failure
                    .as_ref()
                    .map(|failure| failure.message.as_str());
                println!("{name}: {}", error.unwrap_or_default());
            }
        }
        results.push(install_item(&name, &outcome));
    }
    if json {
        super::super::write_json(&serde_json::json!({ "install": results }));
    }
    // The strictest class of the failed items: an unknown name is the
    // caller's to correct (2), an execution failure is not (5).
    octocode_runtime_base::response::exit::strictest(failed)
        .map_or(0, |class| super::super::exit_code(ExitClass::Failed(class)))
}

/// One `install` item as `--json` prints it. A failed item also states what
/// stopped it (`errorCode`) and that code's class (`errorClass`), as every
/// other `--json` failure does.
pub(super) fn install_item(name: &str, outcome: &ProvisionOutcome) -> serde_json::Value {
    let failure = outcome.failure.as_ref();
    let mut item = serde_json::json!({
        "name": name,
        "ok": outcome.ok,
        "source": outcome.source,
        "path": outcome.path,
        "error": failure.map(|failure| &failure.message),
        "verification": outcome.verification,
    });
    if let Some(failure) = failure {
        item["errorCode"] = serde_json::json!(failure.code);
        item["errorClass"] =
            serde_json::json!(octocode_runtime_base::id::error_codes::class(failure.code));
    }
    item
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
    // The clangd indexes kept for mirrored databases go with it.
    if let Some(indexes) = root.parent().map(|home| home.join("lsp-workspaces/clangd")) {
        let _ = std::fs::remove_dir_all(indexes);
    }
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
