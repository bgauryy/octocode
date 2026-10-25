use super::{Args, OutputMode, stdout_is_terminal, write_json};
use clap::{CommandFactory, FromArgMatches};
use octocode_contract_types::ErrorCode;
use octocode_native::config::RuntimeSurface;
use octocode_native::execution::availability::ToolAvailability;
use octocode_native::id::ToolId;
use serde_json::{Value, json};
use std::io::{self, Write};

/// A command's own failure (not a tool call's): the `{error, errorCode}`
/// envelope tool errors use, on stdout in JSON mode, else the message on
/// stderr. Every code is a declared `errorCodes` entry.
pub(super) fn emit_error(code: ErrorCode, message: &str, mode: impl Into<OutputMode>) {
    let mode = mode.into();
    let envelope = error_envelope(code, message, &[]);
    if mode.json {
        write_json(&envelope);
    }
    if !mode.json || mode.tells_stderr() {
        eprintln!("{}", envelope["error"].as_str().unwrap_or_default());
    }
}

/// The `{error, errorCode[, details]}` envelope of a command's own failure,
/// with secrets scrubbed from every line.
pub(super) fn error_envelope(code: ErrorCode, message: &str, details: &[&str]) -> Value {
    use octocode_native::security::scrub_error_text;
    let details = details.iter().map(|line| scrub_error_text(line)).collect();
    let mut envelope = octocode_native::contracts::tool_error(&scrub_error_text(message), details);
    envelope["errorCode"] = json!(code);
    envelope
}

/// `schema` prints JSON Schema, so it answers a pipe in JSON; every other
/// command prints text unless `--json` asks for JSON.
pub(super) fn answers_pipe_in_json(name: &str) -> bool {
    name == "schema"
}

/// How a parse error is reported: as JSON for `--json` anywhere, or for
/// `schema` on a pipe; as text on stderr otherwise.
pub(super) fn parse_error_mode(argv: &[std::ffi::OsString]) -> OutputMode {
    let json_flag = argv.iter().any(|arg| arg == "--json");
    let command = argv
        .iter()
        .skip(1)
        .filter_map(|arg| arg.to_str())
        .find(|arg| !arg.starts_with('-'));
    OutputMode::new(json_flag, command.is_some_and(answers_pipe_in_json))
}

/// Clap's diagnostic without its "to pass '-L' as a value, use '-- -L'" tip:
/// an unknown flag is a flag the tool lacks (its help lists the ones it has),
/// never a positional value that starts with `-`.
pub(super) fn without_dash_dash_tip(rendered: &str) -> String {
    rendered
        .split_inclusive('\n')
        .filter(|line| !(line.contains("to pass '") && line.contains("as a value, use '-- ")))
        .collect()
}

/// Clap's diagnostic as text on stderr, colored when stderr is a terminal,
/// with secrets scrubbed.
pub(super) fn print_clap_error(error: &clap::Error) {
    let original = error.render().to_string();
    let sanitized = octocode_native::security::scrub_error_text(&without_dash_dash_tip(&original));
    if sanitized == original {
        let _ = error.print();
    } else {
        let _ = io::stderr().lock().write_all(sanitized.as_bytes());
    }
}

/// Clap's whole diagnostic as the JSON envelope: `error` is its error line
/// and `tip:` lines (the did-you-mean names), and every other line (possible
/// values, usage) is a `details` entry.
pub(super) fn clap_error_envelope(error: &clap::Error) -> Value {
    let rendered = without_dash_dash_tip(&error.render().to_string());
    let (message, details): (Vec<&str>, Vec<&str>) = rendered
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .partition(|line| line.starts_with("error: ") || line.starts_with("tip: "));
    let message = message
        .iter()
        .map(|line| line.strip_prefix("error: ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("; ");
    // A rejected command line is the caller's input, like a bad query.
    error_envelope(ErrorCode::InvalidInput, &message, &details)
}

pub fn parse_args() -> Result<Args, u8> {
    parse_args_from(std::env::args_os())
}

pub(super) fn parse_args_from<I, T>(argv: I) -> Result<Args, u8>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut argv: Vec<std::ffi::OsString> = argv.into_iter().map(Into::into).collect();
    // A bare `octocode` on a pipe is an agent asking what it can do. A
    // terminal is the ordinary clap help path (stdout and a successful exit).
    if argv.len() == 1 && !stdout_is_terminal() {
        argv.push("schema".into());
    } else if argv.len() == 1 {
        argv.push("--help".into());
    }
    match Args::command()
        .try_get_matches_from(&argv)
        .and_then(|matches| Args::from_arg_matches(&matches))
    {
        Ok(args) => Ok(args),
        Err(error) => {
            let error = if is_help(&error) {
                help_command()
                    .try_get_matches_from(&argv)
                    .err()
                    .unwrap_or(error)
            } else {
                error
            };
            let displays_text =
                is_help(&error) || error.kind() == clap::error::ErrorKind::DisplayVersion;
            if displays_text {
                let _ = error.print();
                return Err(error.exit_code() as u8);
            }
            let mode = parse_error_mode(&argv);
            if !mode.json {
                print_clap_error(&error);
                return Err(error.exit_code() as u8);
            }
            write_json(&clap_error_envelope(&error));
            if mode.tells_stderr() {
                print_clap_error(&error);
            }
            Err(2)
        }
    }
}

pub(super) fn is_help(error: &clap::Error) -> bool {
    matches!(
        error.kind(),
        clap::error::ErrorKind::DisplayHelp
            | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    )
}

/// Help lists tools allowed by configuration; schema and execution also
/// check provider readiness. A hidden tool keeps its own help. Help starts
/// no runtime, cache maintenance, provider request, or config warnings.
pub(super) fn hide_unavailable_tools(mut command: clap::Command) -> clap::Command {
    use octocode_native::config::{acquire_config_input, resolve_config};
    let (Ok(cwd), Some(home)) = (std::env::current_dir(), std::env::home_dir()) else {
        return command;
    };
    let surface = RuntimeSurface::Cli;
    let input = acquire_config_input(octocode_io::env::vars_lossy(), cwd, home, surface);
    let config = resolve_config(&input);
    let availability = ToolAvailability::resolve(&config, surface);
    for tool in ToolId::ALL {
        if !availability.is_available(tool.as_str()) {
            command = command.mut_subcommand(tool.as_str(), |sub| sub.hide(true));
        }
    }
    command
}

/// The help command: unavailable tools hidden, each tool's help ending with
/// what the query needs (required fields and one runnable example), and the
/// root help with every available tool's flag examples, all from the
/// embedded CLI presentation.
pub(super) fn help_command() -> clap::Command {
    ToolId::ALL
        .into_iter()
        .fold(hide_unavailable_tools(Args::command()), |command, tool| {
            match octocode_native::contracts::cli_tool_help(tool.as_str()) {
                Some(help) => command.mut_subcommand(tool.as_str(), |sub| sub.after_help(help)),
                None => command,
            }
        })
}
