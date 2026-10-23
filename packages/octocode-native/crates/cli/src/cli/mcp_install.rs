//! MCP install for all supported clients: the server runs from npm through
//! `--method` (`npx -y octocode-mcp@latest` by default, or bunx/pnpm), never
//! `octo mcp`. JSON clients use their native server map; codex writes
//! `[mcp_servers.octocode]` TOML.
//! `--scope` picks the config file the way the config view does
//! ([`mcp_manage::target`]); `--list` reports each client's entry and
//! `--remove` deletes it.
mod apply;
mod clients;
mod dry_run;
use super::mcp_clients::{self, CLIENTS};
use super::mcp_manage::{self, Method, ServerSpec};
pub(super) use apply::valid_server;
use apply::{install, rollback_destination};
use clients::{client_status, list_clients, select_client, unknown_client_message};
#[cfg(test)]
use dry_run::render_fixture;
use dry_run::{Existing, already_installed, command_line, dry_run_text};
use octocode_contract_types::ErrorCode;
use serde_json::json;
use std::io::{self, IsTerminal};
use std::path::PathBuf;

/// The env name bound to the `local.enabled` config key (`OCTOCODE_ENABLE_LOCAL`).
fn local_enabled_env() -> Option<&'static str> {
    octocode_native::config::field::LOCAL_ENABLED.env_name()
}

/// The GitHub token env var `--pass-env` forwards; one of the config
/// contract's `ENV_TOKEN_VARS` (a unit test pins that).
const PASS_ENV_TOKEN: &str = octocode_native::config::environment::GITHUB_TOKEN;

fn server_spec(args: &InstallArgs) -> ServerSpec {
    let mut spec = ServerSpec::new(args.method);
    if args.pass_env {
        for key in local_enabled_env().into_iter().chain([PASS_ENV_TOKEN]) {
            if let Ok(value) = std::env::var(key)
                && !value.is_empty()
            {
                spec.env.insert(key.to_owned(), value);
            }
        }
    }
    if let (Some(enabled), Some(key)) = (args.enable_local, local_enabled_env()) {
        spec.env.insert(
            key.to_owned(),
            if enabled { "true" } else { "false" }.to_owned(),
        );
    }
    spec
}

/// Which of a client's config files an entry goes in.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, clap::ValueEnum)]
pub(super) enum Scope {
    /// The client's user config: every project.
    #[default]
    Home,
    /// The project config file under the current directory (e.g. `.cursor/mcp.json`).
    Workspace,
    /// Claude Code only: this directory's section of the user config.
    Local,
}

impl Scope {
    const fn name(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Workspace => "workspace",
            Self::Local => "local",
        }
    }
}

/// `octocode install`: add the Octocode MCP server to an agent client's configuration.
#[derive(clap::Args, Debug, Default)]
pub struct InstallArgs {
    /// Client id from `--list`, e.g. `claude-code`, `cursor`, `claude-desktop`.
    #[arg(long, value_name = "ID")]
    pub ide: Option<String>,
    /// Config file to edit; `--list` shows which clients support each scope.
    #[arg(long, value_enum, default_value_t)]
    pub scope: Scope,
    /// Remove the Octocode entry; an entry install did not write needs --force.
    #[arg(long, conflicts_with_all = ["check", "enable_local", "pass_env", "method"])]
    pub remove: bool,
    /// Overwrite (or, with --remove, delete) an entry that differs from the one install writes.
    #[arg(long)]
    pub force: bool,
    /// Preview the config that would be written without making any changes.
    #[arg(long)]
    pub dry_run: bool,
    /// Verify that the MCP config already contains a valid Octocode entry.
    #[arg(long)]
    pub check: bool,
    /// List every client with its Octocode entry status and config path at --scope.
    #[arg(long)]
    pub list: bool,
    /// Print JSON: the global `--json`.
    #[arg(skip)]
    pub json: bool,
    /// Override whether local (filesystem) tools are enabled in the MCP server.
    #[arg(long)]
    pub enable_local: Option<bool>,
    /// Copy GITHUB_TOKEN and OCTOCODE_ENABLE_LOCAL from this shell into the entry's env;
    /// the client config then stores the token in plain text.
    #[arg(long)]
    pub pass_env: bool,
    /// Installation runner.
    #[arg(long, value_enum, default_value_t)]
    pub method: Method,
    /// Restore a client config from the `.bak` file an earlier install left beside it.
    #[arg(long, value_name = "FILE")]
    pub rollback: Option<String>,
}

pub fn run(mut args: InstallArgs) -> u8 {
    // Rollback mode: restore the backup an earlier install left beside the config
    if let Some(ref bak_str) = args.rollback {
        let bak = PathBuf::from(bak_str);
        if !bak.exists() {
            eprintln!("rollback: backup file not found: {}", bak.display());
            return 1;
        }
        if bak.extension().and_then(|value| value.to_str()) != Some("bak") {
            eprintln!("rollback: expected a .bak backup file");
            return 2;
        }
        let dest = rollback_destination(&bak);
        if args.dry_run {
            if args.json {
                return super::write_json(
                    &json!({"success": true, "dryRun": true, "restored": dest, "from": bak}),
                );
            }
            println!("Would restore {} from {}", dest.display(), bak.display());
            return 0;
        }
        let restore = (|| {
            use octocode_native::config::{
                config_revision, read_private_config, replace_private_config,
            };
            let backup = read_private_config(&bak)?
                .ok_or_else(|| std::io::Error::other("Backup is missing."))?;
            let current = read_private_config(&dest)?;
            replace_private_config(&dest, &config_revision(current.as_deref()), &backup, false)
                .map(|(changed, _)| changed)
        })();
        return match restore {
            Ok(_) if args.json => {
                super::write_json(&json!({"success": true, "restored": dest, "from": bak}))
            }
            Ok(_) => {
                println!("Restored {} from {}", dest.display(), bak.display());
                0
            }
            Err(e) => {
                eprintln!("rollback: {e}");
                1
            }
        };
    }
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            super::emit_error(
                ErrorCode::ExecutionFailed,
                &format!("Cannot read the current directory: {error}"),
                args.json,
            );
            return 5;
        }
    };
    let statuses = || {
        CLIENTS
            .iter()
            .map(|client| client_status(client, args.scope, &cwd))
            .collect::<Vec<_>>()
    };
    if args.list {
        return list_clients(args.scope, &statuses(), args.json);
    }
    if args.ide.is_none() && !args.json && io::stdin().is_terminal() && io::stdout().is_terminal() {
        let labels = statuses()
            .iter()
            .map(|status| format!("{} ({})", status.id, status.status))
            .collect::<Vec<_>>();
        match select_client(&labels, &mut io::stdin().lock(), &mut io::stderr().lock()) {
            Ok(Some(id)) => args.ide = Some(id.to_owned()),
            Ok(None) => return 0,
            Err(error) => {
                super::emit_error(
                    ErrorCode::ExecutionFailed,
                    &format!("Cannot select a client: {error}"),
                    false,
                );
                return 5;
            }
        }
    }
    let Some(requested_ide) = args.ide.as_deref().filter(|value| !value.is_empty()) else {
        super::emit_error(
            ErrorCode::InvalidInput,
            "Usage: octocode install --ide <id> [--scope home|workspace|local] [--check|--remove] [--force] [--dry-run] [--json]; clients: octocode install --list",
            args.json,
        );
        return 2;
    };
    let Some(client) = mcp_clients::client(requested_ide) else {
        super::emit_error(
            ErrorCode::InvalidInput,
            &unknown_client_message(requested_ide),
            args.json,
        );
        return 2;
    };
    if !client.scopes().any(|scope| scope == args.scope.name()) {
        super::emit_error(
            ErrorCode::InvalidInput,
            &format!(
                "{} has no {} scope; it supports --scope {}",
                client.id,
                args.scope.name(),
                client.scopes().collect::<Vec<_>>().join(", ")
            ),
            args.json,
        );
        return 2;
    }
    let target = match mcp_manage::target(client.id, args.scope.name(), &cwd) {
        Ok(target) => target,
        Err(message) => {
            eprintln!("{message}");
            return 1;
        }
    };
    match install(&target, &args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

#[cfg(test)]
mod tests;
