use crate::{
    catalog, database,
    store::{Store, strip_nulls},
};
use anyhow::{Result, anyhow, bail};
use clap::Parser;
use serde_json::{Value, json};
use std::{
    io::{Read, Write, stdout},
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

#[derive(Parser, Debug)]
#[command(disable_help_flag = true, version)]
pub struct Args {
    #[arg(long, default_value = ".")]
    pub workspace: PathBuf,
    #[arg(long)]
    pub database: Option<PathBuf>,
    #[arg(long)]
    pub session: Option<String>,
    #[arg(long)]
    pub vendor: Option<String>,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub prompt: Option<String>,
    #[arg(long)]
    pub tools: Option<String>,
    #[arg(long)]
    pub duration_ms: Option<u64>,
    #[arg(long)]
    pub trace: bool,
    #[arg(long)]
    pub managed: bool,
    #[arg(long)]
    pub help: bool,
    pub args: Vec<String>,
}
/// Write a protocol frame exactly as given (JSON-RPC needs `"id":null`).
pub fn emit(value: &Value) -> Result<()> {
    let mut out = stdout().lock();
    serde_json::to_writer(&mut out, value)?;
    writeln!(out)?;
    out.flush()?;
    Ok(())
}
pub fn output(value: &Value) -> Result<()> {
    let mut value = value.clone();
    strip_nulls(&mut value);
    emit(&value)
}
fn arity(args: &[String], min: usize, max: usize) -> Result<()> {
    if args.len() < min || args.len() > max {
        bail!("Unexpected positional arguments; use --help or schema");
    }
    Ok(())
}
pub fn run() -> Result<()> {
    let args = Args::parse();
    if args.help || args.args.is_empty() {
        return output(&if args.args.is_empty() {
            catalog::help()?
        } else {
            catalog::definition(&args.args.join(" "))?
        });
    }
    let command = &args.args[0];
    if args.managed && command != "mcp" {
        bail!("--managed is supported only for mcp");
    }
    let rest = &args.args[1..];
    if let Some(selection) = args.tools.as_deref() {
        if !(matches!(command.as_str(), "mcp" | "run") || command == "schema" && rest == ["tools"])
        {
            bail!("--tools is supported only for mcp, run, and schema tools");
        }
        catalog::selected_tools(Some(selection))?;
    }
    match command.as_str() {
        "view" => {
            arity(rest, 0, 1)?;
            let input: Value =
                serde_json::from_str(rest.first().map(String::as_str).unwrap_or("{}"))?;
            catalog::command("view", &input)?;
            return crate::view::run(&args, &input);
        }
        "host-hook" | "host-config" => {
            arity(rest, 0, 0)?;
            return if command == "host-hook" {
                crate::host_hooks::run(&args)
            } else {
                crate::host_hooks::config(&args)
            };
        }
        "skill" => {
            arity(rest, 0, 0)?;
            return output(
                &json!({"instructions": catalog::skill_instructions(args.vendor.as_deref())}),
            );
        }
        "schema" => {
            let value = match rest
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice()
            {
                [] => catalog::catalog()?,
                ["tools"] => json!(catalog::selected_tools(args.tools.as_deref())?),
                ["entities"] => catalog::catalog()?["entities"].clone(),
                ["entity", action @ ("get" | "list" | "set")] => {
                    catalog::definition(&format!("entity {action}"))?
                }
                ["entity", name] => catalog::entity(name)?,
                _ => catalog::definition(&rest.join(" "))?,
            };
            return output(&value);
        }
        "db" => {
            if matches!(
                rest.first().map(String::as_str),
                Some("retention" | "compact")
            ) {
                arity(rest, 1, 2)?;
                let input: Value =
                    serde_json::from_str(rest.get(1).map(String::as_str).unwrap_or("{}"))?;
                if rest[0] == "compact" {
                    catalog::command("db compact", &input)?;
                }
                let path = database::path(args.database.as_deref())?;
                return output(&if rest[0] == "retention" {
                    crate::retention::report(&path, &input)?
                } else {
                    crate::retention::compact(&path)?
                });
            }
            if rest.first().is_some_and(|action| action == "export") {
                arity(rest, 2, 2)?;
                let input: Value = serde_json::from_str(&rest[1])?;
                catalog::command("db export", &input)?;
                return output(&database::export(
                    &database::path(args.database.as_deref())?,
                    std::path::Path::new(catalog::text(&input, "path")?),
                )?);
            }
            arity(rest, 1, 1)?;
            if rest[0] == "protocol" {
                return output(&json!({
                    "protocol": include_str!("../docs/DB.md"),
                    "database": catalog::catalog()?["database"],
                }));
            }
            if rest[0] != "info" {
                bail!("Use db info, db protocol, db export, db retention, or db compact");
            }
            return output(&database::inspect(
                &database::path(args.database.as_deref())?,
                &args.workspace,
            )?);
        }
        "mcp" => {
            arity(rest, 0, 0)?;
            return crate::mcp::run(&args);
        }
        "run" => {
            arity(rest, 0, 0)?;
            return crate::proxy::run(&args);
        }
        "listen" => {
            arity(rest, 0, 0)?;
            return crate::dispatch::listen(&args);
        }
        "entity" => {
            arity(rest, 2, 4)?;
            let action = rest[0].as_str();
            let name = rest[1].as_str();
            catalog::entity(name)?;
            let session = args
                .session
                .as_deref()
                .ok_or_else(|| anyhow!("--session required"))?;
            let id = rest.get(2).map(String::as_str).unwrap_or("");
            let input = match action {
                "get" => {
                    arity(rest, 3, 3)?;
                    json!({})
                }
                "list" => {
                    arity(rest, 2, 3)?;
                    // Store::entity_list/entity_set validate; parse only here.
                    serde_json::from_str(rest.get(2).map(String::as_str).unwrap_or("{}"))?
                }
                "set" => {
                    arity(rest, 4, 4)?;
                    serde_json::from_str(&rest[3])?
                }
                _ => bail!("Use entity get, list, or set"),
            };
            let store = Store::open(
                database::path(args.database.as_deref())?,
                &args.workspace,
                action != "set",
                false,
            )?;
            return output(&match action {
                "get" => store.entity_get(session, name, id)?,
                "list" => store.entity_list(session, name, &input)?,
                _ => store.entity_set(session, name, id, &input)?,
            });
        }
        _ => {}
    }
    let wait = command == "inbox" && rest.first().is_some_and(|s| s == "wait");
    arity(
        rest,
        usize::from(wait),
        if wait {
            2
        } else if command == "mcp" {
            0
        } else {
            1
        },
    )?;
    let argument = rest
        .get(usize::from(wait))
        .map(String::as_str)
        .unwrap_or("{}");
    let mut stdin = String::new();
    if argument == "-" {
        std::io::stdin()
            .take(8 * 1024 * 1024 + 1)
            .read_to_string(&mut stdin)?;
        if stdin.len() > 8 * 1024 * 1024 {
            bail!("JSON input exceeds 8 MiB");
        }
    }
    let input: Value = serde_json::from_str(if argument == "-" { &stdin } else { argument })?;
    let name = if wait { "inbox wait" } else { command.as_str() };
    // Validate once: Store::call, attach, retry_delivery and record_usage validate
    // at their own ingress. `join` stays here so bad input creates no DB.
    if matches!(
        name,
        "activity"
            | "join"
            | "inbox wait"
            | "completion-check"
            | "hook"
            | "dispatch"
            | "confirm_delivery"
            | "mcp"
    ) {
        catalog::command(name, &input)?;
    } else {
        catalog::definition(name)?;
    }
    if command == "activity" {
        return output(&crate::activity::read(&args.workspace, &input)?);
    }
    let session = args.session.as_deref().unwrap_or("");
    if !matches!(command.as_str(), "join" | "peers" | "prune" | "health")
        && session.trim().is_empty()
    {
        bail!("--session required");
    }
    let store = Store::open(
        database::path(args.database.as_deref())?,
        &args.workspace,
        matches!(
            command.as_str(),
            "peers"
                | "inbox"
                | "read_document"
                | "context"
                | "check_paths"
                | "locks"
                | "check_write"
                | "health"
                | "completion-check"
        ),
        command == "join",
    )?;
    match command.as_str() {
        "completion-check" => return output(&store.completion_check(session, &input)?),
        "attach" => return output(&store.attach(session, &input)?),
        "hook" => return crate::dispatch::hook(&store, session, &input),
        "dispatch" => return output(&crate::dispatch::dispatch(&store, session)?),
        "retry_delivery" => return output(&store.retry_delivery(session, &input)?),
        "record_usage" => return output(&store.record_usage(session, &input)?),
        "confirm_delivery" => {
            let items = input["items"]
                .as_array()
                .ok_or_else(|| anyhow!("items required"))?;
            store.finish_dispatch(session, items, None)?;
            return output(&json!({"submitted":true}));
        }
        _ => {}
    }
    if wait {
        let deadline =
            Instant::now() + Duration::from_millis(input["timeoutMs"].as_u64().unwrap_or(30_000));
        loop {
            let page = store.inbox(session, input["after"].as_i64().unwrap_or(0))?;
            if page["items"].as_array().is_some_and(|a| !a.is_empty()) || Instant::now() >= deadline
            {
                return output(&page);
            }
            thread::sleep(
                Duration::from_millis(250).min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }
    output(&store.call(session, command, &input)?)
}
#[cfg(test)]
mod tests {
    #[test]
    fn compaction_drops_null_fields_only() {
        let mut raw = serde_json::json!({"next":null,"items":[{"topic":null,"id":1},null],"hook":{},"empty":[],"text":""});
        super::strip_nulls(&mut raw);
        assert_eq!(
            raw,
            serde_json::json!({"items":[{"id":1},null],"hook":{},"empty":[],"text":""})
        );
    }
}
