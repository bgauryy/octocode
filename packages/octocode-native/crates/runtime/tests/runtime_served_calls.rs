//! A served call is its `tool` and `input`. On the CLI and the MCP transport
//! alike, no call a response offers (`next.*`, `related[]`,
//! `responsePagination.next`) states a `kind`: the channel that holds it says
//! whether it continues the answer or starts new work, and each one still
//! runs as served.
use crate::support::{Run as _, Workspace, mcp_payload};
use octocode_native::runtime::ToolRuntime;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::continuations::visit_calls;
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug)]
enum Transport {
    Cli,
    Mcp,
}

async fn served(runtime: &ToolRuntime, on: Transport, tool: &str, input: Value) -> Value {
    let id = format!("served-calls-{on:?}");
    match on {
        Transport::Cli => (*runtime
            .run(id, tool.into(), input.clone())
            .await
            .unwrap_or_else(|error| panic!("{tool} {input}: {error:?}"))
            .structured_content)
            .clone(),
        Transport::Mcp => mcp_payload(
            &runtime
                .run_mcp(id, tool.into(), input.clone())
                .await
                .unwrap_or_else(|error| panic!("{tool} {input}: {error:?}")),
        ),
    }
}

/// Every object in `value` that names a `tool` and an `input` and still
/// states a `kind`, wherever it sits.
fn calls_with_kind(value: &Value, found: &mut Vec<Value>) {
    match value {
        Value::Object(fields) => {
            if fields.contains_key("tool")
                && fields.contains_key("input")
                && fields.contains_key("kind")
            {
                found.push(value.clone());
            }
            fields
                .values()
                .for_each(|child| calls_with_kind(child, found));
        }
        Value::Array(items) => items.iter().for_each(|child| calls_with_kind(child, found)),
        _ => {}
    }
}

/// The channel of a call at `path`: the key that holds it (`related` for an
/// item of `related[]`, the `next` key, or `responsePagination.next`).
fn channel(path: &[String]) -> String {
    match path {
        [first, second] if first == "responsePagination" => format!("{first}.{second}"),
        _ => path
            .iter()
            .rev()
            .find(|part| part.parse::<usize>().is_err())
            .cloned()
            .unwrap_or_default(),
    }
}

/// Walk `start` on `on` to its end, following every continuation a page
/// serves; return each served call by channel, with its tool.
async fn walk(
    runtime: &ToolRuntime,
    on: Transport,
    tool: &str,
    start: Value,
) -> Vec<(String, Value)> {
    let id = ToolId::from_name(tool).expect("a runtime tool");
    let mut seen = Vec::new();
    let mut queue = std::collections::VecDeque::from([start]);
    let mut pages = 0;
    while let Some(input) = queue.pop_front() {
        pages += 1;
        assert!(pages < 200, "the walk terminates");
        let output = served(runtime, on, tool, input).await;
        let mut stated = Vec::new();
        calls_with_kind(&output, &mut stated);
        assert!(
            stated.is_empty(),
            "{on:?}: a served call states its kind: {stated:?}\n{output}"
        );
        octocode_runtime_base::contracts::validate_output(tool, &output)
            .unwrap_or_else(|error| panic!("{on:?} output contract: {error:?}\n{output}"));
        let mut calls = Vec::new();
        visit_calls(id, &output, &mut |call, path| {
            calls.push((channel(path), call.clone()));
        })
        .expect("declared call positions");
        for (channel, call) in calls {
            // Continuations of this answer run as served; a lead is new work.
            if channel != "related" && call["tool"] == tool {
                queue.push_back(call["input"].clone());
            }
            seen.push((channel, call));
        }
    }
    seen
}

#[tokio::test]
async fn served_calls_state_no_kind_on_either_transport_and_still_run() {
    let workspace = Workspace::new();
    for index in 0..120 {
        workspace.write(&format!("src/f{index:03}.txt"), "needle\n");
    }
    let runtime = workspace.runtime(&[("OCTOCODE_STORAGE_MODE", "persistent".into())]);
    let root = workspace.workspace.join("src");
    let batch = |chars: Option<u64>| {
        let mut input = json!({"queries": [
            {"path": root, "matchString": ["needle"], "limit": 50},
            {"path": workspace.workspace.join("src/missing"), "matchString": ["needle"]},
        ]});
        if let Some(chars) = chars {
            input["responseChars"] = json!(chars);
        }
        input
    };
    for on in [Transport::Cli, Transport::Mcp] {
        let mut channels = BTreeSet::new();
        for input in [batch(None), batch(Some(1000))] {
            for (channel, call) in walk(&runtime, on, "localSearch", input).await {
                assert!(
                    call["tool"].is_string() && call["input"].is_object(),
                    "{call}"
                );
                if channel == "related" {
                    // A lead is a fresh request; a continuation is its cursor.
                    assert!(call["input"]["queries"].is_array(), "{call}");
                } else {
                    assert!(call["input"]["cursor"].is_string(), "{call}");
                }
                channels.insert(channel);
            }
        }
        for expected in ["nextPage", "related", "responsePagination.next"] {
            assert!(
                channels.contains(expected),
                "{on:?}: no served {expected} call: {channels:?}"
            );
        }
    }
    runtime.close().await;
}
