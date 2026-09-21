//! The `Command` enum: one sub-command per native tool plus system commands.
//!
//! The CLI surface is intentionally minimal — every tool is invoked by its
//! canonical name with a raw JSON query, and `scheme` is the single discovery
//! command. No per-tool flag wrappers, no aliases.
use clap::{Args, Subcommand};

/// Shared arguments for every tool sub-command: a raw JSON query (inline or
/// from a file) executed against the tool's contract.
#[derive(Args, Debug)]
pub(super) struct ToolArgs {
    /// Raw JSON query object, e.g. '{"queries":[…]}'. See `octocode scheme <tool>`.
    pub query: Option<String>,
    /// Read the JSON query from a file instead of inline shell-quoted JSON.
    #[arg(long, value_name = "FILE", conflicts_with = "query")]
    pub input: Option<std::path::PathBuf>,
    /// Emit compact single-line JSON instead of indented JSON.
    #[arg(long)]
    pub compact: bool,
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
        /// Re-authenticate even when credentials are already stored (signs out first).
        #[arg(long)]
        force: bool,
        /// Refresh the stored token using its refresh token instead of a new device flow.
        #[arg(long)]
        refresh: bool,
        /// Emit JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Remove stored GitHub credentials from the native keychain.
    Logout,
}

#[derive(Subcommand)]
pub(super) enum Command {
    // ── Tools: one command per tool, named exactly like the tool ─────────────
    #[command(name = "localSearch")]
    LocalSearch(ToolArgs),
    #[command(name = "localFetch")]
    LocalFetch(ToolArgs),
    #[command(name = "astSearch")]
    AstSearch(ToolArgs),
    #[command(name = "astRewrite")]
    AstRewrite(ToolArgs),
    #[command(name = "lspSearch")]
    LspSearch(ToolArgs),
    #[command(name = "ghSearch")]
    GhSearch(ToolArgs),
    #[command(name = "ghGetFileContent")]
    GhGetFileContent(ToolArgs),
    #[command(name = "ghSearchHistory")]
    GhSearchHistory(ToolArgs),
    #[command(name = "ghGetHistoryItem")]
    GhGetHistoryItem(ToolArgs),
    #[command(name = "ghCloneRepo")]
    GhCloneRepo(ToolArgs),
    #[command(name = "artifactSearch")]
    ArtifactSearch(ToolArgs),
    #[command(name = "clasify")]
    Clasify(ToolArgs),

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
    /// Show configuration files and set key names. Values are never printed.
    Config {
        /// Test whether a specific configuration key is set (prints set/unset, never the value).
        #[arg(long, value_name = "KEY")]
        check: Option<String>,
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
    /// Run an Octocode skill — `list`, `install`, `run <name>`, or any other skill command.
    Skill {
        /// Arguments forwarded verbatim to `octocode skill` (e.g. `list`, `run octocode-research`).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Install the Octocode MCP server into an IDE (Cursor, Windsurf, Claude Desktop, …).
    Install {
        /// Target IDE: `cursor`, `windsurf`, `claude`, `vscode`, `zed`, or another supported editor.
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
        /// Write a .bak backup of the existing config before overwriting.
        #[arg(long)]
        backup: bool,
        /// Restore config from a .bak backup file written by a previous --backup install.
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
    /// Manage auto-downloadable language servers (`list`, `install`, `uninstall`, `clean`).
    #[command(name = "lsp-server", hide = true)]
    LspServer {
        /// Subcommand: `list`, `install <name...>`, `uninstall <name...>`, `clean`, `status [file]`, or `which [file]`.
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
