//! The hoist stage: each form on its own (holders, omitted values, the
//! batch), then the invariants every hoisted response keeps (VC4): every
//! emitted call runs unchanged, every cited value reads back by the one
//! reading rule, and a page of a hoisted response reads alone.
use super::pattern::Pattern;
use super::plan::{Batch, Holder, Keyed, Omit, Piece, Plan, When};
use super::*;
use serde_json::{Value, json};

mod batch;
mod holder;
mod invariants;
mod omit;
mod secrets;

/// A plan for tests: the entries given, nothing else.
fn plan(omit: Vec<Omit>, holders: Vec<Holder>, batch: Batch) -> Plan {
    Plan {
        omit,
        holders,
        batch,
        refs: Vec::new(),
        outline: Vec::new(),
    }
}

fn holder(at: &str, fields: &[&str], after: &[&str]) -> Holder {
    Holder {
        at: Pattern::parse(at),
        fields: fields.iter().map(|field| (*field).to_owned()).collect(),
        after: after.iter().map(|field| (*field).to_owned()).collect(),
    }
}

fn omitted(at: &str, when: When) -> Omit {
    Omit {
        at: Pattern::parse(at),
        when,
    }
}

/// The value at `path` of result row `row`'s `data` as a reader recovers it
/// from a hoisted `envelope` by the one reading rule: the item's own, else
/// the value its omission rule derives, else its nearest holder's (out to
/// `data`), else the envelope's `shared.data`.
fn recovered(plan: &Plan, envelope: &Value, row: usize, path: &[String]) -> Option<Value> {
    let data = &envelope["results"][row]["data"];
    if let Some(value) = data.pointer(&crate::json_pointer::of(path)) {
        return Some(value.clone());
    }
    for entry in &plan.omit {
        let Some(bound) = bound_by(&entry.at, path) else {
            continue;
        };
        let source = |pattern: &Pattern| recovered(plan, envelope, row, &pattern.bind(&bound)?);
        let derived = match &entry.when {
            When::Const(value) => Some(value.clone()),
            When::Equals(pattern) => source(pattern),
            When::CountOf(pattern) => source(pattern)?
                .as_array()
                .map(|items| Value::from(items.len())),
            When::Template(pieces) => pieces
                .iter()
                .map(|piece| match piece {
                    Piece::Text(text) => Some(text.clone()),
                    Piece::Field(pattern) => source(pattern)?.as_str().map(str::to_owned),
                })
                .collect::<Option<String>>()
                .map(Value::String),
        };
        if derived.is_some() {
            return derived;
        }
    }
    let (field, parent) = path.split_last()?;
    let mut holder = parent.to_vec();
    while !holder.is_empty() {
        holder.pop();
        let mut at = holder.clone();
        at.push(field.clone());
        if let Some(value) = data.pointer(&crate::json_pointer::of(&at)) {
            return Some(value.clone());
        }
    }
    envelope
        .pointer(&format!("/shared/data{}", crate::json_pointer::of(path)))
        .cloned()
}

/// The token each `*` of `pattern` takes to name `path`, or `None` when the
/// pattern does not name it.
fn bound_by(pattern: &Pattern, path: &[String]) -> Option<Vec<String>> {
    if pattern.tokens().len() != path.len() {
        return None;
    }
    let mut bound = Vec::new();
    for (token, step) in pattern.tokens().iter().zip(path) {
        if token == "*" {
            bound.push(step.clone());
        } else if token != step {
            return None;
        }
    }
    Some(bound)
}

/// Every leaf of `value` outside the calls it offers, as a path.
fn leaves(value: &Value) -> Vec<Vec<String>> {
    fn walk(value: &Value, path: &mut Vec<String>, found: &mut Vec<Vec<String>>) {
        match value {
            Value::Object(object) if super::pattern::is_call(object) => {}
            Value::Object(object) => {
                for (key, child) in object {
                    if matches!(key.as_str(), "next" | "related") {
                        continue;
                    }
                    path.push(key.clone());
                    walk(child, path, found);
                    path.pop();
                }
            }
            Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    path.push(index.to_string());
                    walk(child, path, found);
                    path.pop();
                }
            }
            _ => found.push(path.clone()),
        }
    }
    let mut found = Vec::new();
    walk(value, &mut Vec::new(), &mut found);
    found
}

/// Asserts every leaf of every row of `original` reads back from `hoisted`.
fn assert_recovers(plan: &Plan, original: &Value, hoisted: &Value) {
    for (row, result) in original["results"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        for path in leaves(&result["data"]) {
            if matches!(path[0].as_str(), "hints" | "warnings") {
                continue;
            }
            assert_eq!(
                recovered(plan, hoisted, row, &path).as_ref(),
                result["data"].pointer(&crate::json_pointer::of(&path)),
                "row {row} {}: {hoisted}",
                crate::json_pointer::of(&path)
            );
        }
        // A row's notes are its own and the envelope's: warnings hold for
        // every row, hints for every empty or failed row.
        for (key, applies) in [
            ("warnings", true),
            (
                "hints",
                matches!(
                    result["status"].as_str(),
                    Some("error" | "empty" | "outOfRange")
                ),
            ),
        ] {
            let held = |value: &Value| value.as_array().cloned().unwrap_or_default();
            let mut read = held(&hoisted["results"][row]["data"][key]);
            if applies {
                read.extend(held(&hoisted[key]));
            }
            for note in held(&result["data"][key]) {
                assert!(read.contains(&note), "row {row} {key} {note}: {hoisted}");
            }
        }
    }
}

/// Every executable call under `value`, with its path: what a reader runs.
fn calls(value: &Value) -> Vec<Value> {
    let mut found = Vec::new();
    fn walk(value: &Value, found: &mut Vec<Value>) {
        match value {
            Value::Object(object) if super::pattern::is_call(object) => found.push(value.clone()),
            Value::Object(object) => object.values().for_each(|child| walk(child, found)),
            Value::Array(items) => items.iter().for_each(|child| walk(child, found)),
            _ => {}
        }
    }
    walk(value, &mut found);
    found
}

/// [`apply`] with a test `plan` and `declared` page context.
fn hoisted(plan: &Plan, envelope: &Value, declared: &dyn Fn(&str) -> Option<PageContext>) -> Value {
    let mut envelope = envelope.clone();
    for row in envelope["results"].as_array_mut().into_iter().flatten() {
        if let Some(data) = row.get_mut("data") {
            super::omit::apply(&plan.omit, data);
            super::omit::through(&plan.refs, &plan.omit, data);
            super::holder::apply(&plan.holders, data, declared);
            super::outline::apply(&plan.outline, data);
        }
    }
    super::batch::apply(&plan.batch, &mut envelope);
    envelope
}

use crate::contracts::output_fields::PageContext;

/// Page context that lets every item omit any field and declares each on
/// every holder: the tests' plans decide what moves.
fn optional(_: &str) -> Option<PageContext> {
    Some(PageContext::Optional)
}

#[test]
fn every_tool_reads_its_plan_from_the_embedded_contract() {
    for tool in ToolId::ALL {
        let plan = plan::of(tool);
        for holder in &plan.holders {
            assert!(!holder.fields.is_empty(), "{tool}: an empty holder group");
        }
        for group in &plan.batch.keyed {
            assert!(group.by.starts_with("/data/"), "{tool}: {}", group.by);
        }
    }
    // Today's contract states identity once on GitHub listings and graph
    // traversals, and keys graph snapshot facts.
    assert!(!plan::of(ToolId::GhSearchCode).holders.is_empty());
    assert!(!plan::of(ToolId::GraphQuery).batch.keyed.is_empty());
    assert!(plan::of(ToolId::LocalFetch).holders.is_empty());
}

/// A unit fixture's own [`Keyed`] group, for the batch tests.
fn keyed_group(by: &str, fields: &[&str]) -> Keyed {
    Keyed {
        by: by.to_owned(),
        fields: fields.iter().map(|field| (*field).to_owned()).collect(),
    }
}

/// A batch that repeats a query states each repeated row exactly as it
/// states that row alone: rows equal apart from their own calls read the
/// first one's statement and keep their own `next`, and the envelope is,
/// as text, the one stating every row on its own.
#[test]
fn repeated_rows_are_stated_as_each_alone() {
    let tool = ToolId::AstSearch;
    let row = |path: &str, handle: &str| {
        json!({"index": 0, "data": {"files": [{"path": path, "symbols": [
            {"line": 3, "kind": "function", "symbolName": "run", "end": 9},
            {"line": 12, "kind": "struct", "symbolName": "Plan", "end": 20, "members": [
                {"line": 13, "kind": "field", "symbolName": "omit"}
            ]}
        ]}], "next": {"nextPage": {"kind": "continue", "tool": "astSearch", "input": {"cursor": handle}}}}})
    };
    let mut rows = vec![
        row("a.rs", "h0"),
        row("b.rs", "h1"),
        row("a.rs", "h2"),
        row("a.rs", "h3"),
    ];
    for (index, row) in rows.iter_mut().enumerate() {
        row["index"] = json!(index);
    }
    let mut hoisted = json!({ "results": rows.clone() });
    apply(tool, &mut hoisted);
    let mut alone = json!({ "results": rows });
    let tool_plan = super::plan::of(tool);
    assert!(!tool_plan.names_calls());
    for row in alone["results"].as_array_mut().unwrap() {
        state_row(tool, tool_plan, &mut row["data"]);
    }
    super::batch::apply(&tool_plan.batch, &mut alone);
    assert_eq!(hoisted.to_string(), alone.to_string());
    assert_eq!(
        hoisted["results"][2]["data"]["next"]["nextPage"]["input"]["cursor"],
        "h2"
    );
    assert_eq!(
        hoisted["results"][0]["data"]["files"],
        hoisted["results"][3]["data"]["files"]
    );
}

/// Rows are matched as executed, member order included, before any is
/// stated; calls differ only where the plan reads none.
#[test]
fn rows_are_compared_as_executed_text() {
    let rows = vec![
        json!({"data": {"n": 1, "next": {"a": 1}}}),
        json!({"data": {"n": 2}}),
        json!({"data": {"n": 1, "next": {"a": 2}}}),
        json!({"status": "error"}),
        json!({"data": {"n": 2}}),
        json!({"data": {"x": {"a": 1, "b": 2}}}),
        serde_json::from_str(r#"{"data": {"x": {"b": 2, "a": 1}}}"#).unwrap(),
        json!({"data": {"next": {"a": 1}, "n": 1}}),
    ];
    assert_eq!(
        same_data(&rows, true),
        vec![None, None, Some(0), None, Some(1), None, None, None]
    );
    assert_eq!(
        same_data(&rows, false),
        vec![None, None, None, None, Some(1), None, None, None]
    );
}
