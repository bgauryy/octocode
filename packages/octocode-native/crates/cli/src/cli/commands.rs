//! The `Command` enum: one sub-command per native tool plus system commands.
//!
//! Every tool is invoked by its canonical name with a JSON query, or with the
//! flags and positionals `flags.rs` builds from the contract (one long flag per
//! field, short letters from rg/grep/fd/ast-grep/gh). `schema` is the single
//! discovery command.
use clap::{ArgMatches, Args, FromArgMatches, Subcommand};
use octocode_native::id::ToolId;

/// Shared arguments for every tool sub-command: positionals (one JSON query,
/// or the tool's positional fields) and a JSON query file.
#[derive(Args, Debug)]
pub(super) struct ToolArgs {
    /// A JSON query ('{…}' or '{"queries":[…]}'), or the tool's positional fields (see Usage).
    #[arg(value_name = "ARGS", num_args = 0.., action = clap::ArgAction::Append)]
    pub args: Vec<String>,
    /// Read the JSON query from a file, or from stdin with `-`.
    #[arg(long, value_name = "FILE|-")]
    pub input: Option<std::path::PathBuf>,
}

/// Largest JSON query read from `--input` (a file or stdin).
const MAX_QUERY_BYTES: u64 = 8 * 1024 * 1024;

impl ToolArgs {
    /// Resolve the JSON query text from `--input FILE|-` or the positional
    /// argument. `Ok(None)` means no query was supplied.
    pub fn query_text(&self) -> Result<Option<String>, String> {
        let Some(path) = &self.input else {
            return Ok(self.json_arg().map(str::to_owned));
        };
        let (source, read) = if path.as_os_str() == "-" {
            (
                "the query from stdin".to_owned(),
                super::read_bounded(std::io::stdin(), MAX_QUERY_BYTES),
            )
        } else {
            (
                format!("--input {}", path.display()),
                std::fs::File::open(path)
                    .and_then(|file| super::read_bounded(file, MAX_QUERY_BYTES)),
            )
        };
        let bytes = read
            .map_err(|error| format!("Cannot read {source}: {error}"))?
            .ok_or_else(|| format!("The JSON query exceeds {} MiB.", MAX_QUERY_BYTES >> 20))?;
        // Editors on Windows save UTF-8 with a byte-order mark; JSON never
        // starts with one.
        let text = &bytes[octocode_engine::text::bom_len(&bytes)..];
        String::from_utf8(text.to_vec())
            .map(Some)
            .map_err(|error| format!("Cannot read {source}: {error}"))
    }
}

impl ToolArgs {
    /// The positional JSON query: one argument that starts with `{`.
    pub fn json_arg(&self) -> Option<&str> {
        match self.args.as_slice() {
            [only] if only.trim_start().starts_with('{') => Some(only),
            _ => None,
        }
    }
}

/// One tool sub-command. The tool list and each command's `about` come from
/// the embedded tool contract, so the CLI never spells the tool set itself.
#[derive(Debug)]
pub(super) struct ToolCommand {
    pub name: &'static str,
    pub args: ToolArgs,
    /// The query the flags and positionals built; `None` for the JSON form.
    pub flags: Result<Option<serde_json::Value>, String>,
}

/// `(name, about)` for every tool in the embedded contract: its
/// `shortDescription`, led by the beta gate for a beta tool. Both come from
/// generated constants, so building the argument parser never parses the
/// multi-megabyte contract.
fn contract_tools() -> impl Iterator<Item = (&'static str, String)> {
    ToolId::ALL.into_iter().map(|tool| {
        let about = tool.short_description();
        let about = if tool.is_beta() {
            let beta = octocode_native::config::field::LOCAL_BETA;
            format!(
                "Beta, disabled by default (set {}=true or {}:true). {about}",
                beta.env_name().unwrap_or(beta.path),
                beta.path
            )
        } else {
            about.to_owned()
        };
        (tool.as_str(), about)
    })
}

fn contract_tool_name(name: &str) -> Option<&'static str> {
    ToolId::from_name(name).map(ToolId::as_str)
}

impl FromArgMatches for ToolCommand {
    fn from_arg_matches(matches: &ArgMatches) -> Result<Self, clap::Error> {
        Self::from_arg_matches_mut(&mut matches.clone())
    }

    fn from_arg_matches_mut(matches: &mut ArgMatches) -> Result<Self, clap::Error> {
        let Some((name, sub_matches)) = matches.remove_subcommand() else {
            return Err(clap::Error::raw(
                clap::error::ErrorKind::MissingSubcommand,
                "a tool name is required",
            ));
        };
        let Some(name) = contract_tool_name(&name) else {
            return Err(clap::Error::raw(
                clap::error::ErrorKind::InvalidSubcommand,
                format!("unrecognized subcommand '{name}'"),
            ));
        };
        // Read without taking: flags::query needs the positionals' places.
        let args = ToolArgs::from_arg_matches(&sub_matches)?;
        let positionals: &[String] = if args.json_arg().is_some() {
            &[]
        } else {
            &args.args
        };
        let flags = super::flags::query(name, &sub_matches, positionals);
        Ok(Self { name, args, flags })
    }

    fn update_from_arg_matches(&mut self, matches: &ArgMatches) -> Result<(), clap::Error> {
        *self = Self::from_arg_matches(matches)?;
        Ok(())
    }
}

impl Subcommand for ToolCommand {
    fn augment_subcommands(command: clap::Command) -> clap::Command {
        contract_tools().fold(command, |command, (name, about)| {
            command.subcommand(super::flags::augment(
                ToolArgs::augment_args(clap::Command::new(name)).about(about),
                name,
            ))
        })
    }

    fn augment_subcommands_for_update(command: clap::Command) -> clap::Command {
        contract_tools().fold(command, |command, (name, about)| {
            command.subcommand(super::flags::augment(
                ToolArgs::augment_args_for_update(clap::Command::new(name)).about(about),
                name,
            ))
        })
    }

    fn has_subcommand(name: &str) -> bool {
        contract_tool_name(name).is_some()
    }
}

#[derive(Subcommand)]
pub(super) enum AuthCommand {
    /// Show whether GitHub accepts the active token, and its source (never the token).
    Status,
    /// Authenticate with GitHub using device flow, or refresh a stored token.
    Login {
        /// GitHub API hostname (override for GitHub Enterprise device login).
        #[arg(long)]
        hostname: Option<String>,
        /// Re-authenticate and replace stored credentials only after a successful login.
        #[arg(long)]
        force: bool,
        /// Refresh the stored token using its refresh token instead of a new device flow.
        #[arg(long)]
        refresh: bool,
    },
    /// Remove stored GitHub credentials from Octocode home and the OS store.
    Logout,
}

#[derive(Subcommand)]
pub(super) enum Command {
    // ── Tools: one command per contract tool, named exactly like the tool ────
    #[command(flatten)]
    Tool(ToolCommand),

    // ── System commands ──────────────────────────────────────────────────────
    /// List tools or query fields; --view full shows the complete reference.
    #[command(long_about = SCHEMA_HELP)]
    Schema {
        /// Tool name, e.g. `localSearch`. Omit to list every enabled tool.
        tool: Option<String>,
        /// View: query (default for a tool), variants, or full (also the catalog).
        #[arg(long, value_parser = ["query", "variants", "full"])]
        view: Option<String>,
        /// Narrow the query view to one branch by a const field, e.g. `operation=symbols`.
        #[arg(long, value_name = "FIELD=VALUE", requires = "tool")]
        select: Option<String>,
    },
    /// Show configuration files and loaded keys, print the home path, or get and set .env keys.
    #[command(args_conflicts_with_subcommands = true, disable_help_subcommand = true)]
    Config {
        #[command(subcommand)]
        command: Option<ConfigCommand>,
        /// Private JSON management transport used by the local config view.
        #[arg(long, hide = true)]
        manage: bool,
    },
    /// GitHub authentication: `status`, `login`, or `logout`.
    #[command(disable_help_subcommand = true)]
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    /// Build (`ingest <path>`) or query (`query <op>`) a persisted code graph in <workspace>/.octocode/graph.
    #[command(long_about = super::graph::GRAPH_HELP, disable_help_subcommand = true)]
    Graph {
        #[command(subcommand)]
        command: super::graph::GraphCommand,
    },
    /// Install, remove, or check bundled Octocode skills (`octocode skill --help`).
    #[command(disable_help_subcommand = true)]
    Skill {
        #[command(subcommand)]
        command: super::skill::SkillCommand,
    },
    /// Add the Octocode MCP server to an agent client's configuration.
    Install(super::mcp_install::InstallArgs),

    // ── Hidden commands (not part of the agent surface) ──────────────────────
    /// Serve warm read-only tool calls for one workspace over a private socket.
    #[command(hide = true)]
    Serve {
        /// Socket path.
        #[arg(long)]
        socket: std::path::PathBuf,
    },
    /// Manage auto-downloadable language servers (`list`, `install`, `uninstall`, `clean`, `status`).
    #[command(name = "lsp-server", hide = true, disable_help_subcommand = true)]
    LspServer {
        #[command(subcommand)]
        command: super::lsp_provision::LspServerCommand,
    },
}

const SCHEMA_HELP: &str =
    "Discover tools and query fields. Full reference remains available on demand.

  octocode schema                         enabled tools and purposes
  octocode schema <tool>                  compact query schema
  octocode schema <tool> --select operation=symbols
                                          one operation's query schema
  octocode schema <tool> --view variants  required fields and examples
  octocode schema <tool> --view full      complete tool contract
  octocode schema --view full             fields, instructions and grammars

JSON: indented on a terminal, one line on a pipe.";

#[derive(Subcommand, Debug)]
pub(super) enum ConfigCommand {
    /// Open the local settings, API keys, and agent configuration view.
    View {
        /// Print the local session URL without opening a browser.
        #[arg(long)]
        no_open: bool,
        /// Close the local server after this many seconds without an authenticated request.
        #[arg(long, default_value_t = 900, value_parser = clap::value_parser!(u64).range(30..=3600))]
        idle_timeout: u64,
    },
    /// Print the Octocode home directory (holds the global .env, .octocoderc, skills, and caches).
    Home,
    /// Print a key's resolved value (environment, then workspace .env, then global .env); exit 1 when unset.
    Get {
        /// Environment key or setting path, e.g. `OCTOCODE_CLASSIFICATION_API` or `classification.api`.
        key: String,
    },
    /// Set a key in the global .env: `set KEY VALUE`, or `set KEY --stdin` to keep secrets out of shell history.
    Set {
        /// Environment key or setting path, e.g. `OCTOCODE_CLASSIFICATION_API` or `classification.api`.
        key: String,
        /// Value to store; omit with `--stdin`.
        value: Option<String>,
        /// Read the value from stdin.
        #[arg(long, conflicts_with = "value")]
        stdin: bool,
    },
    /// Remove every assignment of a key from the global .env.
    Unset {
        /// Environment key or setting path to remove, e.g. `OCTOCODE_BETA`.
        key: String,
    },
    /// Exit 0 when a key is set (from the environment or a .env file), 1 when unset; never prints the value.
    Check {
        /// Environment key or setting path, e.g. `GITHUB_TOKEN`.
        key: String,
    },
}

#[cfg(test)]
mod tests {
    use super::ToolArgs;

    /// A query file saved with a UTF-8 byte-order mark reads as its JSON.
    #[test]
    fn an_input_file_with_a_byte_order_mark_reads_as_its_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("query.json");
        std::fs::write(&file, "\u{feff}{\"queries\":[]}").expect("query file");
        let args = ToolArgs {
            args: Vec::new(),
            input: Some(file),
        };
        assert_eq!(
            args.query_text().expect("readable"),
            Some("{\"queries\":[]}".to_owned())
        );
    }
}
