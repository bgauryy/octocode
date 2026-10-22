//! MCP install for all supported clients: the server runs from npm through
//! `--method` (`npx -y octocode-mcp@latest` by default, or bunx/pnpm), never
//! `octo mcp`. JSON clients use their native server map; codex writes
//! `[mcp_servers.octocode]` TOML; goose writes `extensions.octocode` YAML.
//! `--scope` picks the config file the way the config view does
//! ([`mcp_manage::target`]); `--list` reports each client's entry and
//! `--remove` deletes it.
use super::mcp_clients::{self, CLIENTS, ClientSpec, EntryShape};
use super::mcp_manage::{self, Method, ServerSpec, Target};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

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

/// Ask for a client by number; `labels` describe [`CLIENTS`] in order.
fn select_client(
    labels: &[String],
    reader: &mut impl BufRead,
    output: &mut impl Write,
) -> io::Result<Option<&'static str>> {
    writeln!(output, "Select an MCP client:")?;
    for (index, label) in labels.iter().enumerate() {
        writeln!(output, "  {}. {label}", index + 1)?;
    }
    loop {
        write!(output, "Client number (Enter to cancel): ")?;
        output.flush()?;
        let mut line = Vec::new();
        std::io::Read::take(&mut *reader, 1025).read_until(b'\n', &mut line)?;
        if line.len() > 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Selection is too long.",
            ));
        }
        let selection = std::str::from_utf8(&line)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?
            .trim();
        if selection.is_empty() {
            return Ok(None);
        }
        if let Some(client) = selection
            .parse::<usize>()
            .ok()
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| CLIENTS.get(index))
        {
            return Ok(Some(client.id));
        }
        writeln!(output, "Enter a number from 1 to {}.", CLIENTS.len())?;
    }
}

/// One client's Octocode entry at a scope, as `--list` and the picker show it.
struct ClientStatus {
    id: &'static str,
    /// `installed` (a valid entry), `disabled`, `custom` (an `octocode`
    /// entry install did not write), `absent`, `invalid` (unreadable or
    /// unparsable config) or `unsupported` (no such scope).
    status: &'static str,
    /// The scopes the client reads.
    scopes: Vec<&'static str>,
    path: Option<PathBuf>,
    error: Option<String>,
}

fn client_status(client: &'static ClientSpec, scope: Scope, cwd: &Path) -> ClientStatus {
    let scopes = || client.scopes().collect::<Vec<_>>();
    let unsupported = |error: Option<String>| ClientStatus {
        id: client.id,
        status: if error.is_some() {
            "invalid"
        } else {
            "unsupported"
        },
        scopes: scopes(),
        path: None,
        error,
    };
    if !client.scopes().any(|name| name == scope.name()) {
        return unsupported(None);
    }
    let target = match mcp_manage::target(client.id, scope.name(), cwd) {
        Ok(target) => target,
        Err(error) => return unsupported(Some(error)),
    };
    let inspected = mcp_manage::inspect(&target);
    let status = match inspected["status"].as_str() {
        Some("configured") => {
            let valid = octocode_native::config::read_private_config(&target.path)
                .ok()
                .flatten()
                .and_then(|text| mcp_manage::parse_document(client.format, &text).ok())
                .is_some_and(|root| valid_server(client, &root, &target.keys));
            if valid { "installed" } else { "custom" }
        }
        Some("disabled") => "disabled",
        Some("absent") => "absent",
        _ => "invalid",
    };
    ClientStatus {
        id: client.id,
        status,
        scopes: scopes(),
        path: Some(target.path),
        error: inspected["error"].as_str().map(str::to_owned),
    }
}

fn list_clients(scope: Scope, statuses: &[ClientStatus], json_out: bool) -> u8 {
    if json_out {
        let ides = statuses
            .iter()
            .map(|row| {
                let mut value = json!({"id": row.id, "status": row.status, "scopes": row.scopes, "configPath": row.path});
                if let Some(error) = &row.error {
                    value["error"] = json!(error);
                }
                value
            })
            .collect::<Vec<_>>();
        return super::write_json(&json!({"scope": scope.name(), "ides": ides}));
    }
    let width = statuses.iter().map(|row| row.id.len()).max().unwrap_or(0);
    println!("MCP clients (--scope {}):", scope.name());
    for row in statuses {
        let detail = row
            .path
            .as_deref()
            .map(|path| path.display().to_string())
            .or_else(|| row.error.clone())
            .unwrap_or_else(|| format!("scopes: {}", row.scopes.join(", ")));
        println!("  {:width$}  {:11}  {detail}", row.id, row.status);
    }
    println!(
        "Install: octocode install --ide <id> [--scope {}]",
        scope.name()
    );
    0
}

/// An unknown client id, with the ids that contain it (`claude` names both
/// `claude-code` and `claude-desktop`) or else every id.
fn unknown_client_message(requested: &str) -> String {
    let ids: Vec<&str> = CLIENTS.iter().map(|client| client.id).collect();
    let near: Vec<&str> = ids
        .iter()
        .copied()
        .filter(|id| id.contains(requested))
        .collect();
    if near.is_empty() {
        format!("Unknown --ide {requested}. Ids: {}", ids.join(", "))
    } else {
        format!(
            "Unknown --ide {requested}. Did you mean: {}?",
            near.join(", ")
        )
    }
}

fn rollback_destination(backup: &Path) -> PathBuf {
    let name = backup
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    // Hidden backups are the rolling `.{name}.bak`, or older ones that add a
    // 128-bit nonce: `.{name}-{hex}.bak`. Otherwise `{name}.bak`.
    let Some(stem) = name
        .strip_suffix(".bak")
        .and_then(|name| name.strip_prefix('.'))
        .filter(|stem| !stem.is_empty())
    else {
        return backup.with_extension("");
    };
    let original = match stem.rsplit_once('-') {
        Some((original, nonce))
            if nonce.len() == 32
                && nonce.bytes().all(|byte| byte.is_ascii_hexdigit())
                && !original.is_empty() =>
        {
            original
        }
        _ => stem,
    };
    backup.with_file_name(original)
}

fn install(target: &Target, args: &InstallArgs) -> Result<u8, String> {
    use octocode_native::config::{config_revision, read_private_config, replace_private_config};
    let (client, config_path, keys) = (target.client, target.path.as_path(), &target.keys);
    let original = read_private_config(config_path)
        .map_err(|error| format!("Cannot safely read agent configuration: {error}"))?;
    let root = mcp_manage::parse_document(client.format, original.as_deref().unwrap_or(""))?;
    if args.check {
        return report_check(target, &root, args);
    }
    if args.remove {
        return remove(target, original.as_deref(), &root, args);
    }
    let server = mcp_manage::server_entry(client.shape, &server_spec(args));
    let existing = Existing::of(mcp_manage::entry_at(client, &root, keys), &server);
    if existing != Existing::Absent && !args.force && !args.dry_run {
        return already_installed(config_path, args, existing);
    }
    let rendered = mcp_manage::render_document(
        client,
        original.as_deref().unwrap_or(""),
        keys,
        Some(&server),
    )?;
    if args.dry_run {
        let preview = json!({"success":true,"dryRun":true,"ide":client.id,"scope":target.scope,"configPath":config_path,"entry":mcp_manage::projection(client,Some(&server))});
        if args.json {
            return Ok(super::write_json(&preview));
        }
        println!("{}", dry_run_text(client, config_path, &server, &existing));
        return Ok(0);
    }
    // An existing agent file keeps its previous content in the rolling backup.
    let (_, backed_up) = replace_private_config(
        config_path,
        &config_revision(original.as_deref()),
        &rendered,
        original.is_some(),
    )
    .map_err(|error| error.to_string())?;
    if args.json {
        return Ok(super::write_json(
            &json!({"success":true,"ide":client.id,"scope":target.scope,"configPath":config_path,"backupPath":backed_up}),
        ));
    }
    println!(
        "Installed octocode MCP for {} at {}",
        client.id,
        config_path.display()
    );
    println!(
        "Next: restart {} so it loads the server; check GitHub access with `octocode auth status`.",
        client.id
    );
    Ok(0)
}

/// `install --remove`: delete the Octocode entry, keeping the previous file
/// in the rolling backup. No entry is a successful no-op; an entry install
/// did not write needs `--force`, as overwriting one does.
fn remove(
    target: &Target,
    original: Option<&str>,
    root: &Value,
    args: &InstallArgs,
) -> Result<u8, String> {
    use octocode_native::config::{config_revision, replace_private_config};
    let (client, config_path) = (target.client, target.path.as_path());
    let report = |value: Value| super::write_json(&value);
    let Some(entry) = mcp_manage::entry_at(client, root, &target.keys) else {
        if args.json {
            return Ok(report(
                json!({"success":true,"removed":false,"ide":client.id,"scope":target.scope,"configPath":config_path}),
            ));
        }
        eprintln!(
            "No octocode entry for {} at {}; nothing to remove",
            client.id,
            config_path.display()
        );
        return Ok(0);
    };
    if !args.force && !valid_server(client, root, &target.keys) {
        if args.json {
            let written = report(
                json!({"success":false,"removed":false,"custom":true,"ide":client.id,"scope":target.scope,"configPath":config_path}),
            );
            return Ok(if written == 0 { 1 } else { written });
        }
        eprintln!(
            "The octocode entry in {} was not written by install; pass --force to remove it",
            config_path.display()
        );
        return Ok(1);
    }
    if args.dry_run {
        if args.json {
            return Ok(report(
                json!({"success":true,"dryRun":true,"remove":true,"ide":client.id,"scope":target.scope,"configPath":config_path,"entry":mcp_manage::projection(client,Some(entry))}),
            ));
        }
        println!(
            "Dry run for {}: nothing written.\n  config   {}\n  action   remove the octocode entry\nRun without --dry-run to remove it.",
            client.id,
            config_path.display()
        );
        return Ok(0);
    }
    let rendered = mcp_manage::render_document(client, original.unwrap_or(""), &target.keys, None)?;
    let (_, backed_up) =
        replace_private_config(config_path, &config_revision(original), &rendered, true)
            .map_err(|error| error.to_string())?;
    if args.json {
        return Ok(report(
            json!({"success":true,"removed":true,"ide":client.id,"scope":target.scope,"configPath":config_path,"backupPath":backed_up}),
        ));
    }
    println!(
        "Removed octocode MCP for {} from {}",
        client.id,
        config_path.display()
    );
    if let Some(backup) = backed_up {
        println!("Undo: octocode install --rollback {}", backup.display());
    }
    println!("Next: restart {} so it unloads the server.", client.id);
    Ok(0)
}

pub(super) fn valid_server(client: &ClientSpec, root: &Value, keys: &[String]) -> bool {
    let Some(server) = mcp_manage::entry_at(client, root, keys) else {
        return false;
    };
    if mcp_manage::entry_enabled(client, root, server) == Some(false) {
        return false;
    }
    if match client.shape {
        EntryShape::Opencode => server.get("type").and_then(Value::as_str) != Some("local"),
        EntryShape::Goose => server.get("type").and_then(Value::as_str) != Some("stdio"),
        EntryShape::Stdio | EntryShape::Untyped => false,
    } {
        return false;
    }
    let Some((command, args)) = command_line(client.shape, server) else {
        return false;
    };
    let Some(method) = Method::of_command(command) else {
        return false;
    };
    args.iter().all(Value::is_string)
        && args.iter().any(|arg| {
            arg.as_str()
                .is_some_and(|value| value == "octocode-mcp" || value.starts_with("octocode-mcp@"))
        })
        && (method != Method::Pnpm || args.first().and_then(Value::as_str) == Some("dlx"))
}

fn report_check(target: &Target, root: &Value, args: &InstallArgs) -> Result<u8, String> {
    let (ide, path) = (target.client.id, target.path.as_path());
    let valid = valid_server(target.client, root, &target.keys);
    let exit = if valid { 0 } else { 1 };
    if args.json {
        let written = super::write_json(
            &json!({"success": valid, "check": true, "ide": ide, "scope": target.scope, "configPath": path, "installed": valid}),
        );
        return Ok(if written == 0 { exit } else { written });
    }
    if valid {
        println!("Octocode MCP is configured for {ide} at {}", path.display());
    } else {
        eprintln!(
            "No valid Octocode MCP entry for {ide} at {}",
            path.display()
        );
    }
    Ok(exit)
}

/// An entry's runner and its arguments: opencode keeps both in one
/// `command` array; every other shape has a command key plus `args`.
fn command_line(shape: EntryShape, server: &Value) -> Option<(&str, &[Value])> {
    if shape == EntryShape::Opencode {
        let (first, rest) = server.get("command")?.as_array()?.split_first()?;
        Some((first.as_str()?, rest))
    } else {
        Some((
            server.get(shape.command_key())?.as_str()?,
            server.get("args")?.as_array()?.as_slice(),
        ))
    }
}

/// Whether the client config already carries an octocode entry, and whether it
/// matches the entry this install would write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Existing {
    Absent,
    Same,
    Different,
}

impl Existing {
    fn of<T: PartialEq>(previous: Option<&T>, next: &T) -> Self {
        match previous {
            None => Self::Absent,
            Some(previous) if previous == next => Self::Same,
            Some(_) => Self::Different,
        }
    }
}

/// The `install --dry-run` preview on a terminal: what would change and the
/// entry's launch command, never env values.
fn dry_run_text(
    client: &ClientSpec,
    config_path: &Path,
    server: &Value,
    existing: &Existing,
) -> String {
    let words = |value: Option<&Value>| -> Vec<String> {
        match value {
            Some(Value::String(word)) => vec![word.clone()],
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        }
    };
    let mut command = words(server.get(client.shape.command_key()));
    command.extend(words(server.get("args")));
    let env = server
        .get(client.shape.env_key())
        .and_then(Value::as_object)
        .map(|env| env.keys().cloned().collect::<Vec<_>>().join(", "))
        .filter(|keys| !keys.is_empty())
        .unwrap_or_else(|| "none".to_owned());
    let action = match existing {
        Existing::Absent => "add the octocode entry",
        Existing::Same => "none: the entry already matches",
        Existing::Different => "replace the existing octocode entry",
    };
    format!(
        "Dry run for {}: nothing written.\n  config   {}\n  action   {action}\n  command  {}\n  env      {env}\nRun without --dry-run to write it.",
        client.id,
        config_path.display(),
        command.join(" ")
    )
}

/// Shared "already installed" response. An identical entry is a successful
/// no-op (exit 0) so setup scripts can re-run install; a differing entry is
/// left untouched and needs `--force` (exit 1).
fn already_installed(
    config_path: &Path,
    args: &InstallArgs,
    existing: Existing,
) -> Result<u8, String> {
    if existing == Existing::Same {
        if args.json {
            return Ok(super::write_json(&json!({
                "success": true,
                "alreadyInstalled": true,
                "unchanged": true,
                "configPath": config_path
            })));
        }
        eprintln!(
            "octocode is already installed in {} — unchanged",
            config_path.display()
        );
        return Ok(0);
    }
    if args.json {
        let written = super::write_json(&json!({
            "success": false,
            "alreadyInstalled": true,
            "configPath": config_path
        }));
        return Ok(if written == 0 { 1 } else { written });
    }
    eprintln!(
        "octocode is already installed in {} — pass --force to overwrite",
        config_path.display()
    );
    Ok(1)
}

#[cfg(test)]
fn render_fixture(
    client: &ClientSpec,
    existing: Option<&str>,
    args: &InstallArgs,
) -> Result<(String, Existing), String> {
    let text = existing.unwrap_or("");
    let keys = client.server_keys();
    let root = mcp_manage::parse_document(client.format, text)?;
    let server = mcp_manage::default_entry(client, args.method);
    let already = Existing::of(mcp_manage::entry_at(client, &root, &keys), &server);
    let rendered = mcp_manage::render_document(client, text, &keys, Some(&server))?;
    Ok((rendered, already))
}

#[cfg(test)]
mod tests {
    use super::{Existing, command_line, render_fixture, server_spec};
    use crate::cli::mcp_clients::{self, ClientSpec, ConfigFormat, EntryShape};
    use crate::cli::mcp_manage::{Method, ServerSpec, server_entry};
    use serde_json::json;

    fn spec(id: &str) -> &'static ClientSpec {
        mcp_clients::client(id).expect("supported client")
    }

    /// The picker's labels: every client id, in registry order.
    fn labels() -> Vec<String> {
        mcp_clients::CLIENTS
            .iter()
            .map(|client| client.id.to_owned())
            .collect()
    }

    /// `client`'s home-scope entry, kept in `path`.
    fn home(client: &'static ClientSpec, path: &std::path::Path) -> super::Target {
        super::Target {
            client,
            scope: "home".into(),
            path: path.to_path_buf(),
            keys: client.server_keys(),
        }
    }

    fn default_args() -> super::InstallArgs {
        super::InstallArgs {
            ide: Some("cursor".into()),
            ..super::InstallArgs::default()
        }
    }

    #[test]
    fn client_picker_retries_invalid_input_and_uses_the_shared_registry() {
        let mut input = std::io::Cursor::new(b"0\nnot-a-number\n1\n");
        let mut output = Vec::new();
        let labels = labels();
        let selected =
            super::select_client(&labels, &mut input, &mut output).expect("select client");
        assert_eq!(selected, Some(mcp_clients::CLIENTS[0].id));
        let text = String::from_utf8(output).expect("UTF-8 output");
        for client in &mcp_clients::CLIENTS {
            assert!(text.contains(client.id));
        }
        assert_eq!(text.matches("Enter a number from").count(), 2);
    }

    #[test]
    fn client_picker_cancels_on_eof_or_empty_input() {
        for input in [b"".as_slice(), b"\n", b" \r\n"] {
            assert_eq!(
                super::select_client(&labels(), &mut std::io::Cursor::new(input), &mut Vec::new())
                    .expect("cancel selection"),
                None
            );
        }
    }

    #[test]
    fn client_picker_bounds_input() {
        let input = vec![b'1'; 1025];
        assert!(
            super::select_client(&labels(), &mut std::io::Cursor::new(input), &mut Vec::new())
                .is_err()
        );
    }

    #[test]
    fn passed_env_names_are_config_contract_names() {
        assert!(super::local_enabled_env().is_some());
        assert!(octocode_native::config::ENV_TOKEN_VARS.contains(&super::PASS_ENV_TOKEN));
        let spec = super::server_spec(&super::InstallArgs {
            enable_local: Some(false),
            ..default_args()
        });
        assert_eq!(
            spec.env.get(super::local_enabled_env().unwrap_or_default()),
            Some(&"false".to_owned())
        );
    }

    #[test]
    fn client_ids_resolve_exactly_and_near_misses_are_named() {
        assert_eq!(spec("claude-desktop").id, "claude-desktop");
        assert_eq!(spec("cursor").id, "cursor");
        assert!(mcp_clients::client("claude").is_none());
        assert!(mcp_clients::client("vscode").is_none());
        let message = super::unknown_client_message("claude");
        assert!(
            message.contains("claude-code") && message.contains("claude-desktop"),
            "{message}"
        );
    }

    #[test]
    fn server_is_npx_latest_with_yes_flag() {
        let server = server_entry(EntryShape::Stdio, &server_spec(&default_args()));
        assert_eq!(server["command"], "npx");
        assert_eq!(server["args"], json!(["-y", "octocode-mcp@latest"]));
        assert_ne!(spec("codex").format, ConfigFormat::Json);
    }

    #[test]
    fn server_uses_bunx_when_specified() {
        let spec = server_spec(&super::InstallArgs {
            method: crate::cli::mcp_manage::Method::Bunx,
            ..default_args()
        });
        let server = server_entry(EntryShape::Stdio, &spec);
        assert_eq!(server["command"], "bunx");
        assert_eq!(server["args"], json!(["octocode-mcp@latest"]));
    }

    /// The module promise, held by the entry's inputs: every runner
    /// (`--method`) in every client's shape starts the npm server through
    /// that runner, never the native host (`octo`/`octocode`) or an `mcp`
    /// subcommand. Only the env varies at run time, so this is checked here,
    /// not on every install.
    #[test]
    fn every_method_in_every_shape_runs_the_npm_server() {
        for &method in <Method as clap::ValueEnum>::value_variants() {
            for client in &mcp_clients::CLIENTS {
                let server = server_entry(client.shape, &ServerSpec::new(method));
                let (command, args) =
                    command_line(client.shape, &server).expect("an entry has a command line");
                let context = format!("{} {method:?} {server}", client.id);
                assert_eq!(Method::of_command(command), Some(method), "{context}");
                assert!(
                    args.iter().all(|arg| arg.as_str() != Some("mcp")),
                    "{context}"
                );
                assert!(
                    args.iter()
                        .any(|arg| arg.as_str() == Some("octocode-mcp@latest")),
                    "{context}"
                );
            }
        }
    }

    #[test]
    fn client_formats_route_by_extension() {
        assert_eq!(spec("cursor").format, ConfigFormat::Json);
        assert_eq!(spec("codex").format, ConfigFormat::Toml);
        assert_eq!(spec("goose").format, ConfigFormat::Yaml);
    }

    #[test]
    fn codex_and_goose_have_config_paths() {
        assert!(
            spec("codex")
                .home_path()
                .expect("codex config path")
                .ends_with(".codex/config.toml")
        );
        assert!(
            spec("goose")
                .home_path()
                .expect("goose config path")
                .ends_with(if cfg!(windows) {
                    "Block/goose/config/config.yaml"
                } else {
                    "goose/config.yaml"
                })
        );
    }

    #[test]
    fn codex_toml_renders_mcp_servers_table_and_merges() {
        let (rendered, already) =
            render_fixture(spec("codex"), None, &default_args()).expect("render");
        assert_eq!(already, Existing::Absent);
        // Valid TOML with the codex-native table shape.
        let parsed: toml::Value = toml::from_str(&rendered).expect("valid toml");
        let entry = &parsed["mcp_servers"]["octocode"];
        assert_eq!(entry["command"].as_str(), Some("npx"));
        assert_eq!(
            entry["args"].as_array().expect("args is array").len(),
            2,
            "npx -y octocode-mcp@latest"
        );

        // Merges into an existing config, preserving other servers and detecting reinstall.
        let existing = "[mcp_servers.other]\ncommand = \"foo\"\nargs = []\n";
        let (merged, already_present) =
            render_fixture(spec("codex"), Some(existing), &default_args()).expect("merge");
        let parsed: toml::Value = toml::from_str(&merged).expect("valid toml");
        assert!(
            parsed["mcp_servers"].get("other").is_some(),
            "preserves other"
        );
        assert!(parsed["mcp_servers"].get("octocode").is_some());
        assert_eq!(already_present, Existing::Absent);
        let (_again, now_present) =
            render_fixture(spec("codex"), Some(&merged), &default_args()).expect("re");
        assert_eq!(
            now_present,
            Existing::Same,
            "detects identical octocode entry"
        );
        let edited = merged.replace("npx", "node");
        let (_, differs) =
            render_fixture(spec("codex"), Some(&edited), &default_args()).expect("diff");
        assert_eq!(differs, Existing::Different, "detects a user-edited entry");
    }

    #[test]
    fn goose_yaml_renders_extensions_and_merges() {
        let (rendered, already) =
            render_fixture(spec("goose"), None, &default_args()).expect("render");
        assert_eq!(already, Existing::Absent);
        let parsed =
            octocode_engine::text::yaml_utils::yaml_to_json(&rendered).expect("valid yaml");
        let entry = &parsed["extensions"]["octocode"];
        assert_eq!(entry["type"].as_str(), Some("stdio"));
        assert_eq!(entry["cmd"].as_str(), Some("npx"));
        assert_eq!(entry["enabled"].as_bool(), Some(true));

        let existing = "extensions:\n  other:\n    type: stdio\n    cmd: foo\n";
        let (merged, _) =
            render_fixture(spec("goose"), Some(existing), &default_args()).expect("merge");
        let parsed = octocode_engine::text::yaml_utils::yaml_to_json(&merged).expect("valid yaml");
        assert!(
            parsed["extensions"].get("other").is_some(),
            "preserves other"
        );
        let (_again, now_present) =
            render_fixture(spec("goose"), Some(&merged), &default_args()).expect("re");
        assert_eq!(
            now_present,
            Existing::Same,
            "detects identical octocode extension"
        );
    }

    #[test]
    fn reinstall_is_a_noop_when_identical_and_needs_force_when_edited() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir
            .path()
            .canonicalize()
            .expect("canonical tempdir")
            .join("mcp.json");
        assert_eq!(
            super::install(&home(spec("cursor"), &path), &default_args()).expect("first"),
            0
        );
        let written = std::fs::read_to_string(&path).expect("read");
        assert_eq!(
            super::install(&home(spec("cursor"), &path), &default_args()).expect("again"),
            0
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            written,
            "no rewrite"
        );
        std::fs::write(&path, written.replace("npx", "node")).expect("edit");
        assert_eq!(
            super::install(&home(spec("cursor"), &path), &default_args()).expect("edited"),
            1
        );
    }
    #[test]
    fn client_specific_json_shapes_are_valid_and_preserve_other_settings() {
        for (ide, key) in [
            ("cursor", "mcpServers"),
            ("zed", "context_servers"),
            ("opencode", "mcp"),
        ] {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir
                .path()
                .canonicalize()
                .expect("canonical tempdir")
                .join("config.json");
            std::fs::write(&path, r#"{"theme":"dark","unrelated":{"keep":true}}"#)
                .expect("fixture");
            assert_eq!(
                super::install(&home(spec(ide), &path), &default_args()).expect("install"),
                0
            );
            let root: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
            assert_eq!(root["theme"], "dark");
            assert_eq!(root["unrelated"]["keep"], true);
            assert!(root[key].get("octocode").is_some());
            assert!(super::valid_server(
                spec(ide),
                &root,
                &spec(ide).server_keys()
            ));
            if ide == "opencode" {
                assert_eq!(
                    root[key]["octocode"]["command"],
                    json!(["npx", "-y", "octocode-mcp@latest"])
                );
            }
            let written = std::fs::read_to_string(&path).expect("read");
            assert_eq!(
                super::install(&home(spec(ide), &path), &default_args()).expect("existing"),
                0,
                "identical reinstall is a no-op"
            );
            assert_eq!(std::fs::read_to_string(&path).expect("read"), written);
        }
    }

    #[test]
    fn checks_inspect_existing_entries_without_creating_or_repairing_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let check = super::InstallArgs {
            check: true,
            json: true,
            ..default_args()
        };
        for (ide, file) in [
            ("cursor", "c.json"),
            ("zed", "z.json"),
            ("opencode", "o.json"),
            ("codex", "c.toml"),
            ("goose", "g.yaml"),
        ] {
            let path = dir
                .path()
                .canonicalize()
                .expect("canonical tempdir")
                .join(file);
            let inspect = |args: &super::InstallArgs| super::install(&home(spec(ide), &path), args);
            assert_eq!(inspect(&check).expect("missing"), 1, "{ide}");
            assert!(!path.exists());
            assert_eq!(inspect(&default_args()).expect("install"), 0);
            let before = std::fs::read(&path).expect("before");
            assert_eq!(inspect(&check).expect("valid"), 0);
            assert_eq!(std::fs::read(&path).expect("after"), before);
        }
        let keys = spec("cursor").server_keys();
        assert!(!super::valid_server(
            spec("cursor"),
            &json!({"mcpServers":{"octocode":{"command":"npx","args":["other-package"]}}}),
            &keys
        ));
        assert!(!super::valid_server(
            spec("vscode-cline"),
            &json!({"mcpServers":{"octocode":{"command":"npx","args":["octocode-mcp@latest"],"disabled":true}}}),
            &keys
        ));
    }

    /// `--remove` deletes only the entry install wrote, keeps the rest of
    /// the file and a backup, and is a no-op when nothing is installed.
    #[test]
    fn remove_deletes_the_installed_entry_and_refuses_a_custom_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir
            .path()
            .canonicalize()
            .expect("canonical tempdir")
            .join("mcp.json");
        let target = home(spec("cursor"), &path);
        let remove = super::InstallArgs {
            remove: true,
            ..default_args()
        };
        assert_eq!(super::install(&target, &remove).expect("nothing"), 0);
        assert!(!path.exists(), "a no-op removal writes nothing");

        std::fs::write(
            &path,
            r#"{"mcpServers":{"other":{"command":"x"},"octocode":{"command":"node","args":["dev.js"]}}}"#,
        )
        .expect("fixture");
        assert_eq!(super::install(&target, &remove).expect("custom"), 1);
        assert!(
            std::fs::read_to_string(&path)
                .expect("read")
                .contains("dev.js")
        );

        std::fs::remove_file(&path).expect("reset");
        std::fs::write(&path, r#"{"mcpServers":{"other":{"command":"x"}}}"#).expect("fixture");
        assert_eq!(
            super::install(&target, &default_args()).expect("install"),
            0
        );
        let dry = super::InstallArgs {
            dry_run: true,
            ..remove
        };
        let installed = std::fs::read_to_string(&path).expect("installed");
        assert_eq!(super::install(&target, &dry).expect("dry"), 0);
        assert_eq!(std::fs::read_to_string(&path).expect("read"), installed);
        let remove = super::InstallArgs {
            remove: true,
            ..default_args()
        };
        assert_eq!(super::install(&target, &remove).expect("remove"), 0);
        let root: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(root, json!({"mcpServers":{"other":{"command":"x"}}}));
        assert!(
            path.with_file_name(".mcp.json.bak").exists(),
            "removal keeps a backup"
        );
    }

    /// The local scope writes Claude Code's per-project section of its user
    /// config, beside other projects' entries.
    #[test]
    fn local_scope_writes_this_directory_project_entry() {
        let home_dir = tempfile::tempdir().expect("home");
        let cwd = tempfile::tempdir().expect("cwd");
        let path = home_dir.path().join(".claude.json");
        std::fs::write(&path, r#"{"projects":{"/elsewhere":{"mcpServers":{}}}}"#).expect("fixture");
        let target = super::Target {
            client: spec("claude-code"),
            scope: "local".into(),
            path: path.clone(),
            keys: vec![
                "projects".into(),
                cwd.path().to_string_lossy().into_owned(),
                "mcpServers".into(),
                "octocode".into(),
            ],
        };
        assert_eq!(
            super::install(&target, &default_args()).expect("install"),
            0
        );
        let root: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        let project = &root["projects"][cwd.path().to_string_lossy().as_ref()];
        assert_eq!(project["mcpServers"]["octocode"]["command"], "npx");
        assert!(root["projects"]["/elsewhere"].is_object());
        assert!(super::valid_server(
            spec("claude-code"),
            &root,
            &target.keys
        ));
        assert!(!super::valid_server(
            spec("claude-code"),
            &root,
            &spec("claude-code").server_keys()
        ));
    }

    #[test]
    fn rollback_dry_run_preserves_destination_and_real_rollback_restores() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir
            .path()
            .canonicalize()
            .expect("canonical tempdir")
            .join("config.json");
        let backup = dir
            .path()
            .canonicalize()
            .expect("canonical tempdir")
            .join("config.json.bak");
        std::fs::write(&path, "current").expect("current");
        std::fs::write(&backup, "previous").expect("backup");
        let mut args = super::InstallArgs {
            rollback: Some(backup.to_string_lossy().into_owned()),
            dry_run: true,
            json: true,
            ..default_args()
        };
        assert_eq!(super::run(args), 0);
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "current");
        args = super::InstallArgs {
            rollback: Some(backup.to_string_lossy().into_owned()),
            ..default_args()
        };
        assert_eq!(super::run(args), 0);
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "previous");
    }

    #[test]
    fn client_paths_match_their_documented_configuration_files() {
        for (ide, suffix) in [
            ("kiro", ".kiro/settings/mcp.json"),
            ("vscode-continue", ".continue/mcpServers/octocode.json"),
            ("opencode", "opencode/opencode.json"),
        ] {
            let path = spec(ide).home_path().expect("path");
            assert!(
                path.ends_with(suffix)
                    || ide == "opencode" && path.ends_with("opencode/opencode.jsonc"),
                "{ide}"
            );
        }
    }
    #[test]
    fn randomized_private_backup_restores_the_original_filename() {
        let path = std::path::Path::new("/safe/.config.json-0123456789abcdef0123456789abcdef.bak");
        assert_eq!(
            super::rollback_destination(path),
            std::path::Path::new("/safe/config.json")
        );
        let path = std::path::Path::new("/safe/..claude.json-0123456789abcdef0123456789abcdef.bak");
        assert_eq!(
            super::rollback_destination(path),
            std::path::Path::new("/safe/.claude.json")
        );
        for (rolling, original) in [
            ("/safe/.mcp.json.bak", "/safe/mcp.json"),
            ("/safe/..claude.json.bak", "/safe/.claude.json"),
        ] {
            let rolling = std::path::Path::new(rolling);
            assert_eq!(
                super::rollback_destination(rolling),
                std::path::Path::new(original)
            );
            assert_eq!(
                octocode_native::config::backup_path(&super::rollback_destination(rolling)),
                rolling
            );
        }
    }
}
