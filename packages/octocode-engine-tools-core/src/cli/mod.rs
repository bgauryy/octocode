use crate::config::RuntimeSurface;
use crate::runtime::{HostOptions, ToolRuntime};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::io::{self, Write};

#[derive(Parser)]
#[command(name = "octo", version, about = "Native Octocode research tools")]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
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
    /// Explicit structured interface for parity tests and protocol consumers.
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
    }
}

async fn execute(
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
                    Some(crate::runtime::FailureKind::NotFound) => 3,
                    Some(crate::runtime::FailureKind::Execution) => 5,
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
                        eprintln!(
                            "{}",
                            row["data"]["error"]
                                .as_str()
                                .unwrap_or("Tool execution failed")
                        );
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
                        if crate::runtime::response::is_partial(&row["data"]) {
                            if let Some(call) = row.pointer("/data/next/continue") {
                                let Some(digest) =
                                    outcome.source_digests.get(index).and_then(Option::as_deref)
                                else {
                                    eprintln!("Incomplete read; source snapshot unavailable.");
                                    return 6;
                                };
                                match runtime.continuation_token(call, digest) {
                                    Ok(token) => {
                                        if all {
                                            if pages >= 10_000
                                                || std::time::Instant::now() >= deadline
                                            {
                                                eprintln!(
                                                    "Read limit reached. Continue: octo next {token}"
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
                                            eprintln!("Continue: octo next {token}");
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

fn write_json(value: &Value, compact: bool) -> u8 {
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
