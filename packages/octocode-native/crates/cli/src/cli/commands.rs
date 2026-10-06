//! The `Command` enum: one sub-command per native tool plus system commands.
//!
//! The CLI surface is intentionally minimal — every tool is invoked by its
//! canonical name with a raw JSON query, and `scheme` is the single discovery
//! command. No per-tool flag wrappers, no aliases.
use clap::{ArgMatches, Args, FromArgMatches, Subcommand};
use octocode_native::tools::id::ToolId;

/// Shared arguments for every tool sub-command: a raw JSON query (inline or
/// from a file) executed against the tool's contract.
#[derive(Args, Debug)]
pub(super) struct ToolArgs {
    /// Raw JSON query object, e.g. '{"queries":[…]}'. See `octocode scheme <tool>`.
    pub query: Option<String>,
    /// Read the JSON query from a file instead of inline shell-quoted JSON.
    #[arg(long, value_name = "FILE", conflicts_with = "query")]
    pub input: Option<std::path::PathBuf>,
    /// Emit indented JSON for humans (costs 25–55% more bytes for agents).
    #[arg(long)]
    pub pretty: bool,
}

impl ToolArgs {
    /// Resolve the JSON query text from `--input FILE` or the positional
    /// argument. `Ok(None)` means no query was supplied.
    pub fn query_text(&self) -> Result<Option<String>, String> {
        if let Some(path) = &self.input {
            return std::fs::read_to_string(path)
                .map(Some)
                .map_err(|error| format!("Cannot read --input {}: {error}", path.display()));
        }
        Ok(self.query.clone())
    }
}

/// One tool sub-command. The tool list and each command's `about` come from
/// the embedded tool contract, so the CLI never spells the tool set itself.
#[derive(Debug)]
pub(super) struct ToolCommand {
    pub name: &'static str,
    pub args: ToolArgs,
}

/// `(name, shortDescription)` for every tool in the embedded contract. Both
/// are generated constants, so building the argument parser never parses the
/// multi-megabyte contract.
fn contract_tools() -> impl Iterator<Item = (&'static str, &'static str)> {
    ToolId::ALL
        .into_iter()
        .map(|tool| (tool.as_str(), tool.short_description()))
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
    /// Show GitHub authentication status (token presence and source; no secrets printed).
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
    Logout,
}

#[derive(Subcommand)]
pub(super) enum Command {
    // ── Tools: one command per contract tool, named exactly like the tool ────
    #[command(flatten)]
    Tool(ToolCommand),

    // ── System commands ──────────────────────────────────────────────────────
    /// Print a tool contract; without a name, list tools, availability, and canonical agent instructions.
    Scheme {
        /// Tool name, e.g. `localSearch`. Omit to list all tools with availability.
        tool: Option<String>,
        /// Schema view: the public tool contract (default) or the self-contained query schema.
        #[arg(long, value_enum, requires = "tool")]
        view: Option<super::schema::SchemeView>,
        /// Select one union branch by a const field, e.g. `operation=code`. Requires `--view query`.
        #[arg(long, value_name = "FIELD=VALUE", requires = "tool")]
        select: Option<String>,
        /// Emit compact single-line JSON instead of indented JSON.
        #[arg(long)]
        compact: bool,
    },
    /// Print the global .env path (defaults to <HOME>/.octocode/.env).
    #[command(name = "showConfig")]
    ShowConfig {
        /// Emit the path and file existence as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Inspect configuration, edit keys, or open the local configuration view.
    #[command(args_conflicts_with_subcommands = true)]
    Config {
        #[command(subcommand)]
        command: Option<ConfigCommand>,
        /// Private JSON management transport used by the local config view.
        #[arg(long, hide = true, conflicts_with_all = ["check", "add", "remove", "value_stdin"])]
        manage: bool,
        /// Test whether a specific configuration key is set (prints set/unset, never the value). Exit 0 means set; exit 1 means unset.
        #[arg(long, value_name = "KEY", conflicts_with_all = ["add", "remove"])]
        check: Option<String>,
        /// Add or replace a global .env key: --add KEY VALUE (or --add KEY --value-stdin).
        #[arg(long, num_args = 1..=2, value_names = ["KEY", "VALUE"], conflicts_with = "remove")]
        add: Vec<String>,
        /// Read the --add value from stdin, keeping secrets out of shell history.
        #[arg(long, requires = "add")]
        value_stdin: bool,
        /// Remove every assignment for a key from the global .env only.
        #[arg(long, value_name = "KEY")]
        remove: Option<String>,
        /// Emit JSON output.
        #[arg(long)]
        json: bool,
    },
    /// GitHub authentication: `status` (default), `login`, or `logout`.
    Auth {
        #[command(subcommand)]
        command: Option<AuthCommand>,
        /// Emit JSON output (status only).
        #[arg(long)]
        json: bool,
    },
    /// Build (`ingest <path>`) or query (`query <op>`) a persisted code graph in <workspace>/.octocode/graph.
    #[command(long_about = super::graph::GRAPH_HELP)]
    Graph {
        #[command(subcommand)]
        command: super::graph::GraphCommand,
    },
    /// Manage bundled Octocode skills (`list`, `install`, `remove`, `check`, `info`); runs the npm launcher's `octocode skill`.
    Skill {
        /// Arguments forwarded verbatim to the npm launcher's `octocode skill` (e.g. `list --json`, `check --fix`, `info octocode-research`).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Install the Octocode MCP server into an IDE (Cursor, Windsurf, Claude Desktop, …).
    Install {
        /// Target IDE: `cursor`, `windsurf`, `claude` (= `claude-desktop`), `vscode` (= `vscode-cline`), `zed`, or another id from `--list`.
        #[arg(long)]
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
        /// List all supported IDE targets.
        #[arg(long)]
        list: bool,
        /// Emit JSON output.
        #[arg(long)]
        json: bool,
        /// Override whether local (filesystem) tools are enabled in the MCP server.
        #[arg(long)]
        enable_local: Option<bool>,
        /// Pass through additional environment variables to the MCP server process.
        #[arg(long)]
        pass_env: bool,
        /// Installation runner: "npx" (default), "bunx", or "pnpm".
        #[arg(long, value_parser = ["npx", "bunx", "pnpm"])]
        method: Option<String>,
        /// Restore an agent config from the .bak backup an earlier install left beside it.
        #[arg(long)]
        rollback: Option<String>,
    },

    // ── Hidden maintenance commands (not part of the agent surface) ──────────
    /// Show the cache home directory (`status`) or delete all cached GitHub responses (`clear`).
    #[command(hide = true)]
    Cache {
        /// `status` — print the cache home directory path; `clear` — delete all cached responses.
        #[arg(value_parser = ["status", "clear"])]
        action: String,
    },
    /// Manage auto-downloadable language servers (`list`, `install`, `uninstall`, `remove`, `clean`, `status`, `which`).
    #[command(name = "lsp-server", hide = true)]
    LspServer {
        /// Subcommand: `list`, `install <name...>`, `uninstall <name...>` (alias `remove`), `clean`, `status [file]`, or `which [file]`.
        #[arg(value_parser = ["list", "install", "uninstall", "remove", "clean", "status", "which"])]
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
        /// Emit JSON output.
        #[arg(long)]
        json: bool,
    },
}

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
}
