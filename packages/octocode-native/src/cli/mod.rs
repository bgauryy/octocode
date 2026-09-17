mod commands;
mod human;
mod lsp_provision;
mod mcp_install;
mod mcp_sync;
mod search;
use clap::Parser;
use commands::Command;
use octocode_native::config::RuntimeSurface;
use octocode_native::runtime::{HostOptions, ToolRuntime};
use serde_json::{Value, json};
use std::io::{self, Write};

#[derive(Parser)]
#[command(name = "octocode", version, about = "Native Octocode research tools")]
pub struct Args {
    /// Emit {"success":false,"error":"..."} to stdout on errors instead of stderr text.
    #[arg(long, global = true)]
    json_errors: bool,
    #[command(subcommand)]
    command: Command,
}

fn emit_error(msg: &str, json_errors: bool) {
    if json_errors {
        println!("{}", json!({"success": false, "error": msg}));
    } else {
        eprintln!("{msg}");
    }
}

fn parse_github_reference(
    reference: &str,
    explicit_branch: Option<String>,
) -> Result<(String, String, String, Option<String>), &'static str> {
    let (reference, suffix_branch) = match reference.rsplit_once('@') {
        Some((path, branch)) if !branch.is_empty() => (path, Some(branch.to_owned())),
        _ => (reference, None),
    };
    let mut branch = explicit_branch.or(suffix_branch);
    let parts = if reference.starts_with("https://") || reference.starts_with("http://") {
        let url = url::Url::parse(reference).map_err(|_| "fetch: invalid GitHub URL")?;
        if !matches!(url.host_str(), Some("github.com") | Some("www.github.com")) {
            return Err("fetch: URL host must be github.com");
        }
        url.path_segments()
            .map(|segments| {
                segments
                    .filter(|segment| !segment.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        reference
            .trim_start_matches('/')
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    if parts.len() < 2 {
        return Err("fetch: expected owner/repo[/path][@branch]");
    }
    let owner = parts[0].clone();
    let repo = parts[1].trim_end_matches(".git").to_owned();
    if owner.is_empty() || repo.is_empty() {
        return Err("fetch: expected owner/repo[/path][@branch]");
    }
    let path = if parts.get(2).is_some_and(|part| part == "blob") && parts.len() >= 5 {
        branch.get_or_insert_with(|| parts[3].clone());
        parts[4..].join("/")
    } else if parts.get(2).is_some_and(|part| part == "raw") && parts.len() >= 5 {
        branch.get_or_insert_with(|| parts[3].clone());
        parts[4..].join("/")
    } else {
        parts[2..].join("/")
    };
    Ok((owner, repo, path, branch))
}

pub async fn run(args: Args) -> u8 {
    let json_errors = args.json_errors;
    let runtime = match ToolRuntime::from_host(HostOptions {
        surface: RuntimeSurface::Cli,
        // Use 120 s instead of the default 60 s so LSP cold-start initialisation
        // (which can take ~60 s on the first invocation) completes without a timeout.
        timeout_secs: Some(120),
        ..HostOptions::default()
    }) {
        Ok(runtime) => runtime,
        Err(error) => {
            emit_error(&format!("{}: {}", error.code, error.message), json_errors);
            return 5;
        }
    };
    let result = dispatch(args.command, json_errors, &runtime).await;
    runtime.close().await;
    result
}

async fn dispatch(command: Command, json_errors: bool, runtime: &ToolRuntime) -> u8 {
    match command {
        Command::Pattern(args) => {
            // Direct tool-name dispatch: `octocode localSearch '{...}'`
            // Allows bypassing human wrappers for raw tool JSON queries.
            const KNOWN_TOOLS: &[&str] = &[
                "localSearch",
                "localFetch",
                "astSearch",
                "astRewrite",
                "lspSearch",
                "ghSearch",
                "ghGetFileContent",
                "ghSearchHistory",
                "ghGetHistoryItem",
                "ghCloneRepo",
                "artifactSearch",
            ];
            if let Some(tool_name) = args.first().map(|s| s.as_str())
                && KNOWN_TOOLS.contains(&tool_name)
            {
                let tool = tool_name.to_owned();
                let rest = &args[1..];
                let compact = rest.iter().any(|s| s == "--compact");
                let pretty = rest.iter().any(|s| s == "--pretty");
                let scheme = rest
                    .iter()
                    .any(|argument| matches!(argument.as_str(), "--scheme" | "--schema"));
                if scheme {
                    return match runtime.catalog() {
                        Ok(catalog) => {
                            let value = catalog["tools"]
                                .as_array()
                                .and_then(|ts| ts.iter().find(|t| t["name"] == tool))
                                .cloned()
                                .unwrap_or(Value::Null);
                            if value.is_null() {
                                eprintln!("Unknown tool: {tool}");
                                2
                            } else {
                                write_json(&value, !pretty)
                            }
                        }
                        Err(error) => {
                            eprintln!("{}", error.message);
                            5
                        }
                    };
                }
                let json_arg = rest
                    .iter()
                    .find(|s| s.starts_with('{') || s.starts_with('['));
                return match json_arg {
                    Some(json_str) => match serde_json::from_str::<Value>(json_str) {
                        Ok(input) => {
                            execute(
                                runtime,
                                &tool,
                                input,
                                ExecuteOptions {
                                    structured: true,
                                    compact,
                                    json_errors,
                                    ..ExecuteOptions::default()
                                },
                            )
                            .await
                        }
                        Err(parse_error) => {
                            emit_error(&format!("Invalid JSON query: {parse_error}"), json_errors);
                            2
                        }
                    },
                    None => {
                        eprintln!("Usage: octocode {tool} '<json>'");
                        eprintln!("       octocode {tool} --scheme");
                        2
                    }
                };
            }
            // Fall through to search pattern alias
            match search::SearchArgs::try_parse_from(
                std::iter::once("octocode".to_owned()).chain(args),
            ) {
                Ok(args) => execute_search(runtime, args, json_errors).await,
                Err(error) => {
                    let _ = error.print();
                    2
                }
            }
        }
        Command::Search(args) => execute_search(runtime, *args, json_errors).await,
        Command::Next { token, all } => match runtime.resume_token(&token) {
            Ok((tool, query, digest)) => {
                execute(
                    runtime,
                    &tool,
                    query,
                    ExecuteOptions {
                        all,
                        expected_source: digest,
                        json_errors,
                        ..ExecuteOptions::default()
                    },
                )
                .await
            }
            Err(error) => {
                emit_error(&format!("{}: {}", error.code, error.message), json_errors);
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
            match (tool.as_deref(), scheme, queries.as_deref()) {
                // `tools` or `tools --json` — human-readable catalog
                (None, false, None) => match runtime.catalog() {
                    Ok(catalog) => {
                        if json || compact {
                            return write_json(&catalog, compact);
                        }
                        let tools_arr = catalog["tools"]
                            .as_array()
                            .map(|v| v.as_slice())
                            .unwrap_or(&[]);
                        let enabled = tools_arr.iter().filter(|t| t["available"] == true).count();
                        println!("Tools ({enabled}/{} enabled):", tools_arr.len());
                        println!();
                        println!("  Tip: use tool names directly — `octocode <toolName> '<json>'`");
                        println!("       or inspect schema — `octocode <toolName> --scheme`");
                        println!();
                        let families = ["GitHub", "Local Code", "Package", "Other"];
                        for family in families {
                            let family_tools: Vec<_> = tools_arr
                                .iter()
                                .filter(|t| {
                                    human::tool_family(t["name"].as_str().unwrap_or("")) == family
                                })
                                .collect();
                            if family_tools.is_empty() {
                                continue;
                            }
                            println!("  {family}:");
                            for t in family_tools {
                                let name = t["name"].as_str().unwrap_or_default();
                                let avail = t["available"].as_bool().unwrap_or(false);
                                let desc = t["description"].as_str().unwrap_or("");
                                let short = if desc.len() > 72 { &desc[..72] } else { desc };
                                let flag = if avail { " " } else { "!" };
                                println!("  [{flag}] {name:<30} {short}");
                            }
                        }
                        0
                    }
                    Err(error) => {
                        eprintln!("{}", error.message);
                        5
                    }
                },
                // `tools <name> --scheme` or `tools --scheme` with optional name
                (name, true, _) => match runtime.catalog() {
                    Ok(catalog) => {
                        let value = if let Some(n) = name {
                            catalog["tools"]
                                .as_array()
                                .and_then(|ts| ts.iter().find(|t| t["name"] == n))
                                .cloned()
                                .unwrap_or(Value::Null)
                        } else {
                            catalog
                        };
                        if value.is_null() {
                            eprintln!("Unknown tool: {}", name.unwrap_or("(none)"));
                            2
                        } else {
                            write_json(&value, compact)
                        }
                    }
                    Err(error) => {
                        eprintln!("{}", error.message);
                        5
                    }
                },
                // `tools <name> '<json>'` — execute tool
                (Some(name), false, Some(json_str)) => {
                    let input = match serde_json::from_str::<Value>(json_str) {
                        Ok(v) => v,
                        Err(error) => {
                            emit_error(&format!("Invalid JSON query: {error}"), json_errors);
                            return 2;
                        }
                    };
                    execute(
                        runtime,
                        name,
                        input,
                        ExecuteOptions {
                            structured: json || compact,
                            compact,
                            json_errors,
                            ..ExecuteOptions::default()
                        },
                    )
                    .await
                }
                // `tools <name>` without json or scheme — show usage hint
                (Some(name), false, None) => {
                    eprintln!("Usage: octocode tools {name} '<json>'");
                    eprintln!("       octocode tools {name} --scheme");
                    eprintln!("  Or use the tool name directly:");
                    eprintln!("       octocode {name} '<json>'");
                    eprintln!("       octocode {name} --scheme");
                    2
                }
                // queries without tool name
                (None, false, Some(_)) => {
                    eprintln!("Usage: octocode tools <toolName> '<json>'");
                    2
                }
            }
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
                    emit_error("--lines requires START:END", json_errors);
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
            execute(
                runtime,
                "localFetch",
                query,
                ExecuteOptions {
                    all,
                    json_errors,
                    ..ExecuteOptions::default()
                },
            )
            .await
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
            all,
        } => {
            let (owner, repo_name, file_path, branch_final) =
                match parse_github_reference(&r#ref, branch) {
                    Ok(parsed) => parsed,
                    Err(message) => {
                        emit_error(message, json_errors);
                        return 2;
                    }
                };
            let mut query = json!({
                "owner": owner,
                "repo": repo_name,
                "path": file_path,
            });
            if let Some(b) = branch_final {
                query["branch"] = json!(b);
            }
            if let Some(lines) = lines {
                let Some((start, end)) = lines
                    .split_once(':')
                    .and_then(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?)))
                else {
                    emit_error("--lines requires START:END", json_errors);
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
            execute(
                runtime,
                "ghGetFileContent",
                query,
                ExecuteOptions {
                    structured: pretty,
                    compact: pretty,
                    all,
                    json_errors,
                    ..ExecuteOptions::default()
                },
            )
            .await
        }
        Command::Files(args) => human::files(runtime, args).await,
        Command::Tree(args) => human::tree(runtime, args).await,
        Command::Symbols(args) => human::symbols(runtime, args).await,
        Command::Ast(args) => human::ast(runtime, args).await,
        Command::Graph(args) => human::graph(runtime, args).await,
        Command::Rewrite(args) => human::rewrite(runtime, args).await,
        Command::Def(args) => human::lsp(runtime, "definition", args).await,
        Command::Refs(args) => human::lsp(runtime, "references", args).await,
        Command::Hover(args) => human::hover(runtime, args).await,
        Command::Callers(args) => human::callers(runtime, args).await,
        Command::Callees(args) => human::callees(runtime, args).await,
        Command::TypeDef(args) => human::type_def(runtime, args).await,
        Command::Implementation(args) => human::implementation(runtime, args).await,
        Command::Supertypes(args) => human::supertypes(runtime, args).await,
        Command::Subtypes(args) => human::subtypes(runtime, args).await,
        Command::Diagnostics(args) => human::lsp(runtime, "diagnostic", args).await,
        Command::Repos(args) => human::repos(runtime, args).await,
        Command::Code(args) => human::code_search(runtime, args).await,
        Command::GhTree(args) => human::gh_tree(runtime, args).await,
        Command::Clone(args) => human::clone_repo(runtime, args).await,
        Command::Package(args) => human::package(runtime, args).await,
        Command::History(args) => human::history(runtime, args).await,
        Command::Context {
            full,
            minimal,
            json,
        } => human::context(runtime, json, full, minimal).await,
        Command::Status {
            hostname,
            json,
            sync,
        } => human::status(runtime, hostname.as_deref(), json, sync).await,
        Command::Auth { json } => human::auth_status(runtime, json).await,
        Command::Login { refresh } => human::login(runtime, refresh).await,
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
            method,
            backup,
            rollback,
        } => mcp_install::run(mcp_install::InstallArgs {
            ide,
            force,
            dry_run,
            check,
            list,
            json,
            enable_local,
            pass_env,
            method,
            backup,
            rollback,
        }),
        Command::LspServer {
            action,
            names,
            all,
            yes,
            force,
            json,
        } => lsp_provision::run(&action, names, all, yes, force, json).await,
    }
}

#[derive(Default)]
pub(super) struct ExecuteOptions {
    structured: bool,
    compact: bool,
    all: bool,
    expected_source: Option<String>,
    json_errors: bool,
}

pub(super) async fn execute(
    runtime: &ToolRuntime,
    tool: &str,
    mut input: Value,
    options: ExecuteOptions,
) -> u8 {
    let ExecuteOptions {
        structured,
        compact,
        all,
        mut expected_source,
        json_errors,
    } = options;
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
                if expected_source
                    .as_ref()
                    .is_some_and(|expected| outcome.source_digest.as_ref() != Some(expected))
                {
                    eprintln!("staleCursor: Source changed during continuation; restart the read.");
                    return 6;
                }
                let value = outcome.structured_content;
                let mut exit =
                    match outcome.failure {
                        Some(octocode_native::runtime::FailureKind::NotFound) => 3,
                        // The frozen raw-tool CLI classifies the legacy 401 message
                        // as a tool failure. Human commands use the typed auth code.
                        Some(octocode_native::runtime::FailureKind::Authentication) => {
                            if structured { 5 } else { 4 }
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
                for row in value["results"].as_array().into_iter().flatten() {
                    if row["status"] == "error" && !structured && !json_errors {
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
                        if tool == "localSearch" {
                            if let Err(error) = search::write_row(row, &value) {
                                return if error.kind() == io::ErrorKind::BrokenPipe {
                                    0
                                } else {
                                    5
                                };
                            }
                        } else if row["data"]["content"].as_str().is_none() {
                            let _ = write_json(&row["data"], true);
                        }
                        if octocode_native::runtime::response::is_partial(&row["data"]) {
                            if let Some(call) = row
                                .pointer("/data/next/continue")
                                .or_else(|| row.pointer("/data/next/nextMatchPage"))
                                .or_else(|| row.pointer("/data/next/nextPage"))
                            {
                                let digest = outcome.source_digest.as_deref();
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
                                                    expected_source = digest;
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
                        // Emit a structured JSON error to stdout so callers can parse it.
                        // Previously this went to stderr only, causing silent empty output
                        // (e.g. lspSearch timeout on cold start when stderr is discarded).
                        let hint = if error.code == "timeout" {
                            Some(
                                "Retry -- the first call initialises the language server (~60 s cold start).",
                            )
                        } else {
                            None
                        };
                        let mut v = json!({
                            "error": error.message,
                            "errorCode": error.code,
                        });
                        if let Some(h) = hint {
                            v["hints"] = json!([h]);
                        }
                        write_json(&v, compact);
                    }
                } else if json_errors {
                    emit_error(&format!("{}: {}", error.code, error.message), true);
                } else {
                    eprintln!("{}: {}", error.code, error.message);
                    if error.code == "timeout" {
                        eprintln!(
                            "Hint: retry -- the first call initialises the language server (~60 s cold start)."
                        );
                    }
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

fn read_error(data: &Value, failure: Option<octocode_native::runtime::FailureKind>) -> String {
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
                data["path"]
                    .as_str()
                    .or_else(|| data["resolvedPath"].as_str())
                    .unwrap_or_default()
            )
        }
        _ => data["error"]
            .as_str()
            .unwrap_or("Tool execution failed")
            .into(),
    }
}

async fn execute_search(runtime: &ToolRuntime, args: search::SearchArgs, json_errors: bool) -> u8 {
    let queries = match args.queries() {
        Ok(queries) => queries,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    if args.quiet {
        // Quiet mode: check each path independently; succeed on first match.
        let mut any_failure = false;
        for query in queries {
            match runtime
                .execute("cli-quiet".into(), "localSearch".into(), query)
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
                        return 0;
                    }
                    if result.failure.is_some() {
                        any_failure = true;
                    }
                }
                Err(error) => {
                    eprintln!("{}: {}", error.code, error.message);
                    return if error.code == "invalidInput" { 2 } else { 5 };
                }
            }
        }
        return if any_failure { 5 } else { 1 };
    }
    if queries.len() > 1 {
        // Multiple paths: each source owns its snapshot and continuation chain.
        let mut code = 1;
        for query in queries {
            let current = execute(
                runtime,
                "localSearch",
                query,
                ExecuteOptions {
                    structured: args.json || args.compact,
                    compact: args.compact,
                    all: args.all,
                    json_errors,
                    ..ExecuteOptions::default()
                },
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
        queries.into_iter().next().unwrap_or_default(),
        ExecuteOptions {
            structured: args.json || args.compact,
            compact: args.compact,
            all: args.all,
            json_errors,
            ..ExecuteOptions::default()
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::parse_github_reference;

    #[test]
    fn parses_copied_github_blob_urls() {
        let (owner, repo, path, branch) = parse_github_reference(
            "https://github.com/rust-lang/rust/blob/main/README.md#L1",
            None,
        )
        .expect("GitHub URL");
        assert_eq!(owner, "rust-lang");
        assert_eq!(repo, "rust");
        assert_eq!(path, "README.md");
        assert_eq!(branch.as_deref(), Some("main"));
    }

    #[test]
    fn explicit_branch_overrides_reference_branch() {
        let (_, _, path, branch) =
            parse_github_reference("rust-lang/rust/README.md@main", Some("stable".into()))
                .expect("short reference");
        assert_eq!(path, "README.md");
        assert_eq!(branch.as_deref(), Some("stable"));
    }
}
