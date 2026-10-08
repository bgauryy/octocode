//! The `Command` enum: one sub-command per native tool plus system commands.
//!
//! The CLI surface is intentionally minimal — every tool is invoked by its
//! canonical name with a raw JSON query, and `schema` is the single discovery
//! command. No per-tool flag wrappers, no aliases.
use clap::{ArgMatches, Args, FromArgMatches, Subcommand};
use octocode_native::tools::id::ToolId;

/// Shared arguments for every tool sub-command: a raw JSON query (inline,
/// from a file, or from stdin) executed against the tool's contract.
#[derive(Args, Debug)]
pub(super) struct ToolArgs {
    /// JSON query: one query object, or '{"queries":[…]}' to batch. Fields: `octocode schema <tool> --view query`.
    pub query: Option<String>,
    /// Read the JSON query from a file, or from stdin with `-`.
    #[arg(long, value_name = "FILE|-", conflicts_with = "query")]
    pub input: Option<std::path::PathBuf>,
    /// Print JSON even on a terminal (pipes always get JSON).
    #[arg(long)]
    pub json: bool,
}

/// Largest JSON query read from `--input` (a file or stdin).
const MAX_QUERY_BYTES: u64 = 8 * 1024 * 1024;

impl ToolArgs {
    /// Resolve the JSON query text from `--input FILE|-` or the positional
    /// argument. `Ok(None)` means no query was supplied.
    pub fn query_text(&self) -> Result<Option<String>, String> {
        let Some(path) = &self.input else {
            return Ok(self.query.clone());
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
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|error| format!("Cannot read {source}: {error}"))
    }
}

/// One tool sub-command. The tool list and each command's `about` come from
/// the embedded tool contract, so the CLI never spells the tool set itself.
#[derive(Debug)]
pub(super) struct ToolCommand {
    pub name: &'static str,
    pub args: ToolArgs,
}

/// `(name, about)` for every tool in the embedded contract: its
/// `shortDescription`, led by the beta gate for a beta tool. Both come from
/// generated constants, so building the argument parser never parses the
/// multi-megabyte contract.
fn contract_tools() -> impl Iterator<Item = (&'static str, String)> {
    ToolId::ALL.into_iter().map(|tool| {
        let about = tool.short_description();
        let about = if tool.is_beta() {
            format!(
                "Beta, disabled by default (set OCTOCODE_BETA=true or local.beta:true). {about}"
            )
        } else {
            about.to_owned()
        };
        (tool.as_str(), about)
    })
}

fn contract_tool_name(name: &str) -> Option<&'static str> {
    contract_tools()
        .map(|(tool, _)| tool)
        .find(|tool| *tool == name)
}

impl FromArgMatches for ToolCommand {
    fn from_arg_matches(matches: &ArgMatches) -> Result<Self, clap::Error> {
        Self::from_arg_matches_mut(&mut matches.clone())
    }

    fn from_arg_matches_mut(matches: &mut ArgMatches) -> Result<Self, clap::Error> {
        let Some((name, mut sub_matches)) = matches.remove_subcommand() else {
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
        let args = ToolArgs::from_arg_matches_mut(&mut sub_matches)?;
        Ok(Self { name, args })
    }

    fn update_from_arg_matches(&mut self, matches: &ArgMatches) -> Result<(), clap::Error> {
        *self = Self::from_arg_matches(matches)?;
        Ok(())
    }
}

impl Subcommand for ToolCommand {
    fn augment_subcommands(command: clap::Command) -> clap::Command {
        contract_tools().fold(command, |command, (name, about)| {
            command.subcommand(ToolArgs::augment_args(clap::Command::new(name)).about(about))
        })
    }

    fn augment_subcommands_for_update(command: clap::Command) -> clap::Command {
        contract_tools().fold(command, |command, (name, about)| {
            command.subcommand(
                ToolArgs::augment_args_for_update(clap::Command::new(name)).about(about),
            )
        })
    }

    fn has_subcommand(name: &str) -> bool {
        contract_tool_name(name).is_some()
    }
}

#[derive(Subcommand)]
pub(super) enum AuthCommand {
    /// Show whether GitHub accepts the active token, and its source (never the token).
    Status {
        /// Emit JSON output.
        #[arg(long)]
        json: bool,
    },
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
        /// Emit JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Remove stored GitHub credentials from Octocode home and the OS store.
    Logout {
        /// Emit JSON output.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(super) enum Command {
    // ── Tools: one command per contract tool, named exactly like the tool ────
    #[command(flatten)]
    Tool(ToolCommand),

    // ── System commands ──────────────────────────────────────────────────────
    /// List tools with agent instructions, or print one tool's input contract.
    #[command(long_about = SCHEMA_HELP)]
    Schema {
        /// Tool name, e.g. `localSearch`. Omit to list every enabled tool.
        tool: Option<String>,
        /// Contract view of one tool.
        #[arg(long, value_parser = ["query", "variants", "full"], requires = "tool")]
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
        /// Print JSON.
        #[arg(long)]
        json: bool,
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
    Skill {
        /// Arguments for the npm launcher's `octocode skill`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Add the Octocode MCP server to an agent client's configuration.
    Install {
        /// Client id from `--list`, e.g. `claude-code`, `cursor`, `claude-desktop`.
        #[arg(long, value_name = "ID")]
        ide: Option<String>,
        /// Overwrite an existing MCP server entry.
        #[arg(long)]
        force: bool,
        /// Preview the config that would be written without making any changes.
        #[arg(long)]
        dry_run: bool,
        /// Verify that the MCP config already contains a valid Octocode entry.
        #[arg(long)]
        check: bool,
        /// List supported client ids.
        #[arg(long)]
        list: bool,
        /// Print JSON.
        #[arg(long)]
        json: bool,
        /// Override whether local (filesystem) tools are enabled in the MCP server.
        #[arg(long)]
        enable_local: Option<bool>,
        /// Pass through additional environment variables to the MCP server process.
        #[arg(long)]
        pass_env: bool,
        /// Installation runner (default npx).
        #[arg(long, value_parser = ["npx", "bunx", "pnpm"])]
        method: Option<String>,
        /// Restore a client config from the `.bak` file an earlier install left beside it.
        #[arg(long, value_name = "FILE")]
        rollback: Option<String>,
    },

    // ── Hidden commands (not part of the agent surface) ──────────────────────
    /// Machine tool catalog for the npm launcher's `schema`: availability, fields, fingerprint.
    #[command(hide = true)]
    Catalog,
    /// Serve warm lspSearch calls for one workspace over a private socket.
    #[command(hide = true)]
    Serve {
        /// Socket path.
        #[arg(long)]
        socket: std::path::PathBuf,
    },
    /// Show the cache home directory (`status`) or delete all cached GitHub responses (`clear`).
    #[command(hide = true)]
    Cache {
        #[arg(value_enum)]
        action: CacheAction,
    },
    /// Manage auto-downloadable language servers (`list`, `install`, `uninstall`, `clean`, `status`).
    #[command(name = "lsp-server", hide = true)]
    LspServer {
        /// Subcommand: `list`, `install <name...>`, `uninstall <name...>`, `clean`, or `status [file]`.
        #[arg(value_parser = ["list", "install", "uninstall", "clean", "status"])]
        action: String,
        /// Server names for install/uninstall (e.g. `rust-analyzer`, `clangd`).
        names: Vec<String>,
        /// Install every auto-downloadable server.
        #[arg(long)]
        all: bool,
        /// Confirm destructive operations (clean) or force re-download (install).
        #[arg(long)]
        yes: bool,
        /// Skip the auto-install prompt policy for this run.
        #[arg(long)]
        force: bool,
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub(super) enum CacheAction {
    /// Print the cache home directory and recent evictions.
    Status,
    /// Delete all cached GitHub responses.
    Clear,
}

const SCHEMA_HELP: &str = "List tools with agent instructions, or print one tool's input contract.

  octocode schema                                  enabled tools, fields, agent instructions
  octocode schema <tool>                           variants, usage, then the full contract
  octocode schema <tool> --view query              the self-contained query schema
  octocode schema <tool> --view query --select operation=symbols
                                                   one branch of the query schema
  octocode schema <tool> --view variants           branch names, selectors, examples

A terminal gets indented JSON; a pipe gets one line.";

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
    Home {
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
    /// Print a key's resolved value (environment, then workspace .env, then global .env); exit 1 when unset.
    Get {
        /// Environment key, e.g. `OCTOCODE_CLASSIFICATION_API`.
        key: String,
        /// Print JSON: key, value, and source (`environment`, `project`, or `global`).
        #[arg(long)]
        json: bool,
    },
    /// Set a key in the global .env: `set KEY VALUE`, or `set KEY --stdin` to keep secrets out of shell history.
    Set {
        /// Environment key, e.g. `OCTOCODE_CLASSIFICATION_API`.
        key: String,
        /// Value to store; omit with `--stdin`.
        value: Option<String>,
        /// Read the value from stdin.
        #[arg(long, conflicts_with = "value")]
        stdin: bool,
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
    /// Remove every assignment of a key from the global .env.
    Unset {
        /// Environment key to remove, e.g. `OCTOCODE_BETA`.
        key: String,
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
    /// Exit 0 when a key is set (from the environment or a .env file), 1 when unset; never prints the value.
    Check {
        /// Environment key, e.g. `GITHUB_TOKEN`.
        key: String,
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
}
