//! A row split into `rowPart` parts delivers each evidence value once: a
//! later part keeps required free text only as `""` (so it validates), the
//! names that place the continued element whole, and the parts join back to
//! the unsplit row. The oracle is the unsplit row and the output schema, not
//! the pager's own page-context rule.
use super::nested_continuation::{Join, continued_elements, declared_collections, merge};
use super::part_metadata::parts;
use super::*;
use crate::id::ToolId;
use crate::tools::test_support::assert_row_contract;

/// Every scalar leaf of `value` with its pointer, in document order.
pub(super) fn leaves(value: &Value, at: &str, out: &mut Vec<(String, Value)>) {
    match value {
        Value::Object(members) => {
            for (name, member) in members {
                let pointer = format!("{at}/{}", crate::json_pointer::escape(name));
                leaves(member, &pointer, out);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                leaves(item, &format!("{at}/{index}"), out);
            }
        }
        scalar => out.push((at.to_owned(), scalar.clone())),
    }
}

/// The parts of `data` at `budget`, each checked against the output schema,
/// joined as the contract says.
pub(super) fn walk_and_join(tool: ToolId, data: &Value, budget: usize) -> (Vec<Value>, Value) {
    let collections = declared_collections(tool, data);
    let parts = parts(tool, data, budget);
    let mut joined: Option<Value> = None;
    for part in &parts {
        assert_row_contract(tool, &part["data"]);
        let continued = continued_elements(&part["rowPart"]["continues"]);
        let join = Join {
            continued: &continued,
            text: part["rowPart"]["text"].as_str(),
            collections: &collections,
        };
        match joined.as_mut() {
            None => joined = Some(part["data"].clone()),
            Some(joined) => merge(joined, part["data"].clone(), "/data", &join),
        }
    }
    (parts, joined.expect("a walk has a first part"))
}

/// The joined row equals the unsplit one value by value, then whole.
pub(super) fn assert_joins_to(joined: &Value, data: &Value, label: &str) {
    let mut expected = Vec::new();
    leaves(data, "", &mut expected);
    for (pointer, value) in expected {
        assert_eq!(joined.pointer(&pointer), Some(&value), "{label}: {pointer}");
    }
    assert_eq!(joined, data, "{label}: parts join to the row");
}

fn pull_request() -> Value {
    let comments = (0..24)
        .map(|n| {
            json!({"author": format!("reviewer-{n}"),
                "body": format!("comment {n}: {}", "looks right to me ".repeat(6))})
        })
        .collect::<Vec<_>>();
    json!({"pullRequests":[{"number":42,
        "title":"Rework the pager so required evidence arrives once",
        "state":"open","author":"octocat-author","createdAt":"2026-11-07T10:00:00Z",
        "comments":comments}]})
}

/// A pull request whose comments split across parts: its required free
/// text (`title`, `author`, `createdAt`) arrives on the first part and as
/// `""` on every later one; its `state` (an enum, no empty form) repeats
/// whole and agrees; every comment arrives once.
#[test]
fn a_split_row_delivers_its_required_free_text_once() {
    let tool = ToolId::GhGetHistoryItem;
    let data = pull_request();
    assert_row_contract(tool, &data);
    let mut strings = Vec::new();
    leaves(&data, "", &mut strings);
    let mut split = false;
    for budget in (400..=1_600).step_by(100) {
        let (parts, joined) = walk_and_join(tool, &data, budget);
        if parts.len() < 2 {
            continue;
        }
        split = true;
        for (at, part) in parts.iter().enumerate() {
            assert!(
                part["rowPart"].is_object(),
                "budget {budget}: part {at} is a rowPart"
            );
            let pr = &part["data"]["pullRequests"][0];
            assert_eq!(pr["number"], 42, "budget {budget}: part {at} names the row");
            assert_eq!(
                pr["state"], "open",
                "budget {budget}: part {at} repeats the enum"
            );
            if at > 0 {
                for field in ["title", "author", "createdAt"] {
                    assert_eq!(pr[field], "", "budget {budget}: part {at} {field}");
                }
            }
        }
        assert_joins_to(&joined, &data, &format!("budget {budget}"));
        for (pointer, value) in &strings {
            if pointer.ends_with("/state") || !value.is_string() {
                continue;
            }
            let carried = parts
                .iter()
                .filter(|part| {
                    let mut found = Vec::new();
                    leaves(&part["data"], "", &mut found);
                    found.iter().any(|(_, leaf)| leaf == value)
                })
                .count();
            assert_eq!(carried, 1, "budget {budget}: {pointer} arrives once");
        }
    }
    assert!(split, "some budget splits the pull request");
}

/// An lspSearch call row whose call sites split across parts names itself
/// on every part: its `symbolName` places the continued row, as a symbol's
/// does, so it repeats whole rather than arriving as `""`.
#[test]
fn a_continued_call_row_keeps_its_name() {
    let tool = ToolId::LspSearch;
    let sites = (1..=60)
        .map(|line| json!({"line": line * 3, "column": 9}))
        .collect::<Vec<_>>();
    let data = json!({"operation":"callers","files":[{"path":"src/pager.rs","matches":[
        {"symbolName":"plan_rows_for_the_response","kind":"function","line":12,"sites":sites}]}]});
    assert_row_contract(tool, &data);
    let mut continued = false;
    for budget in (200..=900).step_by(50) {
        let (parts, joined) = walk_and_join(tool, &data, budget);
        for part in parts.iter().skip(1) {
            let call = &part["data"]["files"][0]["matches"][0];
            continued |= call["sites"]
                .as_array()
                .is_some_and(|sites| !sites.is_empty());
            assert_eq!(
                call["symbolName"], "plan_rows_for_the_response",
                "budget {budget}"
            );
        }
        assert_joins_to(&joined, &data, &format!("budget {budget}"));
    }
    assert!(continued, "some budget continues the call row's sites");
}
