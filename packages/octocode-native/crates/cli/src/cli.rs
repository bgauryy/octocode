mod catalog;
mod commands;
mod config;
mod config_view;
mod graph;
mod lines;
mod lsp_provision;
mod management;
mod mcp_clients;
mod mcp_install;
mod mcp_manage;
mod mcp_server;
mod output;
mod parse;
mod schema;
mod serve;
mod skill;
mod system;
use catalog::machine_catalog;
use clap::Parser;
use commands::{AuthCommand, Command, ConfigCommand, ToolArgs};
use management::config_management;
use octocode_contract_types::ErrorCode;
use octocode_native::config::RuntimeSurface;
use octocode_native::id::ToolId;
use octocode_native::runtime::{Channel, HostOptions, RuntimeError, ToolRuntime};
use output::{Printed, exit_code};
pub(super) use output::{write_json, write_text};
use parse::emit_error;
pub use parse::parse_args;
use serde_json::Value;
use std::io::{self};

#[derive(Parser)]
#[command(
    name = "octocode",
    version,
    about = "Native Octocode research tools",
    long_about = ROOT_HELP,
    after_help = "Run: octocode <tool> '<json>' or --input FILE|-\nUse --help for output rules and exit codes; <tool> --help for JSON examples."
)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
    /// Print JSON instead of text: indented on a terminal, one line on a pipe.
    #[arg(long, global = true)]
    json: bool,
}

const ROOT_HELP: &str = r#"Native Octocode research tools.

Tools: one JSON envelope ({"queries":[…]} or {"cursor":"…"});
--input FILE|- reads the same JSON from a file or stdin.
Discovery: schema lists tools; schema <tool> lists query fields.
Reference: schema --view full; schema <tool> --view variants. Examples: <tool> --help.
Graph: graph ingest <path>, then graph query <op> [ref]; see graph --help.

Output: text, on a terminal or a pipe; --json prints JSON (schema always does).
Text: result lines on stdout ([ … ] after a line lists its other fields); # notes
and follow-up commands on stderr.
Errors: the JSON envelope on stdout with --json, text on stderr otherwise.
Exits: 0 success, 1 empty, 2 invalid input (including batch rows), 3 not found,
4 auth required, 5 execution error, 7 rate limited, 130 interrupted.
A batch exits with its worst row; stdout still carries every row's result.
More pages still exit 0: follow next.*; check pagination.hasMore."#;

/// How a command answers: JSON or text, and whether a pipe chose JSON.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct OutputMode {
    /// The command prints JSON.
    pub(super) json: bool,
    /// JSON because stdout is a pipe; the caller did not pass `--json`.
    piped: bool,
}

impl OutputMode {
    /// `--json`, or a pipe for a command that answers a pipe in JSON.
    fn new(json_flag: bool, answers_pipe_in_json: bool) -> Self {
        let piped = !json_flag && answers_pipe_in_json && !stdout_is_terminal();
        Self {
            json: json_flag || piped,
            piped,
        }
    }

    /// An error in this mode is also written as text on stderr: a pipe chose
    /// JSON, and stderr is a terminal, so a person may be watching it while
    /// the JSON goes on to the next program.
    fn tells_stderr(self) -> bool {
        use std::io::IsTerminal;
        self.piped && io::stderr().is_terminal()
    }
}

impl From<bool> for OutputMode {
    /// A command's explicit choice: JSON for `--json`, else text.
    fn from(json: bool) -> Self {
        Self { json, piped: false }
    }
}

fn stdout_is_terminal() -> bool {
    use std::io::IsTerminal;
    io::stdout().is_terminal()
}

pub async fn run(args: Args) -> u8 {
    let Args { command, json } = args;
    let mode = command.output_mode(json);
    let json_out = mode.json;
    let command = match command {
        Command::Config {
            command:
                Some(ConfigCommand::View {
                    no_open,
                    idle_timeout,
                }),
            ..
        } => {
            return config_view::run(no_open, idle_timeout, json_out).await;
        }
        Command::Serve { socket } => return serve::run(&socket).await,
        Command::Mcp => mcp_server::run().await,
        Command::Tool(tool) => return run_tool(tool, mode).await,
        command => command,
    };
    let runtime = match ToolRuntime::from_host(cli_host()) {
        Ok(runtime) if matches!(command, Command::Schema { .. }) => {
            runtime.check_clasify_provider().await
        }
        Ok(runtime) => runtime,
        Err(error) => return start_failed(&error, mode),
    };
    let result = dispatch(command, &runtime, mode).await;
    runtime.close().await;
    result
}

/// The CLI's runtime host: this process's environment and directory.
fn cli_host() -> HostOptions {
    HostOptions {
        surface: RuntimeSurface::Cli,
        ..HostOptions::default()
    }
}

/// The exit code of a login or logout; one that succeeded stops the warm
/// servers, which may still remember the credential it replaced.
fn credential_changed(code: u8) -> u8 {
    if code == 0 {
        serve::stop_all();
    }
    code
}

/// A runtime that could not start: its error in the command's output mode.
fn start_failed(error: &RuntimeError, mode: OutputMode) -> u8 {
    emit_error(error.code, &error.message, mode);
    exit_code(error.exit_class())
}

impl Command {
    /// The output mode the command was asked for: `schema` answers a pipe in
    /// JSON; every command answers `--json` in JSON and prints text otherwise.
    fn output_mode(&self, json: bool) -> OutputMode {
        OutputMode::new(json, matches!(self, Self::Schema { .. }))
    }
}

async fn dispatch(command: Command, runtime: &ToolRuntime, mode: OutputMode) -> u8 {
    let json_out = mode.json;
    match command {
        Command::Schema { tool, view, select } => {
            let value = machine_catalog(runtime)
                .map_err(|message| schema::SchemaError { message, exit: 5 })
                .and_then(|machine| match tool.as_deref() {
                    None => schema::catalog(&machine, view.as_deref()),
                    Some(name) => {
                        schema::tool_view(&machine, name, view.as_deref(), select.as_deref())
                    }
                });
            match value {
                Ok(value) => write_json(&value),
                Err(error) => {
                    let code = if error.exit == 2 {
                        ErrorCode::InvalidInput
                    } else {
                        ErrorCode::Contract
                    };
                    emit_error(code, &error.message, mode);
                    error.exit
                }
            }
        }
        Command::Config {
            command, manage, ..
        } => {
            if manage {
                return config_management(runtime);
            }
            match command {
                Some(ConfigCommand::Home) => config::home(runtime, json_out),
                Some(ConfigCommand::Get { key }) => {
                    config::get(runtime, config::env_key(&key), json_out)
                }
                Some(ConfigCommand::Set { key, value, stdin }) => {
                    config::set(runtime, config::env_key(&key), value, stdin, json_out)
                }
                Some(ConfigCommand::Unset { key }) => {
                    config::unset(runtime, config::env_key(&key), json_out)
                }
                Some(ConfigCommand::Check { key }) => {
                    config::check(runtime, config::env_key(&key), json_out).await
                }
                // `config view` returns from `run` before the runtime starts.
                Some(ConfigCommand::View { .. }) | None => config::show(runtime, json_out),
            }
        }
        Command::Auth { command } => match command {
            AuthCommand::Status => system::auth_status(runtime, json_out).await,
            AuthCommand::Login {
                hostname,
                force,
                refresh,
            } => credential_changed(
                system::login(runtime, hostname.as_deref(), force, refresh, json_out).await,
            ),
            AuthCommand::Logout => credential_changed(system::logout(runtime, json_out)),
        },
        Command::Graph { command } => graph::graph(runtime, command, mode),
        Command::Skill { command } => skill::run(runtime, command, json_out).await,
        Command::Install(args) => mcp_install::run(mcp_install::InstallArgs {
            json: json_out,
            ..args
        }),
        Command::LspServer { command } => lsp_provision::run(runtime, command, json_out).await,
        // Handled in `run` before the runtime starts.
        Command::Tool(_) | Command::Serve { .. } | Command::Mcp => 2,
    }
}

/// Parse the query, then run it on the workspace's warm server (read-only
/// tools) or in this process.
async fn run_tool(tool: commands::ToolCommand, mode: OutputMode) -> u8 {
    let input = match tool_input(tool.name, &tool.args) {
        Ok(input) => input,
        Err(message) => {
            emit_error(ErrorCode::InvalidInput, &message, mode);
            return 2;
        }
    };
    if ToolId::from_name(tool.name).is_some_and(serve::runs_on_server)
        && let Some(code) = serve::call(tool.name, &input, mode).await
    {
        return code;
    }
    let runtime = match ToolRuntime::from_host(cli_host()) {
        Ok(runtime) => runtime.check_clasify_provider().await,
        Err(error) => return start_failed(&error, mode),
    };
    let code = execute(&runtime, tool.name, input, mode).await;
    runtime.close().await;
    code
}

/// The JSON input of a tool command, supplied inline or through --input.
fn tool_input(tool: &str, args: &ToolArgs) -> Result<Value, String> {
    if args.input.is_some() && !args.args.is_empty() {
        return Err(format!(
            "--input reads the query; drop the other arguments. Usage: octocode {tool} --help"
        ));
    }
    parse_json_query(tool, args.query_text()?)
}

/// The JSON form's query text, parsed.
fn parse_json_query(tool: &str, json: Option<String>) -> Result<Value, String> {
    let query_text = json.ok_or_else(|| {
        format!("Missing query. Usage: octocode {tool} '<json>' or --input FILE|-")
    })?;
    serde_json::from_str::<Value>(&query_text)
        .map_err(|parse_error| format!("Invalid JSON query: {parse_error}"))
}

/// What an interrupted call says while its cancelled work drains.
const CANCELLING: &str = "Cancelling; press Ctrl-C again to exit now.";

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

/// Execute one tool call and print its result. Exits with the code of the
/// runtime's [`ExitClass`].
async fn execute(runtime: &ToolRuntime, tool: &str, input: Value, mode: OutputMode) -> u8 {
    const REQUEST: &str = "cli-1";
    let admission = match runtime.admit(REQUEST.into()) {
        Ok(admission) => admission,
        Err(error) => {
            return Printed::finish(tool, Err(error), mode.json, &input, None).print(mode);
        }
    };
    let query = input.clone();
    let execution = runtime.execute(admission, tool.into(), input, Channel::Cli);
    tokio::pin!(execution);
    let result = tokio::select! {
        result = &mut execution => result,
        signal = tokio::signal::ctrl_c() => {
            if signal.is_err() {
                execution.await
            } else {
                runtime.requests.cancel(REQUEST);
                eprintln!("{CANCELLING}");
                if drain_after_interrupt(&mut execution, tokio::signal::ctrl_c()).await
                    == Interrupted::Forced
                {
                    // Work that ignores cancellation must not hold the
                    // terminal hostage: the user asked twice. Returning would
                    // drop the runtime in `main`, which joins its blocking
                    // threads, so the process exits here instead.
                    std::process::exit(130);
                }
                return 130;
            }
        }
    };
    let cwd = std::env::current_dir().ok();
    Printed::finish(tool, result, mode.json, &query, cwd.as_deref()).print(mode)
}

#[cfg(test)]
mod tests;
