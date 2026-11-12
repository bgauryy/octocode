mod catalog;
mod commands;
mod config;
mod config_view;
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
mod tool_call;
use catalog::machine_catalog;
use clap::Parser;
use commands::{AuthCommand, Command, ConfigCommand};
use management::config_management;
use octocode_contract_types::ErrorCode;
use octocode_native::config::RuntimeSurface;
use octocode_native::runtime::{HostOptions, RuntimeError, ToolRuntime};
use output::exit_code;
pub(super) use output::{write_json, write_text};
use parse::emit_error;
pub use parse::parse_args;
use std::io::{self};
use tool_call::run_tool;

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

Route by where the evidence is: this checkout (local tools, lspSearch), a package
(artifactSearch), another repo (gh tools); read the deciding span before a claim.
Which tool, how little to ask for: schema --view flow.

Tools: one JSON envelope ({"queries":[…]} or {"cursor":"…"} alone); a query takes
mainGoal (the task) and reasoning (why this query). --input FILE|- reads the same
JSON from a file or stdin; clasify --content - reads one row's text from stdin.
Examples: <tool> --help.
Discovery, smallest first: schema <tool> --view variants (operations, required
fields, an example); schema <tool> (every field); schema --view full (all tools).
All tools run here, including the MCP opt-ins graphBuild, graphQuery, ghCloneRepo:
  graphBuild '{"queries":[{"path":"."}]}', then graphQuery with operation stats;
  ghCloneRepo '{"queries":[{"owner":"o","repo":"r"}]}', then local tools at location.path.

Output: text, on a terminal or a pipe; --json prints JSON (schema always does).
Text on stdout: search hits (line:text, context line-text), sites and outlines
under their file's path once, a lone one as path:line:…; path:N is always a line,
and [ … ] after a line or path lists its other fields. A listing puts entries
under their dir/. Any other result is indented key: value lines; a batch heads
each row with ## results[N].
After each row's content, as # lines on stdout: its status unless ok (empty,
error, outOfRange), each page state (hasMore), its warnings and hints, then the
commands to run next. So stdout alone tells no hits, a failure and a first page
apart, and says how to read the answer.
Every stderr line is a # note: counters, related leads and notices, and progress
when stderr is a terminal.
Errors: a failed row as above. A call rejected before it runs (invalid input,
timeout) prints the same way: errorCode and error, then # status: error; with
--json, the JSON envelope. Both are on stdout.
Exits: 0 success, 1 empty, 2 invalid input (including batch rows), 3 not found,
4 auth required, 5 execution error, 7 rate limited, 130 interrupted.
A batch exits 2 if any row rejected its input, else its failed rows' code (5 if
they differ), else 1 only when every row is empty or out of range, else 0;
stdout still carries every row's result.
More pages still exit 0: run responsePagination.next, else next.nextPage, until neither remains."#;

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

/// Whether a person is at this terminal and can see a window opened from it.
fn interactive_session() -> bool {
    use std::io::IsTerminal;
    stdout_is_terminal()
        && io::stdin().is_terminal()
        && config_view::can_show_window(|name| std::env::var_os(name))
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
        Command::Config {
            command: None,
            manage: false,
            ..
        } if !json_out && interactive_session() => {
            return config_view::run(false, commands::VIEW_IDLE_SECONDS, false).await;
        }
        Command::Serve { socket } => return serve::run(&socket).await,
        Command::Mcp => mcp_server::run().await,
        Command::Tool(tool) => return run_tool(tool, mode).await,
        command => command,
    };
    let runtime = match ToolRuntime::from_host(cli_host()) {
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
        Command::Schema {
            tool: None, view, ..
        } if view.as_deref() == Some("flow") => {
            // A closed pipe (`| head`) is not a failure of the guide.
            let _ = std::io::Write::write_all(
                &mut std::io::stdout(),
                mcp_server::resources::TOOL_FLOW.as_bytes(),
            );
            0
        }
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
                Some(ConfigCommand::List) => config::list(runtime, json_out),
                Some(ConfigCommand::Get { key }) => config::get(runtime, &key, json_out),
                Some(ConfigCommand::Set { key, value, stdin }) => {
                    config::set(runtime, config::env_key(&key), value, stdin, json_out)
                }
                Some(ConfigCommand::Unset { key }) => {
                    config::unset(runtime, config::env_key(&key), json_out)
                }
                Some(ConfigCommand::Check { key }) => config::check(runtime, &key, json_out).await,
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
        Command::Cache {
            command: commands::CacheCommand::Clear,
        } => {
            if let Err(message) = serve::stop_all_and_wait().await {
                emit_error(ErrorCode::ExecutionFailed, &message, mode);
                return 5;
            }
            match runtime.clear_caches().await {
                Ok(result) => {
                    if json_out {
                        match serde_json::to_value(result) {
                            Ok(value) => write_json(&value),
                            Err(error) => {
                                emit_error(ErrorCode::ExecutionFailed, &error.to_string(), mode);
                                5
                            }
                        }
                    } else {
                        write_text(&format!(
                            "Removed {} cache directories. Clone checkouts and graph snapshots were kept. Restart separate MCP processes to discard their in-memory caches.",
                            result.removed.len()
                        ))
                    }
                }
                Err(error) => start_failed(&error, mode),
            }
        }
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

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tool_flags_tests;
