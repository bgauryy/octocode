mod commands;
mod config;
mod graph;
mod launcher;
mod lsp_provision;
mod mcp_clients;
mod mcp_install;
mod mcp_manage;
mod serve;
mod system;
use clap::{CommandFactory, FromArgMatches, Parser};
use commands::{AuthCommand, Command, ConfigCommand, ToolArgs};
use octocode_native::config::RuntimeSurface;
use octocode_native::runtime::{ExitClass, FailureKind, HostOptions, ToolRuntime};
use octocode_native::tools::id::ToolId;
use serde_json::{Value, json};

const INTERACTIVE_EXECUTION_TIMEOUT_SECS: u64 = 300;
use std::io::{self, Write};

#[derive(Parser)]
#[command(
    name = "octocode",
    version,
    about = "Native Octocode research tools",
    long_about = ROOT_HELP
)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

const ROOT_HELP: &str = r#"Native Octocode research tools.

Tools take one JSON query and print text on a terminal, JSON on a pipe:
  octocode <tool> '<json>'          run a tool; --input FILE|- reads the query, --json forces JSON
  octocode schema                   list tools with agent instructions
  octocode schema <tool>            print a tool's input contract

Code graph (persisted; see `octocode graph --help`):
  octocode graph ingest <path>      parse once into <workspace>/.octocode/graph
  octocode graph query <op> [ref]   callers, impact, cycles, issues, ... in milliseconds

Errors follow the output: JSON on stdout in JSON mode, text on stderr otherwise.

Exit codes:
  0    success
  1    empty result / no matches
  2    invalid input, including any rejected batch row
  3    not found
  4    auth required
  5    execution error
  6    partial result: the response carries a re-runnable next.* continuation
  7    rate limited
  130  interrupted (Ctrl-C)"#;

/// The one error envelope, shared with contract input errors
/// (`contracts::validate`) so callers parse a single shape.
fn emit_error(msg: &str, json_out: bool) {
    if json_out {
        println!(
            "{}",
            octocode_native::contracts::tool_error(None, msg, None)
        );
    } else {
        eprintln!("{msg}");
    }
}

/// A tool call's unusable query (missing or malformed JSON) in the command's
/// output mode: the `invalidInput` envelope on stdout, or text on stderr.
fn emit_tool_error(tool: &str, msg: &str, json_out: bool) {
    if json_out {
        let mut envelope = octocode_native::contracts::tool_error(Some(tool), msg, None);
        envelope["errorCode"] = json!("invalidInput");
        println!("{envelope}");
    } else {
        eprintln!("{msg}");
    }
}

/// Machine output: a tool, `graph`, or `--json` call whose stdout is a pipe,
/// or any call with `--json`. A terminal keeps text.
fn machine_output(json_flag: bool) -> bool {
    use std::io::IsTerminal;
    json_flag || !io::stdout().is_terminal()
}

/// Commands whose output is text for people unless `--json` asks otherwise;
/// every other command (tools, `graph`, an unknown name) answers in JSON.
fn text_command(name: &str) -> bool {
    matches!(
        name,
        "config" | "auth" | "skill" | "install" | "help" | "lsp-server" | "cache" | "serve"
    )
}

/// Whether a parse error is reported as JSON: `--json` anywhere, or a
/// non-text command (a mistyped tool name included) whose stdout is a pipe.
fn json_errors_for(argv: &[std::ffi::OsString]) -> bool {
    let json_flag = argv.iter().any(|arg| arg == "--json");
    let command = argv
        .iter()
        .skip(1)
        .filter_map(|arg| arg.to_str())
        .find(|arg| !arg.starts_with('-'));
    json_flag || (!command.is_some_and(text_command) && machine_output(false))
}

pub fn parse_args() -> Result<Args, u8> {
    parse_args_from(std::env::args_os())
}

fn parse_args_from<I, T>(argv: I) -> Result<Args, u8>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let argv: Vec<std::ffi::OsString> = argv.into_iter().map(Into::into).collect();
    match Args::command()
        .try_get_matches_from(&argv)
        .and_then(|matches| Args::from_arg_matches(&matches))
    {
        Ok(args) => Ok(args),
        Err(error) => {
            let error = if is_help(&error) {
                hide_unavailable_tools(Args::command())
                    .try_get_matches_from(&argv)
                    .err()
                    .unwrap_or(error)
            } else {
                error
            };
            let displays_text =
                is_help(&error) || error.kind() == clap::error::ErrorKind::DisplayVersion;
            if displays_text || !json_errors_for(&argv) {
                let _ = error.print();
                return Err(error.exit_code() as u8);
            }
            // The error line and clap's `tip:` (the did-you-mean names).
            let rendered = error.render().to_string();
            let message = rendered
                .lines()
                .map(str::trim)
                .filter(|line| line.starts_with("error: ") || line.starts_with("tip: "))
                .map(|line| line.trim_start_matches("error: "))
                .collect::<Vec<_>>()
                .join("; ");
            emit_error(&message, true);
            Err(2)
        }
    }
}

fn is_help(error: &clap::Error) -> bool {
    matches!(
        error.kind(),
        clap::error::ErrorKind::DisplayHelp
            | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    )
}

/// Help lists only the tools this surface can run, as MCP `tools/list` does;
/// a hidden tool stays callable by name and keeps its own help.
fn hide_unavailable_tools(mut command: clap::Command) -> clap::Command {
    let Ok(runtime) = ToolRuntime::from_host(HostOptions {
        surface: RuntimeSurface::Cli,
        ..HostOptions::default()
    }) else {
        return command;
    };
    for tool in ToolId::ALL {
        if !runtime.is_available(tool.as_str()) {
            command = command.mut_subcommand(tool.as_str(), |sub| sub.hide(true));
        }
    }
    command
}

/// Fields the contract adds to every query of `tool` (the trimmed
/// `goal`/`reasoning` pair and the defaulted `debug` flag); listing them per
/// mode is noise.
fn meta_fields(tool: &Value) -> Vec<&str> {
    let rules = tool["rules"].as_array().into_iter().flatten();
    rules
        .flat_map(|rule| match rule["id"].as_str() {
            Some("prepare.reasoning-trim") => rule["args"]["fields"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect(),
            Some("prepare.debug") => rule["args"]["field"].as_str().into_iter().collect(),
            _ => Vec::new(),
        })
        .collect()
}

/// Compact field list for hidden `catalog` rows. Union tools (`anyOf`/`oneOf`) list
/// each mode, labelled by its discriminator const (or branch title):
/// `operation=code[keywords*, owner*, …] | operation=tree[…]`.
fn compact_fields(tool: &Value) -> String {
    let schema = tool.get("querySchema").unwrap_or(&Value::Null);
    let meta = meta_fields(tool);
    let resolve = |variant: &'_ Value| -> Value {
        variant
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|reference| reference.strip_prefix("#/"))
            .and_then(|pointer| schema.pointer(&format!("/{pointer}")))
            .cloned()
            .unwrap_or_else(|| variant.clone())
    };
    let variants: Vec<Value> = ["anyOf", "oneOf"]
        .iter()
        .find_map(|key| schema.get(*key).and_then(Value::as_array))
        .map(|items| items.iter().map(resolve).collect())
        .unwrap_or_else(|| vec![schema.clone()]);
    if variants.len() == 1 {
        return variant_fields(&variants[0], None, &meta, 8);
    }
    let required_of = |variant: &Value| -> Vec<String> {
        variant["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    };
    let mut labels: Vec<(String, Option<String>)> = variants
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            // The discriminator is the const whose value differs across modes
            // (astTopology also fixes operation="topology" in every branch).
            let discriminator = variant["properties"].as_object().and_then(|properties| {
                properties.iter().find_map(|(name, field)| {
                    let value = field.get("const")?;
                    variants
                        .iter()
                        .any(|other| other["properties"][name.as_str()].get("const") != Some(value))
                        .then_some((name, value))
                })
            });
            match discriminator {
                Some((name, value)) => (
                    format!("{name}={}", value.as_str().unwrap_or_default()),
                    Some(name.clone()),
                ),
                None => (
                    variant["title"]
                        .as_str()
                        .map_or_else(|| format!("mode{}", index + 1), str::to_owned),
                    None,
                ),
            }
        })
        .collect();
    // Same label twice (e.g. astSearch match by pattern or rule): add the
    // required field that tells the branches apart.
    let original: Vec<String> = labels.iter().map(|label| label.0.clone()).collect();
    for index in 0..labels.len() {
        let duplicate = original
            .iter()
            .enumerate()
            .any(|(other, label)| other != index && *label == original[index]);
        if duplicate {
            let others: Vec<String> = variants
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != index)
                .flat_map(|(_, variant)| required_of(variant))
                .collect();
            if let Some(unique) = required_of(&variants[index])
                .into_iter()
                .find(|field| !others.contains(field))
            {
                labels[index].0 = format!("{}({unique})", labels[index].0);
            }
        }
    }
    variants
        .iter()
        .zip(labels)
        .map(|(variant, (label, discriminator))| {
            format!(
                "{label}{}",
                variant_fields(variant, discriminator.as_deref(), &meta, 6)
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn variant_fields(
    schema: &Value,
    discriminator: Option<&str>,
    meta: &[&str],
    max_fields: usize,
) -> String {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return "[]".to_owned();
    };
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    let mut names = properties
        .iter()
        // `{"not":{}}` marks a field this mode forbids.
        .filter(|(name, field)| {
            Some(name.as_str()) != discriminator
                && !meta.contains(&name.as_str())
                && field.get("not").is_none()
        })
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    names.sort_by_key(|name| (!required.contains(name), *name));
    let truncated = names.len() > max_fields;
    let mut fields = names
        .into_iter()
        .take(max_fields)
        .map(|name| format!("{name}{}", if required.contains(name) { "*" } else { "?" }))
        .collect::<Vec<_>>();
    if truncated {
        fields.push("…".to_owned());
    }
    format!("[{}]", fields.join(", "))
}

/// Env vars that gate a disabled tool, from the config contract bindings of
/// its gating key (`ToolId::availability_config_path`).
fn availability_env_var(name: &str) -> Option<String> {
    let vars = ToolId::from_name(name)?.availability_env_vars();
    (!vars.is_empty()).then(|| vars.join("|"))
}

/// Text for a disabled tool whose gating env key was present in a `.env`
/// file but not applied — the state change would otherwise be invisible.
fn dropped_key_hint(
    env_vars: &str,
    dotenv: &octocode_native::config::EnvApplyReport,
) -> Option<String> {
    // `env_vars` already lists every alias the config contract binds.
    for alias in env_vars.split('|').map(str::to_owned) {
        if dotenv.skipped_protected.contains(&alias) {
            return Some(format!(
                "{alias} was found in a .env file but dropped (protected); set it in the process environment or config file"
            ));
        }
        if dotenv.skipped_existing.contains(&alias) {
            return Some(format!(
                "{alias} in a .env file is shadowed by the process environment"
            ));
        }
    }
    None
}

/// Config paths that include or exclude tools (`tools.enabled` /
/// `tools.disabled`), from the config contract.
fn tool_list_config_paths() -> String {
    octocode_native::config::CONFIG_FIELDS
        .iter()
        .filter(|field| field.section == "tools")
        .map(|field| field.path)
        .collect::<Vec<_>>()
        .join("/")
}

fn compact_tool_catalog(
    catalog: &Value,
    contract: &Value,
    dotenv: &octocode_native::config::EnvApplyReport,
) -> Value {
    let contract_tools = contract.get("tools").and_then(Value::as_array);
    let tools = catalog
        .get("tools")
        .and_then(Value::as_array)
        .map(|tools| {
            tools
                .iter()
                .map(|tool| {
                    let name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
                    let enabled = tool
                        .get("available")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let mut availability = json!({ "enabled": enabled });
                    let tool_list_excluded =
                        tool.get("unavailableReason").and_then(Value::as_str) == Some("toolsList");
                    if !enabled {
                        if let Some(env_var) =
                            availability_env_var(name).filter(|_| !tool_list_excluded)
                        {
                            if let Some(hint) = dropped_key_hint(&env_var, dotenv) {
                                availability["hint"] = Value::String(hint);
                            }
                            availability["envVar"] = Value::String(env_var);
                        } else {
                            availability["configuration"] = Value::String(tool_list_config_paths());
                        }
                    }
                    let fields = contract_tools
                        .and_then(|tools| tools.iter().find(|candidate| candidate["name"] == name))
                        .map(compact_fields)
                        .unwrap_or_else(|| "[]".to_owned());
                    json!({
                        "name": name,
                        "fields": fields,
                        "availability": availability
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({
        "fingerprint": catalog["fingerprint"],
        "grammarCapabilities": catalog["grammarCapabilities"],
        "tools": tools
    })
}

pub async fn run(args: Args) -> u8 {
    let command = match args.command {
        Command::Config {
            command:
                Some(ConfigCommand::View {
                    no_open,
                    idle_timeout,
                }),
            ..
        } => {
            let mut args = vec!["--idle-timeout".to_owned(), idle_timeout.to_string()];
            if no_open {
                args.push("--no-open".to_owned());
            }
            return launcher::CONFIG_VIEW.run(&args);
        }
        Command::Serve { socket } => return serve::run(&socket).await,
        Command::Tool(tool) => return run_tool(tool).await,
        command => command,
    };
    let runtime = match start_runtime() {
        Ok(runtime) => runtime,
        Err(code) => return code_after_start_error(code, command.json_output()),
    };
    let result = dispatch(command, &runtime).await;
    runtime.close().await;
    result
}

fn start_runtime() -> Result<ToolRuntime, octocode_native::runtime::RuntimeError> {
    ToolRuntime::from_host(HostOptions {
        surface: RuntimeSurface::Cli,
        // Keep the outer execution budget above the worst configured cold start
        // plus one logical request: initialize, readiness, retries, and delays.
        timeout_secs: Some(INTERACTIVE_EXECUTION_TIMEOUT_SECS),
        ..HostOptions::default()
    })
}

fn code_after_start_error(error: octocode_native::runtime::RuntimeError, json_out: bool) -> u8 {
    emit_error(&format!("{}: {}", error.code, error.message), json_out);
    5
}

impl Command {
    /// The output mode the command was asked for.
    fn json_output(&self) -> bool {
        match self {
            Self::Tool(tool) => machine_output(tool.args.json),
            Self::Catalog | Self::Graph { .. } | Self::Schema { .. } => machine_output(false),
            Self::Config {
                json,
                command: None,
                ..
            } => *json,
            Self::Config {
                command:
                    Some(
                        ConfigCommand::Set { json, .. }
                        | ConfigCommand::Unset { json, .. }
                        | ConfigCommand::Check { json, .. },
                    ),
                ..
            } => *json,
            Self::Auth {
                command: AuthCommand::Status { json } | AuthCommand::Login { json, .. },
            } => *json,
            Self::Install { json, .. } | Self::LspServer { json, .. } => *json,
            _ => false,
        }
    }
}

async fn dispatch(command: Command, runtime: &ToolRuntime) -> u8 {
    let json_out = command.json_output();
    match command {
        Command::Tool(tool) => run_tool(tool).await,
        Command::Catalog => {
            let catalog = match runtime.catalog() {
                Ok(catalog) => catalog,
                Err(error) => {
                    emit_error(&error.message, true);
                    return 5;
                }
            };
            // Availability comes from the runtime catalog; field lists come
            // from the embedded enforcement contract (the catalog has none).
            let Ok(contract) = octocode_native::contracts::parsed_contract() else {
                emit_error("Embedded contract is invalid", true);
                return 5;
            };
            write_json(
                &compact_tool_catalog(&catalog, contract, &runtime.config().dotenv),
                true,
            )
        }
        Command::Schema { .. } => {
            emit_error(
                "`octocode schema` is served by the octocode npm launcher, which joins the tool contracts with this binary's catalog.",
                json_out,
            );
            5
        }
        Command::Config {
            command, manage, ..
        } => {
            if manage {
                return config_management(runtime);
            }
            match command {
                Some(ConfigCommand::Set {
                    key, value, stdin, ..
                }) => config::set(runtime, &key, value, stdin, json_out),
                Some(ConfigCommand::Unset { key, .. }) => config::unset(runtime, &key, json_out),
                Some(ConfigCommand::Check { key, .. }) => {
                    config::check(runtime, &key, json_out).await
                }
                // `config view` returns from `run` before the runtime starts.
                Some(ConfigCommand::View { .. }) | None => config::show(runtime, json_out),
            }
        }
        Command::Auth { command } => match command {
            AuthCommand::Status { json } => system::auth_status(runtime, json).await,
            AuthCommand::Login {
                hostname,
                force,
                refresh,
                json,
            } => system::login(runtime, hostname.as_deref(), force, refresh, json).await,
            AuthCommand::Logout => system::logout(runtime),
        },
        Command::Graph { command } => graph::graph(runtime, command),
        Command::Skill { args } => launcher::SKILL.run(&args),
        Command::Install {
            ide,
            force,
            dry_run,
            check,
            list,
            json,
            enable_local,
            pass_env,
            method,
            rollback,
        } => mcp_install::run(mcp_install::InstallArgs {
            ide,
            force,
            dry_run,
            check,
            list,
            json,
            enable_local,
            pass_env,
            method,
            rollback,
        }),
        Command::Cache { action } => system::cache(runtime, action),
        Command::LspServer {
            action,
            names,
            all,
            yes,
            force,
            json,
        } => lsp_provision::run(runtime, &action, names, all, yes, force, json).await,
        // Handled in `run` before the runtime starts.
        Command::Serve { .. } => 2,
    }
}

/// Read at most `limit` bytes from `reader`; `Ok(None)` when it holds more.
fn read_bounded(reader: impl io::Read, limit: u64) -> io::Result<Option<Vec<u8>>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes)?;
    Ok((bytes.len() as u64 <= limit).then_some(bytes))
}

/// Largest management request read from stdin.
const MAX_MANAGEMENT_REQUEST_BYTES: u64 = 128 * 1024;

fn config_management(runtime: &ToolRuntime) -> u8 {
    let result = read_bounded(io::stdin(), MAX_MANAGEMENT_REQUEST_BYTES)
        .map_err(|_| config::ManageError::from("Cannot read management request."))
        .and_then(|request| {
            let request = request.ok_or("Management request exceeds 128 KiB.")?;
            config_management_response(runtime, &request)
        });
    match result {
        Ok(response) => write_json(&response, true),
        Err(error) => {
            write_json(
                &json!({"success": false, "apiVersion": 1, "error": {"code": error.code(), "message": error.to_string()}}),
                true,
            );
            2
        }
    }
}

fn config_management_response(
    runtime: &ToolRuntime,
    bytes: &[u8],
) -> Result<Value, config::ManageError> {
    let request: Value =
        serde_json::from_slice(bytes).map_err(|_| "Management request must be valid JSON.")?;
    if !request.is_object() {
        return Err("Management request must be an object.".into());
    }
    let catalog = runtime
        .catalog()
        .map_err(|_| "Cannot verify native contract.")?;
    if request.get("expectedFingerprint") != catalog.get("fingerprint") {
        return Err(
            "Configuration management contracts do not match. Rebuild or update Octocode.".into(),
        );
    }
    let operation = request
        .get("operation")
        .and_then(Value::as_str)
        .ok_or("Missing management operation.")?;
    let data = match operation {
        "inspect" | "setEnv" | "removeEnv" | "setSetting" | "removeSetting" => {
            config::manage(runtime, &request)?
        }
        "agents" | "setAgent" | "removeAgent" => {
            let cwd = std::env::current_dir().map_err(|_| "Cannot resolve workspace.")?;
            mcp_manage::manage(&request, &cwd)?
        }
        _ => return Err("Unsupported management operation.".into()),
    };
    Ok(
        json!({"success": true, "apiVersion": 1, "fingerprint": catalog["fingerprint"], "data": data}),
    )
}

/// Parse the query, then run it on the workspace's warm server (lspSearch)
/// or in this process.
async fn run_tool(tool: commands::ToolCommand) -> u8 {
    let json_out = machine_output(tool.args.json);
    let input = match tool_input(tool.name, &tool.args, json_out) {
        Ok(input) => input,
        Err(code) => return code,
    };
    if tool.name == ToolId::LspSearch.as_str()
        && let Some(code) = serve::call(tool.name, &input, json_out).await
    {
        return code;
    }
    let runtime = match start_runtime() {
        Ok(runtime) => runtime,
        Err(error) => return code_after_start_error(error, json_out),
    };
    let code = execute(&runtime, tool.name, input, json_out).await;
    runtime.close().await;
    code
}

/// The parsed JSON query of a tool command, or the exit code of a usage error
/// already reported in the command's output mode.
fn tool_input(tool: &str, args: &ToolArgs, json_out: bool) -> Result<Value, u8> {
    let query_text = match args.query_text() {
        Ok(Some(text)) => text,
        Ok(None) => {
            emit_tool_error(
                tool,
                &format!(
                    "Missing JSON query. Usage: octocode {tool} '<json>' | --input FILE|-. Contract: octocode schema {tool}"
                ),
                json_out,
            );
            return Err(2);
        }
        Err(error) => {
            emit_tool_error(tool, &error, json_out);
            return Err(2);
        }
    };
    serde_json::from_str::<Value>(&query_text).map_err(|parse_error| {
        emit_tool_error(
            tool,
            &format!("Invalid JSON query: {parse_error}"),
            json_out,
        );
        2
    })
}

#[derive(Debug, PartialEq, Eq)]
enum Interrupted {
    /// The cancelled work finished.
    Drained,
    /// A second interrupt arrived first.
    Forced,
}

/// Wait for cancelled `execution` to finish, unless `second` (the next
/// interrupt) arrives first.
async fn drain_after_interrupt<T>(
    execution: impl std::future::Future<Output = T>,
    second: impl std::future::Future,
) -> Interrupted {
    tokio::select! {
        _ = execution => Interrupted::Drained,
        _ = second => Interrupted::Forced,
    }
}

/// Execute one tool call and print its result: the structured JSON in JSON
/// mode, else the rendered text MCP clients read. Exits with the code of the
/// runtime's [`ExitClass`].
async fn execute(runtime: &ToolRuntime, tool: &str, input: Value, json_out: bool) -> u8 {
    let execution = async {
        if json_out {
            runtime.execute("cli-1".into(), tool.into(), input).await
        } else {
            runtime
                .execute_rendered("cli-1".into(), tool.into(), input)
                .await
        }
    };
    tokio::pin!(execution);
    let result = tokio::select! {
        result = &mut execution => result,
        signal = tokio::signal::ctrl_c() => {
            if signal.is_err() {
                execution.await
            } else {
                runtime.requests.cancel("cli-1");
                eprintln!("Cancelling; press Ctrl-C again to exit now.");
                if drain_after_interrupt(&mut execution, tokio::signal::ctrl_c()).await
                    == Interrupted::Forced
                {
                    // Work that ignores cancellation must not hold the
                    // terminal hostage: the user asked twice (M5).
                    std::process::exit(130);
                }
                return 130;
            }
        }
    };
    let (printed, class) = match result {
        Ok(outcome) => {
            let class = outcome.exit_class();
            let printed = if json_out {
                write_json(&outcome.structured_content, true)
            } else {
                write_text(&outcome_text(&outcome.content))
            };
            (printed, class)
        }
        Err(error) => {
            let class = error.exit_class();
            let value = runtime_error_output(error);
            let printed = if json_out {
                write_json(&value, true)
            } else {
                eprintln!("{}", runtime_error_text(&value));
                0
            };
            (printed, class)
        }
    };
    match printed {
        0 => exit_code(class),
        code => code,
    }
}

/// The text channel of a tool result: its content blocks, in order.
fn outcome_text(content: &[octocode_native::response::pager::TextContent]) -> String {
    content
        .iter()
        .map(|block| block.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The terminal text for a runtime error: the runtime's one tool-error
/// projection (the error, then its repair details).
fn runtime_error_text(value: &Value) -> String {
    octocode_native::runtime::error::tool_error_text(value)
        .unwrap_or_else(|| "Tool call failed.".to_owned())
}

/// The stdout JSON for a runtime error: its structured payload typed with
/// the error's `errorCode` (as MCP's structuredContent), else an
/// `{error, errorCode}` envelope (stdout, so a caller discarding stderr still
/// sees why the call failed). Every string is secret-scrubbed, as at the
/// N-API boundary.
fn runtime_error_output(error: octocode_native::runtime::RuntimeError) -> Value {
    let mut value = match error.payload {
        Some(payload) => {
            let mut value = *payload;
            if value.is_object() && value.get("errorCode").is_none() {
                value["errorCode"] = json!(error.code);
            }
            value
        }
        None => {
            let mut value = json!({ "error": error.message, "errorCode": error.code });
            if error.code == "timeout" {
                value["details"] = json!([
                    "Retry -- the first call initialises the language server (~60 s cold start)."
                ]);
            }
            value
        }
    };
    octocode_native::security::scrub_error_payload(&mut value);
    value
}

fn exit_code(class: ExitClass) -> u8 {
    match class {
        ExitClass::Success => 0,
        ExitClass::Empty => 1,
        ExitClass::InvalidInput => 2,
        ExitClass::Failed(FailureKind::NotFound) => 3,
        ExitClass::Failed(FailureKind::Authentication | FailureKind::Permission) => 4,
        ExitClass::Failed(FailureKind::Execution) => 5,
        ExitClass::Incomplete => 6,
        ExitClass::Failed(FailureKind::RateLimited) => 7,
    }
}

pub(super) fn write_text(text: &str) -> u8 {
    let text = text.strip_suffix('\n').unwrap_or(text);
    match writeln!(io::stdout().lock(), "{text}") {
        Ok(()) => 0,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => 0,
        Err(_) => 5,
    }
}

pub(super) fn write_json(value: &Value, compact: bool) -> u8 {
    let text = if compact {
        serde_json::to_string(value)
    } else {
        serde_json::to_string_pretty(value)
    };
    match text {
        Ok(text) => match writeln!(io::stdout().lock(), "{text}") {
            Ok(()) => 0,
            Err(error) if error.kind() == io::ErrorKind::BrokenPipe => 0,
            Err(_) => 5,
        },
        Err(_) => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        INTERACTIVE_EXECUTION_TIMEOUT_SECS, parse_args_from, runtime_error_output,
        runtime_error_text,
    };
    use serde_json::json;

    /// M5: after the first Ctrl-C cancels, a second Ctrl-C must stop waiting
    /// for work that ignores cancellation; without one, the drain completes.
    #[tokio::test]
    async fn second_interrupt_stops_waiting_for_uncooperative_work() {
        use super::{Interrupted, drain_after_interrupt};
        let forced =
            drain_after_interrupt(std::future::pending::<()>(), std::future::ready(())).await;
        assert_eq!(forced, Interrupted::Forced);
        let drained =
            drain_after_interrupt(std::future::ready(()), std::future::pending::<()>()).await;
        assert_eq!(drained, Interrupted::Drained);
    }

    #[test]
    fn runtime_errors_are_secret_scrubbed_on_stdout() {
        let token = format!("ghp_{}", "a".repeat(37));
        let error = |payload: Option<serde_json::Value>| octocode_native::runtime::RuntimeError {
            code: "providerError".into(),
            message: format!("upstream rejected {token}"),
            payload: payload.map(Box::new),
            validation_issues: None,
        };
        let envelope = runtime_error_output(error(None)).to_string();
        assert!(
            !envelope.contains("ghp_") && envelope.contains("[REDACTED-"),
            "{envelope}"
        );
        let payload = runtime_error_output(error(Some(json!({"detail": [token.clone()]}))));
        assert!(!payload.to_string().contains("ghp_"), "{payload}");
    }

    /// The terminal channel prints the same repair details as the JSON
    /// payload (a typo names the field it meant), and a timeout still tells
    /// the caller to retry.
    #[test]
    fn terminal_errors_carry_the_repair_details() {
        use octocode_native::contracts;
        let tool = "localFetch";
        let error = contracts::prepare_many_and_validate(
            tool,
            json!({"queries":[{"path":"a.rs","matchstring":"x"}]}),
        )
        .expect_err("invalid input");
        let invalid = octocode_native::runtime::RuntimeError {
            code: "invalidInput".into(),
            message: error.to_string(),
            payload: Some(Box::new(contracts::format_input_error(tool, &error, false))),
            validation_issues: None,
        };
        let envelope = runtime_error_output(invalid);
        assert_eq!(envelope["kind"], "octocode.toolError", "{envelope}");
        assert_eq!(envelope["errorCode"], "invalidInput", "{envelope}");
        let text = runtime_error_text(&envelope);
        assert!(text.starts_with("Unknown field(s): matchstring"), "{text}");
        assert!(text.contains("did you mean 'matchString'?"), "{text}");
        assert!(text.contains("octocode schema localFetch"), "{text}");

        let timeout = octocode_native::runtime::RuntimeError {
            code: "timeout".into(),
            message: "Tool call timed out.".into(),
            payload: None,
            validation_issues: None,
        };
        let text = runtime_error_text(&runtime_error_output(timeout));
        assert!(text.starts_with("Tool call timed out."), "{text}");
        assert!(text.contains("initialises the language server"), "{text}");
    }

    #[test]
    fn every_contract_tool_parses_as_a_tool_command() {
        use clap::CommandFactory;
        super::Args::command().debug_assert();
        let contract = octocode_native::contracts::parsed_contract().expect("contract");
        for tool in contract["tools"].as_array().expect("tools") {
            let name = tool["name"].as_str().expect("name");
            let args = parse_args_from(["octocode", name, "{}", "--json"]).expect(name);
            match args.command {
                super::Command::Tool(command) => {
                    assert_eq!(command.name, name);
                    assert_eq!(command.args.query.as_deref(), Some("{}"));
                    assert!(command.args.json);
                }
                _ => panic!("{name} did not parse as a tool"),
            }
        }
    }

    #[test]
    fn argument_errors_exit_two_in_either_output_mode() {
        assert_eq!(
            parse_args_from(["octocode", "notACommand", "--json"]).err(),
            Some(2)
        );
        assert_eq!(parse_args_from(["octocode", "notACommand"]).err(), Some(2));
        assert!(parse_args_from(["octocode", "schema", "localSearch", "--view", "query"]).is_ok());
    }

    #[test]
    fn removed_aliases_do_not_parse() {
        for argv in [
            vec!["octocode", "scheme"],
            vec!["octocode", "showConfig"],
            vec!["octocode", "--json-errors", "catalog"],
            vec!["octocode", "--redact-emails", "catalog"],
            vec!["octocode", "--no-color", "catalog"],
            vec!["octocode", "localSearch", "{}", "--pretty"],
            vec!["octocode", "config", "--add", "K", "V"],
            vec!["octocode", "auth", "--json"],
        ] {
            assert_eq!(parse_args_from(argv.clone()).err(), Some(2), "{argv:?}");
        }
    }

    #[test]
    fn tool_input_reads_a_file_and_rejects_bad_json() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("q.json");
        std::fs::write(&path, r#"{"queries":[]}"#).expect("write");
        let args = super::ToolArgs {
            query: None,
            input: Some(path),
            json: true,
        };
        assert_eq!(
            super::tool_input("localSearch", &args, true),
            Ok(json!({"queries": []}))
        );
        let bad = super::ToolArgs {
            query: Some("{".into()),
            input: None,
            json: true,
        };
        assert_eq!(super::tool_input("localSearch", &bad, true), Err(2));
        let missing = super::ToolArgs {
            query: None,
            input: None,
            json: true,
        };
        assert_eq!(super::tool_input("localSearch", &missing, true), Err(2));
    }

    #[test]
    fn execution_deadline_exceeds_the_worst_cold_lsp_budget() {
        const {
            assert!(
                INTERACTIVE_EXECUTION_TIMEOUT_SECS * 1_000
                    > octocode_engine::lsp::MAX_COLD_LSP_EXECUTION_BUDGET_MS
            );
        }
    }
}
