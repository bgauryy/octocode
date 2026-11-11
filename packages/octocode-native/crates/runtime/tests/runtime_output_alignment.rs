//! Wave 0 of the one Octocode protocol: one call per tool, operation and row
//! state (hit, empty, error, paged) over provider doubles, each response
//! checked against the protocol's shape rules. A check fails on a deviation
//! the allowlist does not name, and on an allowlisted one that no longer
//! deviates, so the list only shrinks as the RFC's waves land
//! (`.octocode/octocode-rfc-generator/2026-11-10-one-protocol.md`).

mod allowlist;
mod checks;
mod fixtures;

use crate::support::Run as _;
use checks::{Findings, Report};
use fixtures::{Case, World};
use octocode_native::response::exit::ExitClass;
use serde_json::Value;
use std::collections::BTreeSet;

fn tool_facts(tool: &str) -> &'static Value {
    static NULL: Value = Value::Null;
    octocode_native::contracts::parsed_contract().expect("embedded contract")["tools"]
        .as_array()
        .and_then(|tools| tools.iter().find(|entry| entry["name"] == tool))
        .unwrap_or(&NULL)
}

/// The tool's row-level collections: `/data/<key>` in the contract's
/// `collectionPaths`, without the ones a row may leave out.
fn row_collections(tool: &str) -> Vec<String> {
    let facts = tool_facts(tool);
    let paths = |key: &str| -> Vec<&str> {
        facts[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect()
    };
    let optional = paths("optionalCollectionPaths");
    paths("collectionPaths")
        .into_iter()
        .filter(|path| !optional.contains(path))
        .filter_map(|path| path.strip_prefix("/data/"))
        .filter(|key| !key.contains('/'))
        .map(str::to_owned)
        .collect()
}

/// The state a response shows: its first row's status, else `paged` when a
/// continuation reads more, else `hit`.
fn state_of(output: &Value) -> String {
    if let Some(status) = output.pointer("/results/0/status").and_then(Value::as_str) {
        return status.to_owned();
    }
    if output.get("results").is_none() {
        return "rejected".to_owned();
    }
    let mut more = output.pointer("/responsePagination/next").is_some();
    let mut path = Vec::new();
    checks::each_node(output, &mut path, &mut |path, node| {
        more |= path.last().is_some_and(|key| key == "hasMore") && node == &Value::Bool(true);
    });
    if more { "paged" } else { "hit" }.to_owned()
}

/// A lead validates against its target's input schema and runs on the same
/// doubles without being rejected as invalid input. A remote tool that
/// writes a checkout is validated only: no HTTP double serves a clone.
async fn run_leads(
    world: &World,
    report: &mut Report<'_>,
    output: &Value,
    ran: &mut BTreeSet<String>,
) {
    let mut calls = Vec::new();
    let mut path = Vec::new();
    checks::each_node(output, &mut path, &mut |path, node| {
        if checks::is_call(node) {
            calls.push((checks::place(path), node.clone()));
        }
    });
    for (at, call) in calls {
        let target = call["tool"].as_str().unwrap_or_default().to_owned();
        let input = call["input"].clone();
        if !ran.insert(format!("{target} {input}")) {
            continue;
        }
        if octocode_native::contracts::validate(&target, input.clone()).is_err() {
            report.add("B3 lead", format!("invalid input: {at} -> {target}"));
            continue;
        }
        let facts = tool_facts(&target);
        if facts["mutatesWorkspace"] == true && facts["family"] != "local" {
            continue;
        }
        let rejected = match world
            .runtime
            .run("lead".into(), target.clone(), input)
            .await
        {
            Ok(outcome) => outcome.exit_class() == ExitClass::InvalidInput,
            Err(_) => true,
        };
        if rejected {
            report.add(
                "B3 lead",
                format!("rejected as invalid input: {at} -> {target}"),
            );
        }
    }
}

/// With `OCTOCODE_ALIGNMENT_DUMP=<file>`, the findings go to `<file>` and
/// every response to `<file>.outputs`, one JSON line per call: what to read
/// when a finding is new.
fn dump(case: &Case, output: &Value) {
    use std::io::Write as _;
    let Ok(file) = std::env::var("OCTOCODE_ALIGNMENT_DUMP") else {
        return;
    };
    let line = serde_json::json!({
        "tool": case.tool, "operation": case.operation, "state": case.state,
        "input": case.input, "output": output,
    });
    let mut outputs = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(format!("{file}.outputs"))
        .expect("outputs dump");
    writeln!(outputs, "{line}").expect("outputs dump");
}

async fn observe(world: &World, case: &Case, findings: &mut Findings, ran: &mut BTreeSet<String>) {
    let outcome = world
        .runtime
        .run("alignment".into(), case.tool.clone(), case.input.clone())
        .await
        .unwrap_or_else(|error| panic!("{} {}: {error:?}", case.tool, case.input));
    let output = outcome.structured_content.into_value();
    crate::support::checks::assert_output_contract(&case.tool, &output);
    dump(case, &output);
    let mut report = Report {
        tool: &case.tool,
        findings,
    };
    let state = state_of(&output);
    if state != case.state {
        report.add(
            "B1 state",
            format!("{} {}: {state}", case.operation, case.state),
        );
    }
    let data = output.pointer("/results/0/data").unwrap_or(&Value::Null);
    checks::envelope(&mut report, &output);
    checks::pagination(&mut report, &output);
    checks::call_places(&mut report, &output);
    if tool_facts(&case.tool)["family"] == "local" {
        checks::paths(&mut report, &output, &world.workspace.workspace);
    }
    checks::echoes(&mut report, &case.input["queries"][0], data);
    checks::uniform(&mut report, data);
    checks::formats(&mut report, &output);
    checks::key_order(&mut report, &output);
    checks::empty_rows(&mut report, &output, &row_collections(&case.tool));
    run_leads(world, &mut report, &output, ran).await;
    // The page a reader runs next keeps the envelope, paging and formats.
    if let Some(next) = crate::support::next_input(&output)
        && let Ok(page) = world
            .runtime
            .run("page".into(), case.tool.clone(), next)
            .await
    {
        let page = page.structured_content.into_value();
        checks::envelope(&mut report, &page);
        checks::pagination(&mut report, &page);
        checks::call_places(&mut report, &page);
        checks::formats(&mut report, &page);
        checks::key_order(&mut report, &page);
    }
}

#[tokio::test]
async fn every_tool_output_follows_the_protocol_or_is_allowlisted() {
    let mut findings = Findings::new();
    let mut reached = BTreeSet::new();
    for world in fixtures::worlds().await {
        let mut ran = BTreeSet::new();
        for case in &world.cases {
            observe(&world, case, &mut findings, &mut ran).await;
            reached.insert(case.tool.clone());
        }
        world.runtime.close().await;
    }
    let tools: BTreeSet<String> = octocode_native::contracts::parsed_contract()
        .expect("embedded contract")["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry["name"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(reached, tools, "every tool has at least one fixture call");

    if let Ok(file) = std::env::var("OCTOCODE_ALIGNMENT_DUMP") {
        let lines: Vec<&str> = findings.iter().map(String::as_str).collect();
        std::fs::write(file, lines.join("\n")).expect("dump");
    }
    let allowed: BTreeSet<String> = allowlist::all().map(str::to_owned).collect();
    let new: Vec<&String> = findings.difference(&allowed).collect();
    let fixed: Vec<&String> = allowed.difference(&findings).collect();
    assert!(
        new.is_empty() && fixed.is_empty(),
        "deviations the allowlist does not name:\n{new:#?}\nallowlisted, no longer deviating (delete them):\n{fixed:#?}"
    );
}
