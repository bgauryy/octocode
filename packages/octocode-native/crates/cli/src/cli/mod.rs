mod commands;
mod config;
mod graph;
mod lsp_provision;
mod mcp_install;
mod schema;
mod skill;
mod system;
use clap::{CommandFactory, FromArgMatches, Parser};
use commands::{AuthCommand, Command, ToolArgs};
use octocode_native::config::RuntimeSurface;
use octocode_native::runtime::{HostOptions, ToolRuntime};
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
  octocode scheme                   list every tool and its availability\n\n\
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
    let command = if argv.iter().any(|arg| arg == "--no-color") {
        Args::command().color(clap::ColorChoice::Never)
    } else {
        Args::command()
    };
    match command
        .try_get_matches_from(&argv)
        .and_then(|matches| Args::from_arg_matches(&matches))
    {
        Ok(args) => Ok(args),
        Err(error) => {
            let json_errors = argv.iter().any(|arg| arg == "--json-errors");
            let displays_text = matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp
                    | clap::error::ErrorKind::DisplayVersion
                    | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            );
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

/// Config paths that include or exclude tools by name (`tools.enabled` /
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
                    let short_description = tool
                        .get("shortDescription")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
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
            check,
            add,
            value_stdin,
            remove,
            json,
        } => {
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
        Command::Skill { args } => skill::skill(runtime, &args),
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
            backup,
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
            backup,
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
        } => lsp_provision::run(&action, names, all, yes, force, json).await,
    }
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

/// Execute one tool call and print its structured JSON result to stdout.
///
/// Exit codes mirror the response: 0 success, 6 when the response carries a
/// re-runnable `next.*` continuation or a partial source read, and the typed
/// failure codes otherwise.
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
    match result {
        Ok(outcome) => {
            let value = outcome.structured_content;
            let mut exit = outcome.failure.map_or(0, failure_exit);
            if outcome.all_failed {
                // A whole-call failure that carries no runtime FailureKind (e.g. a
                // config/gate refusal or admission-time validation) must not read
                // as success — classify it as a usage/input error rather than 0.
                // Rows that all reject the caller's input are exit 2 as well,
                // not an execution failure.
                if let Some(code) = clasify_failure_exit(&value, outcome.failure) {
                    exit = code;
                } else if exit == 0 || (exit == 5 && all_rows_invalid_input(&value)) {
                    exit = 2;
                }
            } else {
                // A bulk result where some rows succeeded is not a total
                // failure, but a partial source read or an available
                // continuation is still exit 6. A nested/informational partial
                // with no continuation and no source content (e.g. a reasoning
                // tool's coverage `truncated`) stays 0.
                let rows: Vec<&Value> = value["results"]
                    .as_array()
                    .map(|array| array.iter().collect())
                    .unwrap_or_default();
                let all_empty = !rows.is_empty()
                    && rows
                        .iter()
                        .all(|row| row.get("status").and_then(Value::as_str) == Some("empty"));
                let has_continuation = rows.iter().any(|row| has_cli_continuation(row))
                    || has_clasify_continuation(&value)
                    || value.pointer("/responsePagination/hasMore") == Some(&Value::Bool(true));
                // Empty (exit 1) takes precedence over a corrective continuation:
                // an empty result with a recovery next.* is still "empty", not
                // "more pages" (exit 6, reserved for results + continuation).
                // A batch row rejected by input validation is a caller error
                // even when sibling rows succeeded (row isolation).
                let rejected_row = rows.iter().any(|row| is_invalid_input_row(row));
                exit = if rejected_row {
                    2
                } else if all_empty {
                    1
                } else if has_continuation {
                    6
                } else {
                    0
                };
            }
            let code = write_json(&value, compact);
            if code != 0 {
                return code;
            }
            exit
        }
        Err(error) => {
            if let Some(payload) = error.payload {
                write_json(&payload, compact);
            } else {
                // Emit a structured JSON error to stdout so callers can parse it.
                // Previously this went to stderr only, causing silent empty output
                // (e.g. lspSearch timeout on cold start when stderr is discarded).
                let hint = if error.code == "timeout" {
                    Some(
                        "Retry -- the first call initialises the language server (~60 s cold start).",
                    )
                } else {
                    None
                };
                let mut value = json!({
                    "error": error.message,
                    "errorCode": error.code,
                });
                if let Some(hint) = hint {
                    value["hints"] = json!([hint]);
                }
                write_json(&value, compact);
            }
            if error.code == "invalidInput" { 2 } else { 5 }
        }
    }
}

fn failure_exit(failure: octocode_native::runtime::FailureKind) -> u8 {
    use octocode_native::runtime::FailureKind;
    match failure {
        FailureKind::NotFound => 3,
        FailureKind::Authentication | FailureKind::Permission => 4,
        FailureKind::RateLimited => 7,
        FailureKind::Execution => 5,
    }
}

/// Exit code for a clasify call in which every resource errored (nothing was
/// judged): the caller's request (2) only when every error rejects it; when
/// every delegated read failed alike, that read tool's exit (`failure`, e.g. 3
/// for a missing file); missing sources (3), throttling (7), otherwise an
/// execution/provider failure (5).
fn clasify_failure_exit(
    value: &Value,
    failure: Option<octocode_native::runtime::FailureKind>,
) -> Option<u8> {
    let queries = value["queries"].as_array().filter(|q| !q.is_empty())?;
    let mut codes = Vec::new();
    for query in queries {
        if let Some(code) = query.pointer("/error/code").and_then(Value::as_str) {
            codes.push(code.to_owned());
        }
        for resource in query["resources"].as_array().into_iter().flatten() {
            // A compact resource with one plain page carries its `error` or
            // `answers` itself; it then has no `pages`.
            let own = std::iter::once(resource).filter(|resource| resource.get("pages").is_none());
            for page in resource["pages"]
                .as_array()
                .into_iter()
                .flatten()
                .chain(own)
            {
                if let Some(code) = page.pointer("/error/code").and_then(Value::as_str) {
                    codes.push(code.to_owned());
                }
                for answer in page["answers"]
                    .as_object()
                    .into_iter()
                    .flat_map(|a| a.values())
                {
                    if let Some(code) = answer.pointer("/error/code").and_then(Value::as_str) {
                        codes.push(code.to_owned());
                    }
                }
            }
        }
    }
    if codes.is_empty() {
        return None;
    }
    let all = |test: fn(&str) -> bool| codes.iter().all(|code| test(code));
    Some(if all(is_clasify_caller_code) {
        2
    } else if let Some(failure) = failure {
        failure_exit(failure)
    } else if all(octocode_native::runtime::response::is_not_found_code) {
        3
    } else if all(|code| matches!(code, "classificationRateLimited" | "rateLimited")) {
        7
    } else {
        5
    })
}

/// clasify error codes that reject the caller's request rather than report a
/// failed read or provider call.
fn is_clasify_caller_code(code: &str) -> bool {
    octocode_native::runtime::response::is_invalid_input_code(code)
        || matches!(
            code,
            "invalidClassificationContext"
                | "invalidClassificationRequest"
                | "classificationLocateUnsupported"
                | "classificationExpandedCellsExceeded"
                | "pathOutsideAllowedRoots"
                | "pathValidationFailed"
        )
}

/// An error row whose `errorCode` rejects the caller's input.
fn is_invalid_input_row(row: &Value) -> bool {
    row.get("status").and_then(Value::as_str) == Some("error")
        && row
            .pointer("/data/errorCode")
            .and_then(Value::as_str)
            .is_some_and(octocode_native::runtime::response::is_invalid_input_code)
}

fn all_rows_invalid_input(value: &Value) -> bool {
    value["results"]
        .as_array()
        .is_some_and(|rows| !rows.is_empty() && rows.iter().all(is_invalid_input_row))
}

/// clasify returns `queries[].next.clasify` (a complete query, not a
/// `{tool,query}` row continuation); remaining coverage is still exit 6.
fn has_clasify_continuation(value: &Value) -> bool {
    value["queries"].as_array().is_some_and(|queries| {
        queries.iter().any(|query| {
            query
                .get("next")
                .and_then(|next| next.get(ToolId::Clasify.as_str()))
                .is_some_and(Value::is_object)
        })
    })
}

/// A row with more of its result remaining: an open `next.*` page/resume
/// call (the runtime's continuation-name set) or a partial source read.
fn has_cli_continuation(row: &Value) -> bool {
    use octocode_native::runtime::response::{continuation, is_remaining_continuation_name};
    // A row that declares `complete:true` has nothing left to page; any
    // `next.*` it carries (e.g. astSearch `expandCaptures`) is a drill-down.
    let complete = row.pointer("/data/complete") == Some(&Value::Bool(true));
    (!complete
        && (row
            .pointer("/data/next")
            .and_then(Value::as_object)
            .is_some_and(|calls| {
                calls
                    .keys()
                    .any(|name| is_remaining_continuation_name(name))
            })
            || continuation(&row["data"], &is_remaining_continuation_name)))
        || (octocode_native::runtime::response::is_partial(&row["data"])
            && row["data"]["content"]
                .as_str()
                .is_some_and(|text| !text.is_empty()))
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
        INTERACTIVE_EXECUTION_TIMEOUT_SECS, all_rows_invalid_input, clasify_failure_exit,
        error_envelope, has_clasify_continuation, has_cli_continuation, parse_args_from,
    };
    use octocode_native::runtime::FailureKind;
    use serde_json::json;

    #[test]
    fn clasify_whose_every_read_failed_exits_like_that_read() {
        let failed = |code: &str| {
            json!({"queries":[{"queryId":"q","resources":[{"resourceId":"x","coverage":"error",
                "pages":[{"error":{"code":code,"message":"m"}}]}]}]})
        };
        // localFetch reports a missing file as fileAccessFailed + NotFound.
        let missing = failed("fileAccessFailed");
        assert_eq!(
            clasify_failure_exit(&missing, Some(FailureKind::NotFound)),
            Some(3)
        );
        assert_eq!(clasify_failure_exit(&missing, None), Some(5));
        // Compact output: a single failed page is stated on the resource.
        let compact = |code: &str| {
            json!({"queries":[{"queryId":"q","resources":[{"resourceId":"x","coverage":"error",
                "answers":{"q":{"error":{"code":code,"message":"m"}}}}]}]})
        };
        assert_eq!(
            clasify_failure_exit(&compact("classificationProviderError"), None),
            Some(5)
        );
        assert_eq!(
            clasify_failure_exit(&compact("pathNotFound"), None),
            Some(3)
        );
        assert_eq!(clasify_failure_exit(&failed("pathNotFound"), None), Some(3));
        assert_eq!(
            clasify_failure_exit(&failed("rateLimited"), Some(FailureKind::RateLimited)),
            Some(7)
        );
        assert_eq!(
            clasify_failure_exit(
                &failed("classificationLocateUnsupported"),
                Some(FailureKind::NotFound)
            ),
            Some(2),
            "a rejected request stays a caller error"
        );
        assert_eq!(clasify_failure_exit(&json!({"queries":[]}), None), None);
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
    fn optional_drill_downs_are_not_remaining_pages() {
        let call = json!({"tool":"ghGetHistoryItem","query":{"number":1}});
        let menu = json!({"data":{"next":{"getBody":call,"readPr":call,"verifyReferences":call}}});
        assert!(!has_cli_continuation(&menu));
        for name in ["nextPage", "continue", "expandLimit", "retry"] {
            let row = json!({"data":{"next":{name:call}}});
            assert!(has_cli_continuation(&row), "{name}");
        }
        let nested = json!({"data":{"nestedEvidence":{"next":{"nextPage":call}}}});
        assert!(has_cli_continuation(&nested));
    }

    #[test]
    fn complete_rows_with_only_drill_downs_are_not_partial() {
        let call = json!({"tool":"astSearch","query":{"captureText":true}});
        let complete = json!({"data":{"complete":true,"next":{"expandCaptures":call}}});
        assert!(!has_cli_continuation(&complete));
        let open = json!({"data":{"complete":false,"next":{"expandCaptures":call}}});
        assert!(has_cli_continuation(&open));
    }

    #[test]
    fn rows_rejecting_caller_input_are_invalid_input() {
        let rows = json!({"results":[
            {"status":"error","data":{"errorCode":"invalidRegex"}},
            {"status":"error","data":{"errorCode":"validation"}}
        ]});
        assert!(all_rows_invalid_input(&rows));
        let mixed = json!({"results":[
            {"status":"error","data":{"errorCode":"invalidRegex"}},
            {"status":"error","data":{"errorCode":"fileAccessFailed"}}
        ]});
        assert!(!all_rows_invalid_input(&mixed));
        assert!(!all_rows_invalid_input(&json!({"results":[]})));
    }

    #[test]
    fn clasify_remaining_coverage_is_partial_cli_output() {
        let pending = json!({"queries":[
            {"queryId":"a","results":[]},
            {"queryId":"b","results":[],"next":{"clasify":{"id":"b","resources":[]}}}
        ]});
        assert!(has_clasify_continuation(&pending));
        assert!(!has_clasify_continuation(
            &json!({"queries":[{"queryId":"a","results":[]}]})
        ));
    }

    #[test]
    fn nested_executable_continuation_is_classified_as_partial_cli_output() {
        let row = json!({
            "data": {
                "files": [{
                    "path": "src/lib.rs",
                    "isPartial": true,
                    "next": {
                        "continue": {
                            "tool": "ghGetFileContent",
                            "query": {"owner":"a","repo":"b","path":"src/lib.rs","charOffset":64}
                        }
                    }
                }]
            }
        });

        assert!(has_cli_continuation(&row));
    }

    #[test]
    fn informational_nested_partial_without_executable_next_stays_success() {
        let row = json!({
            "data": {
                "pages": [{"isPartial": true, "coverage": "partial"}]
            }
        });

        assert!(!has_cli_continuation(&row));
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
