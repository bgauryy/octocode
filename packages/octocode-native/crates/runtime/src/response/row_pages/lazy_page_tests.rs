//! Lazy row-page planning tests.

use super::*;
use crate::id::continuation_channels::{HINTS_KEY, PAGES_KEY};
mod delivery;
mod split_pages;
mod split_parts;

fn envelope() -> Map<String, Value> {
    envelope_of(400)
}

/// [`envelope`] with `count` matches in each broad file.
fn envelope_of(count: usize) -> Map<String, Value> {
    let matches = (0..count)
        .map(|line| json!({"line": line, "value": format!("fn item_{line}() {{ \u{1F600} }}")}))
        .collect::<Vec<_>>();
    let huge = json!({"line": 9999, "value": "x".repeat(900)});
    json!({"root": "/repo", "results": [
        {"index": 0, "data": {"files": [{"path": "a.rs", "matches": matches.clone()},
                                        {"path": "b.rs", "matches": [huge]}],
                              "next": {"sidecar": {"tool": "localSearch",
                                                      "query": {"page": 2}}}}},
        {"index": 1, "data": {"content": "small"}},
        {"index": 2, "data": {"files": [{"path": "c.rs", "matches": matches}]}}
    ]})
    .as_object()
    .cloned()
    .expect("object")
}

fn options(length: usize, offset: usize, snapshot: Option<String>) -> ResponsePageOptions {
    ResponsePageOptions {
        response_length: Some(length),
        response_offset: Some(offset),
        response_snapshot: snapshot,
        response_scope: Some(crate::response::cursor::PageScope::Rows),
        saved: None,
    }
}

/// Every page of `structured` at `length`, following the continuation.
fn walk(
    structured: &Map<String, Value>,
    length: usize,
) -> Vec<(Map<String, Value>, ResponsePagination)> {
    let full = Value::Object(structured.clone()).to_string();
    let (_, first) = paginate_rows(
        structured.clone(),
        &full,
        &options(length, 0, None),
        Vec::new(),
        &[],
        None,
    );
    (0..first.total_pages)
        .map(|offset| {
            let request = options(length, offset, Some(first.snapshot.clone()));
            let page = paginate_rows(structured.clone(), &full, &request, Vec::new(), &[], None);
            assert!(page.1.restart.is_none(), "length {length} page {offset}");
            page
        })
        .collect()
}

/// (row index, file path, line) of every delivered match.
fn delivered(pages: &[(Map<String, Value>, ResponsePagination)]) -> Vec<(u64, String, u64)> {
    let mut seen = Vec::new();
    for (page, _) in pages {
        for row in page["results"].as_array().expect("rows") {
            for file in row["data"]["files"].as_array().into_iter().flatten() {
                for item in file["matches"].as_array().into_iter().flatten() {
                    seen.push((
                        row["index"].as_u64().expect("index"),
                        file["path"].as_str().expect("path").to_owned(),
                        item["line"].as_u64().expect("line"),
                    ));
                }
            }
        }
    }
    seen.sort();
    seen
}

/// A cycle row as a graph tool lists it: `files` module paths, `runtime` runtime
/// cycles of `members` paths each, and its edge records.
fn cycle_row(cycle: usize, files: usize, runtime: usize, members: usize) -> Value {
    let edges = |kind: &str, count: usize| {
        (0..count)
            .map(|n| {
                json!({"from": format!("c{cycle}/{kind}/from_{n}.ts"),
                           "to": format!("c{cycle}/{kind}/to_{n}.ts"),
                           "edgeKinds": ["static-import"]})
            })
            .collect::<Vec<_>>()
    };
    json!({
        "runtimeCycleCount": runtime,
        "runtimeCycles": (0..runtime)
            .map(|r| (0..members)
                .map(|n| format!("packages/c{cycle}/runtime_{r}/member_{n:03}.ts"))
                .collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        "cycleEdges": edges("cycle", 4),
        "runtimeCycleEdges": edges("runtime", 7),
        "edgeKinds": ["static-import", "type-import"],
        "files": (0..files)
            .map(|n| format!("excalidraw-app/c{cycle}/components/Module_{n:04}.tsx"))
            .collect::<Vec<_>>(),
    })
}

/// Every part of row 0 across `pages`, in order: (rowPart, cycles).
fn row_parts(pages: &[(Map<String, Value>, ResponsePagination)]) -> Vec<(Value, Vec<Value>)> {
    pages
        .iter()
        .flat_map(|(page, _)| page["results"].as_array().cloned().unwrap_or_default())
        .filter(|row| row["index"] == 0)
        .map(|row| {
            let cycles = row["data"]["results"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            (row["rowPart"].clone(), cycles)
        })
        .collect()
}

/// Whether `cycle` holds evidence of cycle number `id`.
fn is_cycle(cycle: &Value, id: usize) -> bool {
    let text = cycle.to_string();
    text.contains(&format!("/c{id}/"))
}
