mod human;
mod search;
use clap::{Parser, Subcommand};
use octocode_engine_tools_core::config::RuntimeSurface;
use octocode_engine_tools_core::runtime::{HostOptions, ToolRuntime};
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
    /// Search local files with native regex and ignore-aware traversal.
    #[command(alias = "s")]
    Search(Box<search::SearchArgs>),
    #[command(external_subcommand)]
    Pattern(Vec<String>),
    /// Continue a bounded read using an opaque native token.
    Next {
        token: String,
        #[arg(long)]
        all: bool,
    },
    /// Read sanitized file content.
    Read {
        path: String,
        #[arg(long)]
        lines: Option<String>,
        #[arg(long)]
        full: bool,
        /// Follow every executable page while the source remains unchanged.
        #[arg(long)]
        all: bool,
        #[arg(long)]
        r#match: Option<String>,
        #[arg(long)]
        regex: bool,
        #[arg(short = 'i', long)]
        ignore_case: bool,
        #[arg(short = 'C', long)]
        context: Option<usize>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        offset: Option<usize>,
        #[arg(long, value_parser = ["lines", "bytes"])]
        chunk: Option<String>,
        #[arg(long, value_parser = ["none", "standard", "symbols"])]
        minify: Option<String>,
    },
    /// Inspect configuration without printing secret values.
    Config {
        #[arg(long, conflicts_with = "check")]
        keys: bool,
        #[arg(long)]
        check: Option<String>,
    },
    /// Explicit structured interface for protocol consumers and Cargo tests.
    Tools {
        tool: Option<String>,
        #[arg(long)]
        queries: Option<String>,
        #[arg(long)]
        scheme: bool,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        compact: bool,
    },
    /// List files through astSearch.
    Files(human::FilesArgs),
    /// Show a filesystem or syntax tree.
    Tree(human::TreeArgs),
    /// List declarations through astSearch symbols.
    Symbols(human::SymbolsArgs),
    /// Structural match through astSearch.
    Ast(human::AstArgs),
    /// File topology through astSearch.
    Graph(human::GraphArgs),
    /// Preview or apply a structural rewrite.
    Rewrite(human::RewriteArgs),
    /// Go to definition.
    Def(human::LspArgs),
    /// Find references.
    Refs(human::LspArgs),
    /// Incoming calls.
    Callers(human::LspArgs),
    /// Outgoing calls.
    Callees(human::LspArgs),
    /// Type definition.
    Type(human::LspArgs),
    /// File diagnostics.
    Diagnostics(human::LspArgs),
    /// Named lspSearch operation.
    Lsp(human::LspArgs),
    /// Search GitHub repositories.
    Repos(human::ReposArgs),
    /// Clone a GitHub repository into local cache.
    Clone(human::CloneArgs),
    /// Look up a package across registries.
    Package(human::PackageArgs),
    /// Search or read GitHub history.
    History(human::HistoryArgs),
    /// Print enabled tools and MCP instructions.
    Context {
        #[arg(long)]
        full: bool,
        #[arg(long)]
        json: bool,
    },
    /// Runtime home, storage, auth, and available tools.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Check GitHub authentication without printing secrets.
    Auth {
        action: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Explain how to authenticate.
    Login,
    /// Explain how to clear stored credentials.
    Logout,
    /// Inspect or clear native GitHub caches.
    Cache { action: String },
    /// Skill catalog pointer.
    Skill { action: Option<String> },
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
            json: _,
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
                true,
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
        Command::Lsp(args) => human::lsp(runtime, "documentSymbols", args).await,
        Command::Repos(args) => human::repos(runtime, args).await,
        Command::Clone(args) => human::clone_repo(runtime, args).await,
        Command::Package(args) => human::package(runtime, args).await,
        Command::History(args) => human::history(runtime, args).await,
        Command::Context { full, json } => human::context(runtime, json, full).await,
        Command::Status { json } => human::status(runtime, json).await,
        Command::Auth { action, json } => match action.as_deref().unwrap_or("status") {
            "status" => human::auth_status(runtime, json),
            other => {
                eprintln!("Unknown auth action: {other}");
                2
            }
        },
        Command::Login => human::login(),
        Command::Logout => human::logout(runtime),
        Command::Cache { action } => human::cache(runtime, &action),
        Command::Skill { action } => human::skill(action.as_deref()),
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
                    Some(octocode_engine_tools_core::runtime::FailureKind::NotFound) => 3,
                    // The frozen raw-tool CLI classifies the legacy 401 message
                    // as a tool failure. Human commands use the typed auth code.
                    Some(octocode_engine_tools_core::runtime::FailureKind::Authentication) => {
                        if structured {
                            5
                        } else {
                            4
                        }
                    }
                    Some(octocode_engine_tools_core::runtime::FailureKind::Permission) => 4,
                    Some(octocode_engine_tools_core::runtime::FailureKind::RateLimited) => 7,
                    Some(octocode_engine_tools_core::runtime::FailureKind::Execution) => 5,
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
                        if octocode_engine_tools_core::runtime::response::is_partial(&row["data"]) {
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
    failure: Option<octocode_engine_tools_core::runtime::FailureKind>,
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
            if failure == Some(octocode_engine_tools_core::runtime::FailureKind::NotFound) =>
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
