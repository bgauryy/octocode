//! `--all-pages`: run the call, then send each reply's top-level `next` to
//! its tool until a reply has none, and print one reply whose `results` are
//! the pages' rows joined once ([`join::Joiner`]): rows of one `index` in
//! order, `rowPart` pieces as one row, `shared.data` applied to each row. Row
//! `next.*` calls stay in their rows. A failed page ends the walk: the rows
//! joined so far print with that page's error and its `next`, so no evidence
//! is dropped silently. The walk mirrors the graph package's `readResponse`.
mod join;

use super::OutputMode;
use super::output::Printed;
use super::start_failed;
use super::tool_call::{Answered, answer, in_process_runtime};
use octocode_runtime_base::id::ToolId;
use serde_json::{Map, Value};
use std::collections::HashSet;

/// Walk every page of one call in this process and print the joined reply.
pub(super) async fn run(tool: &str, input: Value, mode: OutputMode) -> u8 {
    let runtime = match in_process_runtime(None) {
        Ok(runtime) => runtime,
        Err(error) => return start_failed(&error, mode),
    };
    let code = walk(&runtime, tool, input, mode).await;
    runtime.close().await;
    code
}

async fn walk(
    runtime: &octocode_native::runtime::ToolRuntime,
    tool: &str,
    input: Value,
    mode: OutputMode,
) -> u8 {
    let query = input.clone();
    let mut pages: Vec<Value> = Vec::new();
    let mut seen = HashSet::new();
    let (mut call_tool, mut call_input) = (tool.to_owned(), input);
    loop {
        if !seen.insert(format!("{call_tool} {call_input}")) {
            return refused(
                tool,
                &pages,
                &query,
                mode,
                "a continuation repeated; the evidence is incomplete",
            );
        }
        let printed = match answer(runtime, &call_tool, call_input.clone(), true).await {
            Answered::Printed(printed) => printed,
            Answered::Exit(code) => return code,
        };
        let Ok(page) = serde_json::from_str::<Value>(&printed.body) else {
            return printed.print(mode);
        };
        let failed = printed.exit > 1 || page.get("results").and_then(Value::as_array).is_none();
        if failed {
            // The first page prints as it came; a later one ends the walk with
            // the rows joined so far beside its error and its `next`.
            if pages.is_empty() {
                return printed.print(mode);
            }
            pages.push(page);
            return print_joined(tool, &pages, &query, printed.exit, mode);
        }
        let next = page.get("next").filter(|next| !next.is_null()).cloned();
        pages.push(page);
        let Some(next) = next else { break };
        let Some(next_input) = next.get("input").cloned() else {
            return refused(
                tool,
                &pages,
                &query,
                mode,
                "a reply's next has no input to send",
            );
        };
        call_tool = next
            .get("tool")
            .and_then(Value::as_str)
            .unwrap_or(&call_tool)
            .to_owned();
        call_input = next_input;
    }
    print_joined(tool, &pages, &query, 0, mode)
}

/// The pages of one walk as one reply: the first page's envelope without
/// its `next`, `expiresAt` and `replayed`, and every page's rows joined.
/// A later page that failed keeps its error fields and `next` in the reply.
pub(super) fn join_pages(tool: &str, pages: &[Value]) -> Result<Value, String> {
    let id = ToolId::from_name(tool).ok_or_else(|| format!("unknown tool {tool}"))?;
    let mut joiner = join::Joiner::new(id);
    let mut failure: Option<&Value> = None;
    for page in pages {
        let Some(rows) = page.get("results").and_then(Value::as_array) else {
            failure = Some(page);
            break;
        };
        let shared = page.pointer("/shared/data");
        for row in rows {
            let row = match (shared, row.get("data")) {
                (Some(shared), Some(data)) => {
                    let mut row = row.clone();
                    row["data"] = join::with_shared(shared, Some(data.clone()));
                    row
                }
                _ => row.clone(),
            };
            joiner.add(row)?;
        }
    }
    let first = pages
        .first()
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut reply: Map<String, Value> = first
        .into_iter()
        .filter(|(key, _)| !matches!(key.as_str(), "results" | "next" | "expiresAt" | "replayed"))
        .collect();
    let rows = if failure.is_some() {
        joiner.rows()
    } else {
        joiner.finish()?
    };
    reply.insert("results".to_owned(), Value::Array(rows));
    reply.insert("pages".to_owned(), Value::from(pages.len()));
    if let Some(failed) = failure.and_then(Value::as_object) {
        for (key, value) in failed {
            if key != "results" {
                reply.insert(key.clone(), value.clone());
            }
        }
    }
    Ok(Value::Object(reply))
}

fn print_joined(tool: &str, pages: &[Value], query: &Value, exit: u8, mode: OutputMode) -> u8 {
    match join_pages(tool, pages) {
        Ok(reply) => {
            let exit = if exit == 0 && reply["results"].as_array().is_some_and(Vec::is_empty) {
                1
            } else {
                exit
            };
            Printed::reply(tool, &reply, exit, mode.json, query, None, || {
                reply.to_string()
            })
            .print(mode)
        }
        Err(message) => refused(tool, pages, query, mode, &message),
    }
}

/// A walk that cannot rebuild its evidence: the rows read so far and why.
fn refused(tool: &str, pages: &[Value], query: &Value, mode: OutputMode, why: &str) -> u8 {
    let mut reply = Map::new();
    reply.insert(
        "error".to_owned(),
        Value::from(format!("--all-pages stopped: {why}")),
    );
    reply.insert("errorCode".to_owned(), Value::from("allPagesIncomplete"));
    reply.insert("pages".to_owned(), Value::from(pages.len()));
    if let Some(next) = pages.last().and_then(|page| page.get("next")) {
        reply.insert("next".to_owned(), next.clone());
    }
    let reply = Value::Object(reply);
    Printed::reply(tool, &reply, 1, mode.json, query, None, || {
        reply.to_string()
    })
    .print(mode)
}

#[cfg(test)]
mod tests {
    use super::join_pages;
    use serde_json::json;

    #[test]
    fn whole_rows_from_every_page_join_once_and_the_walk_envelope_is_dropped() {
        let pages = [
            json!({"results": [{"index": 0, "data": {"a": 1}}], "next": {"tool": "localSearch", "input": {"cursor": "c2"}}, "expiresAt": "x"}),
            json!({"results": [{"index": 1, "data": {"a": 2}}]}),
        ];
        let reply = join_pages("localSearch", &pages).expect("joined");
        assert_eq!(
            reply["results"],
            json!([{"index": 0, "data": {"a": 1}}, {"index": 1, "data": {"a": 2}}])
        );
        assert_eq!(reply["pages"], 2);
        assert!(reply.get("next").is_none() && reply.get("expiresAt").is_none());
    }

    #[test]
    fn shared_data_fills_each_row_without_overriding_its_own_fields() {
        let pages = [
            json!({"shared": {"data": {"path": "p", "kind": "k"}}, "results": [{"index": 0, "data": {"kind": "own"}}]}),
        ];
        let reply = join_pages("localSearch", &pages).expect("joined");
        assert_eq!(
            reply["results"][0]["data"],
            json!({"kind": "own", "path": "p"})
        );
    }

    #[test]
    fn a_failed_later_page_keeps_the_rows_read_and_reports_its_error_and_next() {
        let pages = [
            json!({"results": [{"index": 0, "data": {"a": 1}}], "next": {"input": {"cursor": "c2"}}}),
            json!({"error": "expired", "errorCode": "staleSnapshot", "next": {"input": {"restart": true}}}),
        ];
        let reply = join_pages("localSearch", &pages).expect("joined");
        assert_eq!(reply["results"], json!([{"index": 0, "data": {"a": 1}}]));
        assert_eq!(reply["errorCode"], "staleSnapshot");
        assert_eq!(reply["next"], json!({"input": {"restart": true}}));
    }

    #[test]
    fn a_row_whose_pieces_stop_before_the_last_is_an_error() {
        let pages = [
            json!({"results": [{"index": 0, "rowPart": {"part": 1, "of": 2, "text": "/data/text"}, "data": {"text": "ab"}}]}),
        ];
        assert!(join_pages("localSearch", &pages).is_err());
    }
}
