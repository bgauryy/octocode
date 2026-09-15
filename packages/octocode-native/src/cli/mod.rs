mod human;
mod mcp_install;
mod search;
use clap::{Parser, Subcommand};
use octocode_native::config::RuntimeSurface;
use octocode_native::runtime::{HostOptions, ToolRuntime};
use serde_json::{Value, json};
use std::io::{self, Write};

#[derive(Parser)]
#[command(name = "octocode", version, about = "Native Octocode research tools")]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Search for text or a regex pattern across local files (alias: s).
    #[command(alias = "s")]
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
        #[arg(long)]
        pretty: bool,
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
        /// Raw JSON query object or array (positional; omit with --scheme to print the schema).
        queries: Option<String>,
        /// Print the input schema for the given tool instead of executing it.
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
    /// List all call sites that call this function (incoming call hierarchy).
    Callers(human::LspArgs),
    /// List all functions called by this function (outgoing call hierarchy).
    Callees(human::LspArgs),
    /// Jump to the type definition of the symbol under the cursor.
    Type(human::LspArgs),
    /// Show LSP diagnostics (errors, warnings, hints) for a source file.
    Diagnostics(human::LspArgs),
    /// Search GitHub repositories by keyword.
    Repos(human::ReposArgs),
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
        /// Emit JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Show runtime status: home directory, storage, authentication, and available tools.
    Status {
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
    },
}

pub async fn run(args: Args) -> u8 {
    let runtime = match ToolRuntime::from_host(HostOptions {
        surface: RuntimeSurface::Cli,
        ..HostOptions::default()
    }) {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("{}: {}", error.code, error.message);
            return 5;
        }
    };
    let result = dispatch(args.command, &runtime).await;
    runtime.close().await;
    result
}

async fn dispatch(command: Command, runtime: &ToolRuntime) -> u8 {
    match command {
        Command::Pattern(args) => {
            match search::SearchArgs::try_parse_from(
                std::iter::once("octocode".to_owned()).chain(args),
            ) {
                Ok(args) => execute_search(runtime, args).await,
                Err(error) => {
                    let _ = error.print();
                    2
                }
            }
        }
        Command::Search(args) => execute_search(runtime, *args).await,
        Command::Next { token, all } => match runtime.resume_token(&token) {
            Ok((tool, query, digest)) => {
                execute(runtime, &tool, query, false, false, all, Some(digest)).await
            }
            Err(error) => {
                eprintln!("{}: {}", error.code, error.message);
                2
            }
        },
        Command::Config { keys, check } => {
            let view = runtime.inspect_config();
            if let Some(key) = check {
                let set = runtime
                    .config()
                    .env_value(&key)
                    .is_some_and(|value| !value.is_empty());
                println!("{key}: {}", if set { "set" } else { "unset" });
                return if set { 0 } else { 1 };
            }
            if keys {
                for key in view.loaded_keys {
                    println!("{key}");
                }
            } else {
                println!(
                    "home: {}\nstorage: {}\nglobal keys: {}\nproject keys: {}",
                    view.home.display(),
                    view.storage_mode,
                    view.global_key_count,
                    view.project_key_count
                );
                for diagnostic in &view.diagnostics {
                    eprintln!("{}: {}", diagnostic.code, diagnostic.message);
                }
            }
            0
        }
        Command::Tools {
            tool,
            queries,
            scheme,
            json,
            compact,
        } => {
            if tool.is_none() || scheme {
                return match runtime.catalog() {
                    Ok(catalog) => {
                        let value = if let Some(name) = tool {
                            catalog["tools"]
                                .as_array()
                                .and_then(|tools| tools.iter().find(|tool| tool["name"] == name))
                                .cloned()
                                .unwrap_or(Value::Null)
                        } else {
                            catalog
                        };
                        if value.is_null() {
                            eprintln!("Unknown tool");
                            2
                        } else {
                            write_json(&value, compact)
                        }
                    }
                    Err(error) => {
                        eprintln!("{}", error.message);
                        5
                    }
                };
            }
            let input = match queries.and_then(|text| serde_json::from_str::<Value>(&text).ok()) {
                Some(input) => input,
                None => {
                    eprintln!("--queries requires a valid structured tool input");
                    return 2;
                }
            };
            execute(
                runtime,
                tool.as_deref().unwrap_or_default(),
                input,
                json || compact,
                compact,
                false,
                None,
            )
            .await
        }
        Command::Read {
            path,
            lines,
            full,
            all,
            r#match,
            regex,
            ignore_case,
            context,
            limit,
            offset,
            chunk,
            minify,
        } => {
            let mut query = json!({"path":path});
            if let Some(lines) = lines {
                let Some((start, end)) = lines
                    .split_once(':')
                    .and_then(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?)))
                else {
                    eprintln!("--lines requires START:END");
                    return 2;
                };
                query["startLine"] = json!(start);
                query["endLine"] = json!(end);
            }
            if full {
                query["fullContent"] = json!(true);
            }
            if let Some(pattern) = r#match {
                query["matchString"] = json!(pattern);
            }
            if regex {
                query["matchStringIsRegex"] = json!(true);
            }
            if ignore_case {
                query["matchStringCaseSensitive"] = json!(false);
            }
            if let Some(value) = context {
                query["contextLines"] = json!(value);
            }
            if let Some(value) = limit {
                query["limit"] = json!(value);
            }
            if let Some(value) = offset {
                query["offset"] = json!(value);
            }
            if let Some(value) = chunk {
                query["chunkType"] = json!(value);
            }
            if let Some(value) = minify {
                query["minify"] = json!(value);
            }
            execute(runtime, "localFetch", query, false, false, all, None).await
        }
        Command::Fetch {
            r#ref,
            branch,
            lines,
            full,
            r#match,
            regex,
            context,
            minify,
            pretty,
        } => {
            // Parse owner/repo[/path][@branch].
            let (path_part, ref_branch) = match r#ref.rsplit_once('@') {
                Some((p, b)) => (p, Some(b.to_owned())),
                None => (r#ref.as_str(), None),
            };
            let branch_final = branch.or(ref_branch);
            // Strip a leading https://github.com/ if the user pasted a URL.
            let stripped = path_part
                .trim_start_matches("https://github.com/")
                .trim_start_matches("http://github.com/");
            let parts: Vec<&str> = stripped.splitn(3, '/').collect();
            if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
                eprintln!("fetch: expected owner/repo[/path][@branch]");
                return 2;
            }
            let owner = parts[0];
            let repo_name = parts[1];
            let file_path = if parts.len() > 2 { parts[2] } else { "" };
            let mut query = json!({
                "owner": owner,
                "repo":  repo_name,
                "path":  file_path,
            });
            if let Some(b) = branch_final {
                query["branch"] = json!(b);
            }
            if let Some(lines) = lines {
                let Some((start, end)) = lines
                    .split_once(':')
                    .and_then(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?)))
                else {
                    eprintln!("--lines requires START:END");
                    return 2;
                };
                query["startLine"] = json!(start);
                query["endLine"] = json!(end);
            }
            if full {
                query["fullContent"] = json!(true);
            }
            if let Some(pattern) = r#match {
                query["matchString"] = json!(pattern);
            }
            if regex {
                query["matchStringIsRegex"] = json!(true);
            }
            if let Some(value) = context {
                query["contextLines"] = json!(value);
            }
            if let Some(value) = minify {
                query["minify"] = json!(value);
            }
            // pretty=false → raw content to stdout (mirrors `read`)
            // pretty=true  → structured indented JSON
            execute(runtime, "ghGetFileContent", query, pretty, pretty, false, None).await
        }
        Command::Files(args) => human::files(runtime, args).await,
        Command::Tree(args) => human::tree(runtime, args).await,
        Command::Symbols(args) => human::symbols(runtime, args).await,
        Command::Ast(args) => human::ast(runtime, args).await,
        Command::Graph(args) => human::graph(runtime, args).await,
        Command::Rewrite(args) => human::rewrite(runtime, args).await,
        Command::Def(args) => human::lsp(runtime, "definition", args).await,
        Command::Refs(args) => human::lsp(runtime, "references", args).await,
        Command::Callers(args) => human::lsp(runtime, "callers", args).await,
        Command::Callees(args) => human::lsp(runtime, "callees", args).await,
        Command::Type(args) => human::lsp(runtime, "typeDefinition", args).await,
        Command::Diagnostics(args) => human::lsp(runtime, "diagnostic", args).await,
        Command::Repos(args) => human::repos(runtime, args).await,
        Command::Clone(args) => human::clone_repo(runtime, args).await,
        Command::Package(args) => human::package(runtime, args).await,
        Command::History(args) => human::history(runtime, args).await,
        Command::Context { full, json } => human::context(runtime, json, full).await,
        Command::Status { json } => human::status(runtime, json).await,
        Command::Auth { json } => human::auth_status(runtime, json),
        Command::Login { refresh } => human::login(refresh).await,
        Command::Logout => human::logout(runtime),
        Command::Cache { action } => human::cache(runtime, &action),
        Command::Skill { args } => human::skill(&args),
        Command::Install {
            ide,
            force,
            dry_run,
            check,
            list,
            json,
            enable_local,
            pass_env,
        } => mcp_install::run(mcp_install::InstallArgs {
            ide,
            force,
            dry_run,
            check,
            list,
            json,
            enable_local,
            pass_env,
        }),
    }
}

pub(super) async fn execute(
    runtime: &ToolRuntime,
    tool: &str,
    mut input: Value,
    structured: bool,
    compact: bool,
    all: bool,
    mut expected_source: Option<String>,
) -> u8 {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let mut seen = std::collections::HashSet::new();
    let mut pages = 0;
    loop {
        pages += 1;
        let mut next_query = None;
        let execution = runtime.execute("cli-1".into(), tool.into(), input);
        tokio::pin!(execution);
        let result = tokio::select! {
            result = &mut execution => result,
            signal = tokio::signal::ctrl_c() => {
                if signal.is_ok() { runtime.requests.cancel("cli-1"); let _ = execution.await; return 130; }
                execution.await
            }
        };
        match result {
            Ok(outcome) => {
                if expected_source.as_ref().is_some_and(|expected| {
                    outcome.source_digests.first().and_then(Option::as_ref) != Some(expected)
                }) {
                    eprintln!("staleCursor: Source changed during continuation; restart the read.");
                    return 6;
                }
                let value = outcome.structured_content;
                let mut exit = match outcome.failure {
                    Some(octocode_native::runtime::FailureKind::NotFound) => 3,
                    // The frozen raw-tool CLI classifies the legacy 401 message
                    // as a tool failure. Human commands use the typed auth code.
                    Some(octocode_native::runtime::FailureKind::Authentication) => {
                        if structured {
                            5
                        } else {
                            4
                        }
                    }
                    Some(octocode_native::runtime::FailureKind::Permission) => 4,
                    Some(octocode_native::runtime::FailureKind::RateLimited) => 7,
                    Some(octocode_native::runtime::FailureKind::Execution) => 5,
                    None => 0,
                };
                if structured && !outcome.all_failed {
                    exit = 0;
                }
                if structured {
                    let code = write_json(&value, compact);
                    if code != 0 {
                        return code;
                    }
                }
                for (index, row) in value["results"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    if row["status"] == "error" && !structured {
                        let recoverable = matches!(
                            row["data"]["errorCode"].as_str(),
                            Some("fileTooLarge" | "fullContentLimit")
                        ) && row.pointer("/data/next/continue").is_some();
                        if !(all && recoverable) {
                            eprintln!("{}", read_error(&row["data"], outcome.failure));
                        }
                    }
                    if !structured {
                        if let Some(content) = row["data"]["content"].as_str()
                            && let Err(error) = io::stdout().lock().write_all(content.as_bytes())
                        {
                            return if error.kind() == io::ErrorKind::BrokenPipe {
                                0
                            } else {
                                5
                            };
                        }
                        if tool == "localSearch"
                            && let Err(error) = search::write_row(row, &value)
                        {
                            return if error.kind() == io::ErrorKind::BrokenPipe {
                                0
                            } else {
                                5
                            };
                        } else if row["data"]["content"].as_str().is_none() {
                            let _ = write_json(&row["data"], true);
                        }
                        if octocode_native::runtime::response::is_partial(&row["data"]) {
                            if let Some(call) = row
                                .pointer("/data/next/continue")
                                .or_else(|| row.pointer("/data/next/nextMatchPage"))
                                .or_else(|| row.pointer("/data/next/nextPage"))
                            {
                                let digest =
                                    outcome.source_digests.get(index).and_then(Option::as_deref);
                                if digest.is_none()
                                    && row["data"]["content"]
                                        .as_str()
                                        .is_some_and(|text| !text.is_empty())
                                {
                                    eprintln!("Incomplete read; source snapshot unavailable.");
                                    return 6;
                                }
                                match runtime.continuation_token(call, digest) {
                                    Ok(token) => {
                                        if all {
                                            if pages >= 10_000
                                                || std::time::Instant::now() >= deadline
                                            {
                                                eprintln!(
                                                    "Read limit reached. Continue: octocode next {token}"
                                                );
                                                return 6;
                                            }
                                            match runtime.resume_token(&token) {
                                                Ok((_, query, digest)) => {
                                                    expected_source = Some(digest);
                                                    let key = serde_json::to_string(&query)
                                                        .unwrap_or_default();
                                                    if !seen.insert(key) {
                                                        eprintln!(
                                                            "Continuation repeated; stopping incomplete read."
                                                        );
                                                        return 6;
                                                    }
                                                    next_query = Some(query);
                                                }
                                                Err(error) => {
                                                    eprintln!("{}: {}", error.code, error.message);
                                                    return 6;
                                                }
                                            }
                                        } else {
                                            eprintln!("Continue: octocode next {token}");
                                        }
                                    }
                                    Err(error) => {
                                        eprintln!(
                                            "Incomplete read; {}: {}",
                                            error.code, error.message
                                        )
                                    }
                                }
                            } else {
                                eprintln!("Incomplete read; select a smaller source-line range.");
                            }
                            if exit == 0 {
                                exit = 6;
                            }
                        }
                    }
                }
                if tool == "localSearch"
                    && exit == 0
                    && !structured
                    && value["results"]
                        .as_array()
                        .is_some_and(|rows| rows.iter().all(|row| row["status"] == "empty"))
                {
                    exit = 1;
                }
                if let Some(query) = next_query {
                    input = query;
                    continue;
                }
                return exit;
            }
            Err(error) => {
                if structured {
                    if let Some(payload) = error.payload {
                        write_json(&payload, compact);
                    } else {
                        eprintln!("{}: {}", error.code, error.message);
                    }
                } else {
                    eprintln!("{}: {}", error.code, error.message);
                }
                return if error.code == "invalidInput" { 2 } else { 5 };
            }
        }
    }
}

pub(super) fn write_json(value: &Value, compact: bool) -> u8 {
    let text = if compact {
        serde_json::to_string(value)
    } else {
        serde_json::to_string_pretty(value)
    };
    match text {
        Ok(text) => match writeln!(io::stdout().lock(), "{text}") {
            Ok(()) => 0,
            Err(error) if error.kind() == io::ErrorKind::BrokenPipe => 0,
            Err(_) => 5,
        },
        Err(_) => 5,
    }
}

fn read_error(
    data: &Value,
    failure: Option<octocode_native::runtime::FailureKind>,
) -> String {
    match data["errorCode"].as_str() {
        Some("fileTooLarge" | "fullContentLimit") => "Read exceeds the single-page limit.".into(),
        Some("contentSecurityLimit") => {
            "Selected content is too large to scan safely. Read a smaller line range.".into()
        }
        Some("binaryFileUnsupported") => format!(
            "Binary file cannot be read as text: {}",
            data["path"].as_str().unwrap_or_default()
        ),
        Some("fileAccessFailed")
            if failure == Some(octocode_native::runtime::FailureKind::NotFound) =>
        {
            format!(
                "File not found: {}",
                data["path"].as_str().unwrap_or_default()
            )
        }
        _ => data["error"]
            .as_str()
            .unwrap_or("Tool execution failed")
            .into(),
    }
}

async fn execute_search(runtime: &ToolRuntime, args: search::SearchArgs) -> u8 {
    let queries = match args.queries() {
        Ok(queries) => queries,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    if args.quiet {
        // Quiet mode still uses the bounded runtime, security and canonical validation.
        return match runtime
            .execute("cli-quiet".into(), "localSearch".into(), queries)
            .await
        {
            Ok(result) => {
                if result.structured_content["results"]
                    .as_array()
                    .is_some_and(|rows| {
                        rows.iter().any(|r| {
                            r["data"]["files"]
                                .as_array()
                                .is_some_and(|files| !files.is_empty())
                        })
                    })
                {
                    0
                } else if result.failure.is_some() {
                    5
                } else {
                    1
                }
            }
            Err(error) => {
                eprintln!("{}: {}", error.code, error.message);
                if error.code == "invalidInput" { 2 } else { 5 }
            }
        };
    }
    if args.all && args.paths.len() > 1 {
        // Each source owns its snapshot and continuation chain.
        let mut code = 1;
        for query in queries.as_array().into_iter().flatten() {
            let current = execute(
                runtime,
                "localSearch",
                query.clone(),
                false,
                false,
                true,
                None,
            )
            .await;
            if current > 1 {
                return current;
            }
            if current == 0 {
                code = 0;
            }
        }
        return code;
    }
    execute(
        runtime,
        "localSearch",
        queries,
        false,
        false,
        args.all,
        None,
    )
    .await
}
