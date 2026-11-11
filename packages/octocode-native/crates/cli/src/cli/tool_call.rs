//! One tool command: its JSON input, where it runs, and what an interrupt does.
use super::commands::{self, ToolArgs};
use super::output::Printed;
use super::{OutputMode, cli_host, serve, start_failed};
use octocode_contract_types::ErrorCode;
use octocode_native::id::ToolId;
use octocode_native::runtime::{Channel, HostOptions, RuntimeError, ToolRuntime};
use serde_json::Value;

/// Parse the query, then run it on the workspace's warm server (read-only
/// tools) or in this process.
pub(super) async fn run_tool(tool: commands::ToolCommand, mode: OutputMode) -> u8 {
    let input = match tool_input(tool.name, &tool.args) {
        Ok(input) => input,
        // No query to run is a rejected call, printed as the runtime's are.
        Err(message) => {
            let rejected = RuntimeError {
                code: ErrorCode::InvalidInput,
                message,
                payload: None,
                validation_issues: None,
            };
            return Printed::finish(tool.name, Err(rejected), mode.json, &Value::Null, None)
                .print(mode);
        }
    };
    let mut loaded = None;
    if ToolId::from_name(tool.name).is_some_and(serve::runs_on_server)
        && let Some(code) = serve::call(tool.name, &input, mode, &mut loaded).await
    {
        return code;
    }
    // Progress (a build's stages, a rate-limit wait) goes to a terminal's
    // stderr as notes; stdout keeps only the result.
    if let Some(print) = progress_printer(stderr_is_terminal()) {
        octocode_io::notice::report_progress_to(print);
    }
    let host = HostOptions {
        loaded,
        ..cli_host()
    };
    let runtime = match ToolRuntime::from_host(host) {
        Ok(runtime) => runtime,
        Err(error) => return start_failed(&error, mode),
    };
    let code = execute(&runtime, tool.name, input, mode).await;
    runtime.close().await;
    code
}

/// Where a CLI call's progress lines (a build's stages, a rate-limit wait, a
/// language server starting) go: to stderr as notes when it is a `terminal`,
/// for a person watching the call; nowhere otherwise, since a program reading
/// the streams gets them only when the call ends, beside the result that
/// already says what it needs. MCP relays progress by its own token.
pub(in crate::cli) fn progress_printer(
    terminal: bool,
) -> Option<impl Fn(&str) + Send + Sync + 'static> {
    terminal.then_some(|line: &str| eprintln!("{}", octocode_io::notice::as_note(line)))
}

pub(in crate::cli) fn stderr_is_terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stderr().is_terminal()
}

/// The JSON input of a tool command, supplied inline or through --input.
pub(super) fn tool_input(tool: &str, args: &ToolArgs) -> Result<Value, String> {
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
pub(in crate::cli) const CANCELLING: &str = "Cancelling; press Ctrl-C again to exit now.";

#[derive(Debug, PartialEq, Eq)]
pub(in crate::cli) enum Interrupted {
    /// The cancelled work finished.
    Drained,
    /// A second interrupt arrived first.
    Forced,
}

/// Wait for cancelled `execution` to finish, unless `second` (the next
/// interrupt) arrives first.
pub(in crate::cli) async fn drain_after_interrupt<T>(
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
        Ok(admission) => admission.requiring_persistent_continuations(),
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
                eprintln!("{}", octocode_io::notice::as_note(CANCELLING));
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
