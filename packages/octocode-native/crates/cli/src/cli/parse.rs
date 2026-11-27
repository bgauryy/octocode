use super::{Args, OutputMode, ToolPackage, stdout_is_terminal, write_json};
use clap::{CommandFactory, FromArgMatches};
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::config::RuntimeSurface;
use octocode_runtime_base::execution::availability::ToolAvailability;
use octocode_runtime_base::id::ToolId;
use serde_json::{Value, json};
use std::io::{self, Write};

/// A command's own failure (not a tool call's): the `{error, errorCode,
/// errorClass}` envelope tool errors use, on stdout in JSON mode, else the message on
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
    use octocode_runtime_base::security::scrub_error_text;
    let details = details.iter().map(|line| scrub_error_text(line)).collect();
    let mut envelope =
        octocode_runtime_base::contracts::tool_error(&scrub_error_text(message), details);
    envelope["errorCode"] = json!(code);
    envelope["errorClass"] = json!(octocode_runtime_base::id::error_codes::class(code));
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
    let sanitized =
        octocode_runtime_base::security::scrub_error_text(&without_dash_dash_tip(&original));
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

/// Where the command word runs when the parser refused it as a subcommand
/// but another package serves a tool of that name.
fn other_package_tool_note(
    package: ToolPackage,
    error: &clap::Error,
    argv: &[std::ffi::OsString],
) -> Option<String> {
    if error.kind() != clap::error::ErrorKind::InvalidSubcommand {
        return None;
    }
    let name = argv
        .iter()
        .skip(1)
        .filter_map(|arg| arg.to_str())
        .find(|arg| !arg.starts_with('-'))?;
    package.elsewhere_note(name)
}

pub(crate) fn parse_args() -> Result<Args, u8> {
    parse_args_from(std::env::args_os())
}

pub(super) fn parse_args_from<I, T>(argv: I) -> Result<Args, u8>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut argv: Vec<std::ffi::OsString> = argv.into_iter().map(Into::into).collect();
    // The graph distribution delegates to this same binary through a private selector.
    let package = if argv.get(1).is_some_and(|arg| arg == "__graph") {
        argv.remove(1);
        argv[0] = "octocode-graph".into();
        ToolPackage::Graph
    } else {
        ToolPackage::Research
    };
    // A bare `octocode` on a pipe is an agent asking what it can do. A
    // terminal is the ordinary clap help path (stdout and a successful exit).
    if argv.len() == 1 && !stdout_is_terminal() {
        argv.push("schema".into());
    } else if argv.len() == 1 {
        argv.push("--help".into());
    }
    match package_command(package)
        .try_get_matches_from(&argv)
        .and_then(|matches| Args::from_arg_matches(&matches))
    {
        Ok(mut args) => {
            args.package = package;
            Ok(args)
        }
        Err(error) => {
            let error = if is_help(&error) {
                help_command(package)
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
                return Err(clap_exit_code(&error));
            }
            let mode = parse_error_mode(&argv);
            let elsewhere = other_package_tool_note(package, &error, &argv);
            if !mode.json {
                print_clap_error(&error);
                if let Some(note) = elsewhere {
                    eprintln!("{note}");
                }
                return Err(clap_exit_code(&error));
            }
            let mut envelope = clap_error_envelope(&error);
            if let (Some(note), Some(message)) = (elsewhere, envelope["error"].as_str()) {
                envelope["error"] = json!(format!("{message}; {note}"));
            }
            write_json(&envelope);
            if mode.tells_stderr() {
                print_clap_error(&error);
            }
            Err(2)
        }
    }
}

/// Project the one clap definition onto the installed package, including direct help.
fn package_command(package: ToolPackage) -> clap::Command {
    let all = Args::command();
    let mut command = clap::Command::new(package.cli())
        .version(super::build_version())
        .subcommand_required(true)
        .args(all.get_arguments().cloned())
        .subcommands(
            all.get_subcommands()
                .filter(|sub| {
                    let name = sub.get_name();
                    match ToolId::from_name(name) {
                        Some(id) => ToolPackage::of(id) == package,
                        None => {
                            package == ToolPackage::Research || matches!(name, "mcp" | "schema")
                        }
                    }
                })
                .cloned(),
        );
    if package == ToolPackage::Graph {
        command = command.about("Create and query Octocode graph snapshots");
    } else {
        command = command
            .about("Native Octocode research tools")
            .after_help(super::ROOT_HELP_AFTER);
    }
    command
}

/// The exit code clap gives `error`: 0 for help and version, 2 for a usage
/// error. Clap gives no other; one would read as a usage error.
fn clap_exit_code(error: &clap::Error) -> u8 {
    u8::try_from(error.exit_code()).unwrap_or(2)
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
    use octocode_runtime_base::config::{acquire_config_input, resolve_config};
    let (Ok(cwd), Some(home)) = (std::env::current_dir(), std::env::home_dir()) else {
        return command;
    };
    let surface = RuntimeSurface::Cli;
    let input = acquire_config_input(octocode_io::env::vars_lossy(), cwd, home, surface);
    let config = resolve_config(&input);
    let availability = ToolAvailability::resolve(&config, surface);
    for tool in ToolId::ALL {
        if command.find_subcommand(tool.as_str()).is_some()
            && !availability.is_available(tool.as_str())
        {
            command = command.mut_subcommand(tool.as_str(), |sub| sub.hide(true));
        }
    }
    command
}

/// The help command: unavailable tools hidden, each tool's help ending with
/// what the query needs (required fields and one runnable example) and where
/// its output fields are explained, and the root help with each available
/// tool's required fields by operation, all from the embedded CLI
/// presentation.
pub(super) fn help_command(package: ToolPackage) -> clap::Command {
    let command = ToolId::ALL
        .into_iter()
        .filter(|tool| ToolPackage::of(*tool) == package)
        .fold(
            hide_unavailable_tools(package_command(package)),
            |command, tool| {
                let name = tool.as_str();
                match octocode_runtime_base::contracts::cli_tool_help(name) {
                    Some(help) => command.mut_subcommand(name, |sub| {
                        sub.after_help(format!(
                            "{help}\nOutput fields: {}",
                            super::schema::output_command(name)
                        ))
                    }),
                    None => command,
                }
            },
        );
    if package == ToolPackage::Graph {
        return command;
    }
    let listed: Vec<(&str, Vec<String>)> = ToolId::ALL
        .into_iter()
        .map(ToolId::as_str)
        .filter(|name| {
            command
                .find_subcommand(name)
                .is_some_and(|sub| !sub.is_hide_set())
        })
        .filter_map(|name| Some((name, super::schema::required_by_operation(name)?)))
        .collect();
    let (head, tail, after) = (
        super::ROOT_HELP_HEAD,
        super::ROOT_HELP_TAIL,
        super::ROOT_HELP_AFTER,
    );
    if listed.is_empty() {
        return command.long_about(format!("{head}\n\n{tail}"));
    }
    // --help reads the fields before the output rules; -h, after the commands.
    let fields = required_fields_block(&listed);
    command
        .long_about(format!("{head}\n\n{fields}\n\n{tail}"))
        .after_help(format!("{fields}\n\n{after}"))
        .after_long_help(after)
}

/// The root help's required fields: one entry per listed tool, its parts
/// (`required_by_operation`) wrapped at `;` under the fields column, then
/// where every form and the output fields are explained.
fn required_fields_block(listed: &[(&str, Vec<String>)]) -> String {
    const WIDTH: usize = 88;
    let width = listed.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
    let indent = " ".repeat(width + 4);
    let mut block = String::from(
        "Required query fields (\"operation a|b: f\": operation \"a\" or \"b\", plus f):",
    );
    for (name, parts) in listed {
        let mut line = format!("  {name:width$}  ");
        for (index, part) in parts.iter().enumerate() {
            let piece = if index + 1 < parts.len() {
                format!("{part};")
            } else {
                part.clone()
            };
            let start = line.len() == width + 4;
            if !start && line.len() + 1 + piece.len() > WIDTH {
                block.push('\n');
                block.push_str(line.trim_end());
                line.clone_from(&indent);
            } else if !start {
                line.push(' ');
            }
            line.push_str(&piece);
        }
        block.push('\n');
        block.push_str(&line);
    }
    block.push_str(
        "\nBefore a tool's first call: octocode <tool> --help (every form, examples).\n\
         What output fields mean: schema <tool> --view output",
    );
    block
}
