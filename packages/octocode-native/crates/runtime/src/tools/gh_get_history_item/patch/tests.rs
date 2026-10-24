use super::super::HistoryItemRequest;
use super::super::filter::*;
use super::super::inventory::*;
use super::super::promotion::promote_pr_continuations;
use super::super::remaining::Remainder;
use super::super::util::str_at;
use super::super::window::*;
use super::hunks::*;
use super::locate::*;
use super::page::*;
use octocode_github::MAX_PER_PAGE;
use serde_json::{Map, Value, json};
use std::collections::HashMap;

mod delivery;
mod inventory_rows;
mod matching;
mod packing;
mod pagination;
mod summary;

fn patch_query(offset: usize) -> HistoryItemRequest {
    with_budget(patch_request(json!({"offset":offset})), 5)
}

fn patch_request(fields: serde_json::Value) -> HistoryItemRequest {
    let base = json!({
        "operation":"pullRequest",

        "owner":"a",
        "repo":"b",
        "number":1
    });
    HistoryItemRequest::from_row(super::super::util::merge(base, fields))
        .expect("patch query fixture should be valid")
}

fn file(name: &str, patch: &str) -> Value {
    json!({"filename":name,"status":"modified","patch":patch})
}

fn window(fields: Value) -> HistoryItemRequest {
    let base = json!({
        "operation":"commit",
        "owner":"a","repo":"b","ref":"abc","sections":["patches"]
    });
    HistoryItemRequest::from_row(super::super::util::merge(base, fields))
        .expect("commit window fixture should be valid")
}

/// Follow a commit page's `nextOffset` cursor to the end and return
/// (calls, per-file reassembled patches, files reported patchless).
fn follow_commit_page(
    files: &[Value],
    page_budget: Option<usize>,
) -> (usize, HashMap<String, String>, Vec<String>) {
    let mut offset = 0usize;
    let mut calls = 0;
    let mut patches = HashMap::<String, String>::new();
    let mut patchless = Vec::new();
    loop {
        calls += 1;
        assert!(calls < 1_000, "cursor did not advance");
        let mut query = window(json!({"offset":offset}));
        query.auto_page_chars = page_budget;
        let page = shape_patch_page(files.to_vec(), &query);
        for row in &page.rows {
            let name = str_at(row, "/filename").unwrap_or("").to_owned();
            if row.get("patchUnavailable").is_some() {
                patchless.push(name.clone());
            }
            if let Some(text) = row.get("patch").and_then(Value::as_str) {
                patches.entry(name).or_default().push_str(text);
            }
        }
        let Some(next) = page.cursor else {
            return (calls, patches, patchless);
        };
        assert!(next > offset, "cursor must advance");
        offset = next;
    }
}

fn with_budget(mut query: HistoryItemRequest, page_chars: usize) -> HistoryItemRequest {
    query.auto_page_chars = Some(page_chars);
    query
}
