mod commands;
mod lsp_provision;
mod mcp_install;
mod schema;
mod skill;
mod system;
use clap::Parser;
use commands::{AuthCommand, Command, ToolArgs};
use octocode_native::config::RuntimeSurface;
use octocode_native::runtime::{HostOptions, ToolRuntime};
use serde_json::{Value, json};
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
  octocode scheme                   list every tool with availability\n\n\
EXIT CODES:\n\
  0    Success\n\
  1    Empty result / no matches\n\
  2    Invalid input (also clap argument errors)\n\
  3    Not found\n\
  4    Auth required\n\
  5    Execution error\n\
  6    Partial result - the response carries a re-runnable next.* continuation\n\
  7    Rate limited\n\
  130  Interrupted (Ctrl-C)"
)]
pub struct Args {
    /// Emit {"success":false,"error":"..."} to stdout on errors instead of stderr text.
    #[arg(long, global = true)]
    json_errors: bool,
    /// Mask email addresses in GitHub tool outputs (same as OCTOCODE_REDACT_EMAILS=true).
    #[arg(long, global = true)]
    redact_emails: bool,
    #[command(subcommand)]
    command: Command,
}

fn emit_error(msg: &str, json_errors: bool) {
    if json_errors {
        println!("{}", json!({"success": false, "error": msg}));
    } else {
        eprintln!("{msg}");
    }
}

/// Group tool names into catalog families.
fn tool_family(name: &str) -> &'static str {
    match name {
        "ghSearch" | "ghGetFileContent" | "ghSearchHistory" | "ghGetHistoryItem"
        | "ghCloneRepo" => "GitHub",
        "localSearch" | "localFetch" | "astSearch" | "astRewrite" | "lspSearch" => "Local Code",
        "artifactSearch" => "Package",
        "jev" => "Reasoning",
        _ => "Other",
    }
}

fn compact_description(description: &str, max_chars: usize) -> String {
    let mut chars = description.chars();
    let prefix: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{}…", prefix.trim_end_matches([' ', '.', ',']))
    } else {
        prefix
    }
}

fn compact_fields(tool: &Value) -> String {
    let schema = tool
        .get("querySchema")
        .and_then(|schema| schema.get("anyOf"))
        .and_then(Value::as_array)
        .and_then(|variants| variants.first())
        .unwrap_or_else(|| tool.get("querySchema").unwrap_or(&Value::Null));
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
    let mut names = properties.keys().map(String::as_str).collect::<Vec<_>>();
    names.sort_by_key(|name| (!required.contains(name), *name));
    const MAX_FIELDS: usize = 8;
    let truncated = names.len() > MAX_FIELDS;
    let mut fields = names
        .into_iter()
        .take(MAX_FIELDS)
        .map(|name| format!("{name}{}", if required.contains(name) { "*" } else { "?" }))
        .collect::<Vec<_>>();
    if truncated {
        fields.push("…".to_owned());
    }
    format!("[{}]", fields.join(", "))
}

fn availability_env_var(name: &str) -> Option<&'static str> {
    match name {
        // Canonical names; `OCTOCODE_ENABLE_CLONE`/`OCTOCODE_ENABLE_LOCAL`
        // are accepted aliases (config/resolver.rs).
        "ghCloneRepo" => Some("ENABLE_CLONE|OCTOCODE_STORAGE_MODE"),
        "jev" => Some("OCTOCODE_JEV_KEY"),
        "localFetch" | "localSearch" | "astSearch" | "astRewrite" | "lspSearch" => {
            Some("ENABLE_LOCAL")
        }
        _ => None,
    }
}

/// Text for a disabled tool whose gating env key was present in a `.env`
/// file but not applied — the state change would otherwise be invisible.
fn dropped_key_hint(
    env_vars: &str,
    dotenv: &octocode_native::config::EnvApplyReport,
) -> Option<String> {
    for key in env_vars.split('|') {
        for alias in [key.to_owned(), format!("OCTOCODE_{key}")] {
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
    }
    None
}

fn compact_tool_catalog(
    catalog: &Value,
    dotenv: &octocode_native::config::EnvApplyReport,
) -> Value {
    let tools = catalog
        .get("tools")
        .and_then(Value::as_array)
        .map(|tools| {
            tools
                .iter()
                .map(|tool| {
                    let name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
                    let description = tool
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let enabled = tool
                        .get("available")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let mut availability = json!({ "enabled": enabled });
                    if !enabled {
                        if let Some(env_var) = availability_env_var(name) {
                            availability["envVar"] = Value::String(env_var.to_owned());
                            if let Some(hint) = dropped_key_hint(env_var, dotenv) {
                                availability["hint"] = Value::String(hint);
                            }
                        } else {
                            availability["configuration"] =
                                Value::String("tools.enabled/tools.disabled".to_owned());
                        }
                    }
                    json!({
                        "name": name,
                        "category": tool_family(name),
                        "description": compact_description(description, 96),
                        "fields": compact_fields(tool),
                        "availability": availability
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut value = json!({
        "kind": "octocode.toolCatalog",
        "version": 1,
        "toolCount": tools.len(),
        "output": "Compact discovery catalog. Inspect one tool before execution.",
        "commands": {
            "schema": "scheme <name>",
            "querySchema": "scheme <name> --view query",
            "run": "<name> '<json>'"
        },
        "tools": tools
    });
    // Core-authored jev guidance surfaces here because `scheme` is the only
    // discovery command.
    if let Some(guidance) = catalog["cliGuidance"]["jev"]
        .as_str()
        .filter(|text| !text.is_empty())
    {
        let jev_enabled = catalog["tools"].as_array().is_some_and(|tools| {
            tools
                .iter()
                .any(|tool| tool["name"] == "jev" && tool["available"] == true)
        });
        if jev_enabled {
            value["guidance"] = json!({ "jev": guidance });
        }
    }
    value
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
        // Use 120 s instead of the default 60 s so LSP cold-start initialisation
        // (which can take ~60 s on the first invocation) completes without a timeout.
        timeout_secs: Some(120),
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
        Command::LocalSearch(args) => run_tool(runtime, "localSearch", args, json_errors).await,
        Command::LocalFetch(args) => run_tool(runtime, "localFetch", args, json_errors).await,
        Command::AstSearch(args) => run_tool(runtime, "astSearch", args, json_errors).await,
        Command::AstRewrite(args) => run_tool(runtime, "astRewrite", args, json_errors).await,
        Command::LspSearch(args) => run_tool(runtime, "lspSearch", args, json_errors).await,
        Command::GhSearch(args) => run_tool(runtime, "ghSearch", args, json_errors).await,
        Command::GhGetFileContent(args) => {
            run_tool(runtime, "ghGetFileContent", args, json_errors).await
        }
        Command::GhSearchHistory(args) => {
            run_tool(runtime, "ghSearchHistory", args, json_errors).await
        }
        Command::GhGetHistoryItem(args) => {
            run_tool(runtime, "ghGetHistoryItem", args, json_errors).await
        }
        Command::GhCloneRepo(args) => run_tool(runtime, "ghCloneRepo", args, json_errors).await,
        Command::ArtifactSearch(args) => {
            run_tool(runtime, "artifactSearch", args, json_errors).await
        }
        Command::Jev(args) => run_tool(runtime, "jev", args, json_errors).await,
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
            let Some(name) = tool else {
                return write_json(
                    &compact_tool_catalog(&catalog, &runtime.config().dotenv),
                    compact,
                );
            };
            let value = catalog["tools"]
                .as_array()
                .and_then(|tools| tools.iter().find(|tool| tool["name"] == *name))
                .cloned();
            let Some(value) = value else {
                let known = catalog["tools"]
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
                Ok(value) => write_json(&value, compact),
                Err(error) => {
                    emit_error(&error, json_errors);
                    2
                }
            }
        }
        Command::Config { check, json } => {
            let view = runtime.inspect_config();
            if let Some(key) = check {
                let set = runtime
                    .config()
                    .env_value(&key)
                    .is_some_and(|value| !value.is_empty());
                println!("{key}: {}", if set { "set" } else { "unset" });
                return if set { 0 } else { 1 };
            }
            let config_file = view
                .config_path
                .clone()
                .unwrap_or_else(|| view.home.join(".octocoderc"));
            let config_file_exists = view.config_path.is_some();
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
                        "envFiles": {
                            "global": view.global_env_path,
                            "project": view.project_env_path,
                        },
                        "envKeys": view.loaded_keys,
                        "skippedProtected": view.skipped_protected,
                        "skippedExisting": view.skipped_existing,
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
            println!("env files:");
            println!("  global:  {}", view.global_env_path.display());
            println!("  project: {}", view.project_env_path.display());
            if !view.config_keys.is_empty() {
                println!("config keys ({}):", view.config_keys.len());
                for key in &view.config_keys {
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
            for diagnostic in &view.diagnostics {
                eprintln!("{}: {}", diagnostic.code, diagnostic.message);
            }
            0
        }
        Command::Auth { command, json } => match command {
            None | Some(AuthCommand::Status { json: false }) => {
                system::auth_status(runtime, json).await
            }
            Some(AuthCommand::Status { json: true }) => system::auth_status(runtime, true).await,
            Some(AuthCommand::Login {
                hostname,
                force,
                refresh,
                json,
            }) => system::login(runtime, hostname.as_deref(), force, refresh, json).await,
            Some(AuthCommand::Logout) => system::logout(runtime),
        },
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
            emit_error(&error, json_errors);
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
            emit_error(&format!("Invalid JSON query: {parse_error}"), json_errors);
            return 2;
        }
    };
    execute(runtime, tool, input, args.compact).await
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
            let mut exit = match outcome.failure {
                Some(octocode_native::runtime::FailureKind::NotFound) => 3,
                // The raw-tool CLI classifies the legacy 401 message as a tool
                // failure for parity with the frozen Node CLI contract.
                Some(octocode_native::runtime::FailureKind::Authentication) => 5,
                Some(octocode_native::runtime::FailureKind::Permission) => 4,
                Some(octocode_native::runtime::FailureKind::RateLimited) => 7,
                Some(octocode_native::runtime::FailureKind::Execution) => 5,
                None => 0,
            };
            if !outcome.all_failed {
                // A bulk result where some rows succeeded is not a total
                // failure, but a partial source read or an available
                // continuation is still exit 6. A nested/informational partial
                // with no continuation and no source content (e.g. a reasoning
                // tool's coverage `truncated`) stays 0.
                let has_continuation =
                    value["results"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|row| {
                            row.pointer("/data/next").is_some()
                                || (octocode_native::runtime::response::is_partial(&row["data"])
                                    && row["data"]["content"]
                                        .as_str()
                                        .is_some_and(|text| !text.is_empty()))
                        });
                exit = if has_continuation { 6 } else { 0 };
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
