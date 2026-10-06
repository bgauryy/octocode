mod commands;
mod config;
mod config_view;
mod graph;
mod lsp_provision;
mod mcp_clients;
mod mcp_install;
mod mcp_manage;
mod schema;
mod skill;
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
    // Keep in sync with the "Exit codes" table in packages/octocode-native/README.md.
    long_about = "Native Octocode research tools.\n\n\
Every tool is called by its canonical name with a raw JSON query:\n\
  octocode <toolName> '<json>'      execute a tool\n\
  octocode scheme <toolName>        print the tool's contract\n\
  octocode scheme                   list enabled tools\n\n\
Code graph (persisted; see `octocode graph --help`):\n\
  octocode graph ingest <path>      parse once into <workspace>/.octocode/graph\n\
  octocode graph query <op> [ref]   callers, impact, cycles, issues, ... in milliseconds\n\n\
EXIT CODES:\n\
  0    Success\n\
  1    Empty result / no matches\n\
  2    Invalid input, including any rejected batch row (also clap argument errors)\n\
  3    Not found\n\
  4    Auth required\n\
  5    Execution error\n\
  6    Partial result - the response carries a re-runnable next.* continuation\n\
  7    Rate limited\n\
  130  Interrupted (Ctrl-C)"
)]
pub struct Args {
    /// Emit {"kind":"octocode.toolError","version":1,"error":"..."} to stdout on
    /// errors (including argument parse errors) instead of stderr text.
    #[arg(long, global = true)]
    json_errors: bool,
    /// Mask email addresses in GitHub tool outputs (same as OCTOCODE_REDACT_EMAILS=true).
    #[arg(long, global = true)]
    redact_emails: bool,
    /// Disable ANSI color (also available through NO_COLOR).
    #[arg(long, global = true)]
    no_color: bool,
    #[command(subcommand)]
    command: Command,
}

/// The one `--json-errors` envelope, shared with contract input errors
/// (`contracts::validate`) so callers parse a single shape.
fn error_envelope(tool: Option<&str>, msg: &str) -> Value {
    let mut value = json!({"kind": "octocode.toolError", "version": 1, "error": msg});
    if let Some(tool) = tool {
        value["tool"] = json!(tool);
    }
    value
}

fn emit_error(msg: &str, json_errors: bool) {
    emit_tool_error(None, msg, json_errors);
}

fn emit_tool_error(tool: Option<&str>, msg: &str, json_errors: bool) {
    if json_errors {
        println!("{}", error_envelope(tool, msg));
    } else {
        eprintln!("{msg}");
    }
}

/// Parse argv; with `--json-errors`, argument errors use the JSON envelope
/// (exit 2) instead of clap's text. Help and version output stay text.
pub fn parse_args() -> Result<Args, u8> {
    parse_args_from(std::env::args_os())
}

fn parse_args_from<I, T>(argv: I) -> Result<Args, u8>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let argv: Vec<std::ffi::OsString> = argv.into_iter().map(Into::into).collect();
    let command = || {
        if argv.iter().any(|arg| arg == "--no-color") {
            Args::command().color(clap::ColorChoice::Never)
        } else {
            Args::command()
        }
    };
    match command()
        .try_get_matches_from(&argv)
        .and_then(|matches| Args::from_arg_matches(&matches))
    {
        Ok(args) => Ok(args),
        Err(error) => {
            let error = if is_help(&error) {
                hide_unavailable_tools(command())
                    .try_get_matches_from(&argv)
                    .err()
                    .unwrap_or(error)
            } else {
                error
            };
            let json_errors = argv.iter().any(|arg| arg == "--json-errors");
            let displays_text =
                is_help(&error) || error.kind() == clap::error::ErrorKind::DisplayVersion;
            if !json_errors || displays_text {
                let _ = error.print();
                return Err(error.exit_code() as u8);
            }
            let rendered = error.render().to_string();
            let message = rendered
                .lines()
                .next()
                .unwrap_or_default()
                .trim_start_matches("error: ")
                .to_owned();
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

/// Compact field list for `scheme` rows. Union tools (`anyOf`/`oneOf`) list
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
/// `tools.disabled` / `tools.family`), from the config contract.
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
                    let short_description = tool
                        .get("shortDescription")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let enabled = tool
                        .get("available")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let mut availability = json!({ "enabled": enabled });
                    // Tool lists and the family preset are both `tools.*` config.
                    let tool_list_excluded = matches!(
                        tool.get("unavailableReason").and_then(Value::as_str),
                        Some("toolsList" | "family")
                    );
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
                        "description": short_description,
                        "fields": fields,
                        "availability": availability
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({
        "kind": "octocode.toolCatalog",
        "version": 1,
        "toolCount": tools.len(),
        "output": "Machine tool catalog. Descriptions, examples, and workflow instructions ship with the octocode npm launcher and MCP server.",
        "commands": {
            "schema": "scheme <name>",
            "querySchema": "scheme <name> --view query",
            "run": "<name> '<json>'"
        },
        "fingerprint": catalog["fingerprint"],
        "grammarCapabilities": catalog["grammarCapabilities"],
        "tools": tools
    })
}

pub async fn run(args: Args) -> u8 {
    let json_errors = args.json_errors;
    if let Command::Config {
        command:
            Some(ConfigCommand::View {
                no_open,
                idle_timeout,
            }),
        ..
    } = &args.command
    {
        return config_view::run(*no_open, *idle_timeout);
    }
    if args.redact_emails {
        // Single-threaded startup; the config resolver reads the process env,
        // so the flag is just the env spelling set before runtime creation.
        unsafe { std::env::set_var("OCTOCODE_REDACT_EMAILS", "true") };
    }
    let runtime = match ToolRuntime::from_host(HostOptions {
        surface: RuntimeSurface::Cli,
        // Keep the outer execution budget above the worst configured cold start
        // plus one logical request: initialize, readiness, retries, and delays.
        timeout_secs: Some(INTERACTIVE_EXECUTION_TIMEOUT_SECS),
        ..HostOptions::default()
    }) {
        Ok(runtime) => runtime,
        Err(error) => {
            emit_error(&format!("{}: {}", error.code, error.message), json_errors);
            return 5;
        }
    };
    let result = dispatch(args.command, json_errors, &runtime).await;
    runtime.close().await;
    result
}

async fn dispatch(command: Command, json_errors: bool, runtime: &ToolRuntime) -> u8 {
    match command {
        Command::Tool(tool) => run_tool(runtime, tool.name, tool.args, json_errors).await,
        Command::Scheme {
            tool,
            view,
            select,
            compact,
        } => {
            let catalog = match runtime.catalog() {
                Ok(catalog) => catalog,
                Err(error) => {
                    emit_error(&error.message, json_errors);
                    return 5;
                }
            };
            // Availability comes from the runtime catalog; schemas come from
            // the embedded enforcement contract (the catalog carries neither).
            let contract = match octocode_native::contracts::parsed_contract() {
                Ok(contract) => contract,
                Err(_) => {
                    emit_error("Embedded contract is invalid", json_errors);
                    return 5;
                }
            };
            let Some(name) = tool else {
                return write_json(
                    &compact_tool_catalog(&catalog, contract, &runtime.config().dotenv),
                    compact,
                );
            };
            let value = contract["tools"]
                .as_array()
                .and_then(|tools| tools.iter().find(|tool| tool["name"] == *name))
                .cloned();
            let Some(value) = value else {
                let known = contract["tools"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|tool| tool["name"].as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                emit_error(
                    &format!("Unknown tool: {name}. Known tools: {known}"),
                    json_errors,
                );
                return 2;
            };
            match schema::project_selected(value, view.unwrap_or_default(), select.as_deref()) {
                Ok(mut value) => {
                    // The compact catalog carries a generic `run` hint; the
                    // per-tool view echoes the concrete invocation so an agent
                    // inspecting one contract sees exactly how to execute it.
                    value["run"] = Value::String(format!("octocode {name} '<json>'"));
                    write_json(&value, compact)
                }
                Err(error) => {
                    emit_error(&error, json_errors);
                    2
                }
            }
        }
        Command::ShowConfig { json } => config::show_path(runtime, json),
        Command::Config {
            // `config view`, the only subcommand, returns from `run` before
            // the runtime starts.
            command: _,
            manage,
            check,
            add,
            value_stdin,
            remove,
            json,
        } => {
            if manage {
                return config_management(runtime);
            }
            if !add.is_empty() || remove.is_some() {
                return config::edit(
                    runtime,
                    &add,
                    remove.as_deref(),
                    value_stdin,
                    json || json_errors,
                );
            }
            let view = runtime.inspect_config();
            if let Some(key) = check {
                let set = runtime
                    .config()
                    .env_value(&key)
                    .is_some_and(|value| !value.is_empty());
                if json {
                    let code = write_json(&json!({"key": key, "set": set}), true);
                    if code != 0 {
                        return code;
                    }
                } else {
                    println!("{key}: {}", if set { "set" } else { "unset" });
                }
                return if set { 0 } else { 1 };
            }
            let config_file = view
                .config_path
                .clone()
                .unwrap_or_else(|| view.home.join(".octocoderc"));
            let config_file_exists = view.config_path.is_some();
            let project_config_exists = view.project_config_path.is_some();
            if json {
                return write_json(
                    &json!({
                        "home": view.home,
                        "storage": view.storage_mode,
                        "configFile": {
                            "path": config_file,
                            "exists": config_file_exists,
                            "keys": view.config_keys,
                        },
                        "projectConfigFile": {
                            "path": view.project_config_file,
                            "exists": project_config_exists,
                            "keys": view.project_config_keys,
                        },
                        "envFiles": {
                            "global": view.global_env_path,
                            "project": view.project_env_path,
                        },
                        "envKeys": view.loaded_keys,
                        "skippedProtected": view.skipped_protected,
                        "skippedExisting": view.skipped_existing,
                        "diagnostics": view.diagnostics,
                        "note": "Key names only; values are never printed.",
                    }),
                    true,
                );
            }
            println!("home:    {}", view.home.display());
            println!("storage: {}", view.storage_mode);
            println!(
                "config file: {}{}",
                config_file.display(),
                if config_file_exists {
                    ""
                } else {
                    " (not found)"
                }
            );
            println!(
                "project config file: {}{}",
                view.project_config_file.display(),
                if project_config_exists {
                    ""
                } else {
                    " (not found)"
                }
            );
            println!("env files:");
            println!("  global:  {}", view.global_env_path.display());
            println!("  project: {}", view.project_env_path.display());
            if !view.config_keys.is_empty() {
                println!("config keys ({}):", view.config_keys.len());
                for key in &view.config_keys {
                    println!("  {key}");
                }
            }
            if !view.project_config_keys.is_empty() {
                println!("project config keys ({}):", view.project_config_keys.len());
                for key in &view.project_config_keys {
                    println!("  {key}");
                }
            }
            if !view.loaded_keys.is_empty() {
                println!("env keys ({}):", view.loaded_keys.len());
                for key in &view.loaded_keys {
                    println!("  {key}");
                }
            }
            println!("(key names only; values are never printed)");
            for skip in &view.skipped_protected {
                println!(
                    "skipped (protected): {} — found in {} but not applied; set it in the process environment or config file",
                    skip.key,
                    skip.source_path.display()
                );
            }
            for skip in &view.skipped_existing {
                println!(
                    "skipped (existing): {} — found in {} but the process environment already sets it",
                    skip.key,
                    skip.source_path.display()
                );
            }
            // Diagnostics were already printed to stderr at runtime start.
            0
        }
        Command::Auth { command, json } => match command {
            None => system::auth_status(runtime, json).await,
            Some(AuthCommand::Status { json: sub_json }) => {
                system::auth_status(runtime, json || sub_json).await
            }
            Some(AuthCommand::Login {
                hostname,
                force,
                refresh,
                json,
            }) => system::login(runtime, hostname.as_deref(), force, refresh, json).await,
            Some(AuthCommand::Logout) => system::logout(runtime),
        },
        Command::Graph { command } => graph::graph(runtime, command),
        Command::Skill { args } => skill::skill(&args),
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
        Command::Cache { action } => system::cache(runtime, &action),
        Command::LspServer {
            action,
            names,
            all,
            yes,
            force,
            json,
        } => lsp_provision::run(runtime, &action, names, all, yes, force, json).await,
    }
}

fn config_management(runtime: &ToolRuntime) -> u8 {
    use std::io::Read;
    let mut request = Vec::new();
    let result = io::stdin()
        .take(131_073)
        .read_to_end(&mut request)
        .map_err(|_| config::ManageError::from("Cannot read management request."))
        .and_then(|_| config_management_response(runtime, &request));
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
    if bytes.len() > 131_072 {
        return Err("Management request exceeds 128 KiB.".into());
    }
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

async fn run_tool(runtime: &ToolRuntime, tool: &str, args: ToolArgs, json_errors: bool) -> u8 {
    let query_text = match args.query_text() {
        Ok(text) => text,
        Err(error) => {
            emit_tool_error(Some(tool), &error, json_errors);
            return 2;
        }
    };
    let Some(query_text) = query_text else {
        eprintln!("Usage: octocode {tool} '<json>'");
        eprintln!("       octocode {tool} --input <file>");
        eprintln!("Schema: octocode scheme {tool}");
        return 2;
    };
    let input = match serde_json::from_str::<Value>(&query_text) {
        Ok(input) => input,
        Err(parse_error) => {
            emit_tool_error(
                Some(tool),
                &format!("Invalid JSON query: {parse_error}"),
                json_errors,
            );
            return 2;
        }
    };
    execute(runtime, tool, input, !args.pretty).await
}

/// Execute one tool call and print its structured JSON result to stdout,
/// exiting with the code of the runtime's [`ExitClass`].
pub(super) async fn execute(runtime: &ToolRuntime, tool: &str, input: Value, compact: bool) -> u8 {
    let execution = runtime.execute("cli-1".into(), tool.into(), input);
    tokio::pin!(execution);
    let result = tokio::select! {
        result = &mut execution => result,
        signal = tokio::signal::ctrl_c() => {
            if signal.is_ok() { runtime.requests.cancel("cli-1"); let _ = execution.await; return 130; }
            execution.await
        }
    };
    let (value, class) = match result {
        Ok(outcome) => {
            let class = outcome.exit_class();
            (outcome.structured_content, class)
        }
        Err(error) => {
            let class = error.exit_class();
            (runtime_error_output(error), class)
        }
    };
    match write_json(&value, compact) {
        0 => exit_code(class),
        code => code,
    }
}

/// The stdout JSON for a runtime error: its structured payload, else an
/// `{error, errorCode}` envelope (stdout, so a caller discarding stderr still
/// sees why the call failed). Every string is secret-scrubbed, as at the
/// N-API boundary.
fn runtime_error_output(error: octocode_native::runtime::RuntimeError) -> Value {
    let mut value = match error.payload {
        Some(payload) => *payload,
        None => {
            let mut value = json!({ "error": error.message, "errorCode": error.code });
            if error.code == "timeout" {
                value["hints"] = json!([
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
        INTERACTIVE_EXECUTION_TIMEOUT_SECS, error_envelope, parse_args_from, runtime_error_output,
    };
    use serde_json::json;

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

    #[test]
    fn json_error_envelope_matches_contract_tool_errors() {
        assert_eq!(
            error_envelope(Some("localSearch"), "bad"),
            json!({"kind":"octocode.toolError","version":1,"tool":"localSearch","error":"bad"})
        );
        assert!(error_envelope(None, "bad").get("tool").is_none());
    }

    #[test]
    fn every_contract_tool_parses_as_a_tool_command() {
        use clap::CommandFactory;
        super::Args::command().debug_assert();
        let contract = octocode_native::contracts::parsed_contract().expect("contract");
        for tool in contract["tools"].as_array().expect("tools") {
            let name = tool["name"].as_str().expect("name");
            let args = parse_args_from(["octocode", name, "{}", "--pretty"]).expect(name);
            match args.command {
                super::Command::Tool(command) => {
                    assert_eq!(command.name, name);
                    assert_eq!(command.args.query.as_deref(), Some("{}"));
                    assert!(command.args.pretty);
                }
                _ => panic!("{name} did not parse as a tool"),
            }
        }
    }

    #[test]
    fn argument_errors_keep_exit_two_with_or_without_json_errors() {
        assert_eq!(
            parse_args_from(["octocode", "--json-errors", "notACommand"]).err(),
            Some(2)
        );
        assert_eq!(parse_args_from(["octocode", "notACommand"]).err(), Some(2));
        assert!(parse_args_from(["octocode", "--json-errors", "scheme"]).is_ok());
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
