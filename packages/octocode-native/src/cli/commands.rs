//! The `Command` enum: maps every CLI sub-command to its argument struct.
use clap::Subcommand;
use super::{human, search};

#[derive(Subcommand)]
pub(super) enum Command {
    /// Search for text or a regex pattern across local files.
    Search(Box<search::SearchArgs>),
    #[command(external_subcommand)]
    Pattern(Vec<String>),
    /// Fetch the next page of a paginated `read` result using the token printed to stderr.
    Next {
        /// Continuation token printed to stderr as `Continue: octocode next <TOKEN>`.
        token: String,
        /// Drain every subsequent page automatically.
        #[arg(long)]
        all: bool,
    },
    /// Read a local file with optional pagination, line ranges, match filtering, and minification.
    Read {
        /// Path to the local file to read.
        path: String,
        /// Exact line range, e.g. `10:50` (1-based, inclusive).
        #[arg(long)]
        lines: Option<String>,
        /// Read the whole file in one response (up to 50 000 bytes).
        #[arg(long)]
        full: bool,
        /// Drain every page automatically until the whole file is returned.
        #[arg(long)]
        all: bool,
        /// Show only lines matching this text or pattern.
        #[arg(long)]
        r#match: Option<String>,
        /// Treat --match as a regular expression.
        #[arg(long)]
        regex: bool,
        /// Case-insensitive --match.
        #[arg(short = 'i', long)]
        ignore_case: bool,
        /// Lines of context around each --match hit.
        #[arg(short = 'C', long)]
        context: Option<usize>,
        /// Page size (lines or bytes depending on --chunk).
        #[arg(long)]
        limit: Option<usize>,
        /// Start offset (lines or bytes from the beginning of the file).
        #[arg(long)]
        offset: Option<usize>,
        /// Pagination unit: `lines` (default) or `bytes`.
        #[arg(long, value_parser = ["lines", "bytes"])]
        chunk: Option<String>,
        /// Content transformation: `none` exact, `standard` trim comments, `symbols` signatures only.
        #[arg(long, value_parser = ["none", "standard", "symbols"])]
        minify: Option<String>,
    },
    /// Read a file from a GitHub repository without cloning it locally.
    /// Reference format: `owner/repo/path`, `owner/repo/path@branch`, or a full GitHub URL.
    Fetch {
        /// GitHub reference: `owner/repo`, `owner/repo/path`, or `owner/repo/path@branch`.
        r#ref: String,
        /// Branch, tag, or commit SHA — overrides an @branch suffix in the reference.
        #[arg(long)]
        branch: Option<String>,
        /// Exact line range, e.g. `10:50` (1-based, inclusive).
        #[arg(long)]
        lines: Option<String>,
        /// Read the whole file in one response (up to 50 000 bytes).
        #[arg(long)]
        full: bool,
        /// Show only lines matching this text or pattern.
        #[arg(long)]
        r#match: Option<String>,
        /// Treat --match as a regular expression.
        #[arg(long)]
        regex: bool,
        /// Lines of context around each --match hit.
        #[arg(short = 'C', long)]
        context: Option<usize>,
        /// Content transformation: `none` exact, `standard` trim comments, `symbols` signatures only.
        #[arg(long, value_parser = ["none", "standard", "symbols"])]
        minify: Option<String>,
        /// Emit indented JSON instead of raw file content.
        #[arg(long, conflicts_with = "all")]
        pretty: bool,
        /// Drain every remote file-content page to raw stdout.
        #[arg(long)]
        all: bool,
    },
    /// Show active configuration keys and values (secrets are always redacted).
    Config {
        /// List configuration key names only, without values.
        #[arg(long, conflicts_with = "check")]
        keys: bool,
        /// Test whether a specific configuration key is set.
        #[arg(long)]
        check: Option<String>,
    },
    /// Call a tool by name with a JSON query, or inspect its schema with --scheme.
    Tools {
        /// Tool to call, e.g. `localSearch`, `astSearch`, `ghSearch`, `lspSearch`.
        tool: Option<String>,
        /// Raw JSON query object (positional; omit with --scheme to print the schema).
        queries: Option<String>,
        /// Print the complete contract for the given tool instead of executing it.
        #[arg(long)]
        scheme: bool,
        /// Emit structured JSON output.
        #[arg(long)]
        json: bool,
        /// Compact single-line JSON (implies --json).
        #[arg(long)]
        compact: bool,
    },
    /// Find files by name or glob within a directory.
    Files(human::FilesArgs),
    /// Show a directory tree, or the parsed syntax tree for a single source file (--syntax).
    Tree(human::TreeArgs),
    /// List declarations — functions, classes, types — in a file or directory.
    Symbols(human::SymbolsArgs),
    /// Search code by structure using ast-grep patterns (e.g. `fn $NAME($$$) { $$$ }`).
    Ast(human::AstArgs),
    /// Analyse the file import graph: dead code, cycles, dependencies, dependents, or reachability.
    Graph(human::GraphArgs),
    /// Find-and-replace code by structure using ast-grep patterns; previews changes before writing.
    Rewrite(human::RewriteArgs),
    /// Jump to the definition of a symbol at a given file and line.
    Def(human::LspArgs),
    /// Find all references to a symbol across the workspace.
    Refs(human::LspArgs),
    /// Show hover documentation for a symbol at a given file and line.
    Hover(human::LspArgs),
    /// Find all callers of a function (incoming call hierarchy).
    Callers(human::LspArgs),
    /// Find all callees of a function (outgoing call hierarchy).
    Callees(human::LspArgs),
    /// Jump to the type definition of a symbol.
    #[command(name = "type-def")]
    TypeDef(human::LspArgs),
    /// Find all implementations of a trait, interface, or abstract type.
    Implementation(human::LspArgs),
    /// Find supertypes of a type in the type hierarchy.
    Supertypes(human::LspArgs),
    /// Find subtypes of a type in the type hierarchy.
    Subtypes(human::LspArgs),
    /// Show LSP diagnostics (errors, warnings, hints) for a source file.
    Diagnostics(human::LspArgs),
    /// Search GitHub repositories by keyword.
    Repos(human::ReposArgs),
    /// Search GitHub code by keyword, owner, repo, path, or language.
    Code(human::CodeArgs),
    /// Browse a GitHub repository tree.
    #[command(name = "gh-tree")]
    GhTree(human::GhTreeArgs),
    /// Clone a GitHub repository into the local Octocode cache for offline access.
    Clone(human::CloneArgs),
    /// Look up or discover packages across npm, PyPI, crates.io, Maven, and 4 other registries.
    Package(human::PackageArgs),
    /// Search or read GitHub pull requests, issues, and commits (use `prs`/`issues`/`commits` to search, `pr`/`issue`/`commit` for a single item).
    History(human::HistoryArgs),
    /// Show which tools are enabled and the MCP server instructions for this workspace.
    Context {
        /// Include the full tool context with all available parameters.
        #[arg(long)]
        full: bool,
        /// Emit a compact one-line summary (enabled tool count + protocol).
        #[arg(long)]
        minimal: bool,
        /// Emit JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Show runtime status: home directory, storage, authentication, and available tools.
    Status {
        /// GitHub API hostname (override for GitHub Enterprise).
        #[arg(long)]
        hostname: Option<String>,
        /// Emit JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Show GitHub authentication status (token presence and scopes; no secrets printed).
    Auth {
        /// Emit JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Authenticate with GitHub using device flow, or refresh a stored token.
    Login {
        /// Force re-authentication even when credentials are already stored.
        #[arg(long)]
        refresh: bool,
    },
    /// Remove stored GitHub credentials from the native keychain.
    Logout,
    /// Show the cache home directory (`status`) or delete all cached GitHub responses (`clear`).
    Cache {
        /// `status` — print the cache home directory path; `clear` — delete all cached responses.
        #[arg(value_parser = ["status", "clear"])]
        action: String,
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
}


