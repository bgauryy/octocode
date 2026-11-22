//! A classified walk of a tool's rows pages on with the tool's private
//! progress beside its public query, which only an issued cursor carries.
use super::*;
use octocode_runtime_base::security::{ContentSecurity, InputPolicy};

/// The `listed` state of a ghSearchCode walk that listed `files` files: one
/// 16-hex digest each, as the walk writes it.
fn listed(files: usize) -> String {
    (0..files).map(|file| format!("{file:016x}")).collect()
}

/// A classified ghSearchCode row, as a caller sends it.
fn walk_row() -> Row {
    let rows = super::super::super::rows::parse_rows(&[json_fixture(
        r#"{
            "operation": "evaluate",
            "source": {
                "tool": "ghSearchCode",
                "query": {"keywords": ["useState"], "owner": "vercel", "repo": "next.js"}
            },
            "questions": {"hook": {"type": "noul", "instructions": "Calls a React hook."}}
        }"#,
    )]);
    rows.into_iter().next().expect("one row")
}

/// The tool's own next page past 625 listed files (a mocked page-1 lead).
fn page_two() -> Map<String, Value> {
    let query = json!({"keywords":["useState"],"owner":"vercel","repo":"next.js",
        "page":2,"listed":listed(626)});
    query.as_object().expect("a row").clone()
}

/// The restored row's gate: the one a cursor-restored clasify row passes.
fn restored_gate(row: Value) -> Result<(), String> {
    ContentSecurity::new()
        .check_saved_input(
            row,
            InputPolicy::ClasifyEvidence {
                content_limit: octocode_runtime_base::id::clasify_policy::REQUEST_BYTES,
            },
            ToolId::Clasify,
        )
        .map(|_| ())
        .map_err(|issues| format!("{issues:?}"))
}

/// Past 625 files the walk's `listed` outgrows any caller string. The next
/// page keeps it beside the public query, where the restored row's gate
/// leaves it as written, so the walk pages on; the row the cursor restores
/// is one the contract admits.
#[test]
fn a_classified_walk_past_625_files_pages_on() {
    let Some(NextPage::Call(call)) =
        next_page(&walk_row(), ToolId::GhSearchCode, &page_two(), true)
    else {
        panic!("a resumable runtime continues the walk");
    };
    let call = call.build();
    let row = call["input"]["queries"][0].clone();
    let query = &row["source"]["query"];
    assert!(
        query.get("listed").is_none() && query.get("page").is_none(),
        "{query}"
    );
    assert_eq!(row["sourceProgress"]["listed"], json!(listed(626)));
    assert_eq!(row["sourceProgress"]["page"], json!(2));
    restored_gate(row.clone()).expect("a restored walk row passes the gate");
    octocode_runtime_base::contracts::prepare_saved_rows(
        ToolId::Clasify.as_str(),
        json!({"queries": [row]}),
    )
    .expect("the contract admits the restored row");
}

/// A runtime that keeps no saved work cannot carry private progress in a
/// fresh call: the walk's next page is not offered.
#[test]
fn a_runtime_without_saved_work_offers_no_page_it_cannot_resume() {
    assert!(matches!(
        next_page(&walk_row(), ToolId::GhSearchCode, &page_two(), false),
        Some(NextPage::Unsaved)
    ));
}

/// A caller's `source.query` is the tool's public row: a private field in
/// it is named, and nothing is read.
#[test]
fn private_progress_in_a_callers_source_query_is_named() {
    let query = json!({"keywords":["x"],"owner":"o","repo":"r","page":2});
    assert_eq!(private_field(ToolId::GhSearchCode, &query), Some("page"));
    let public = json!({"keywords":["x"],"owner":"o","repo":"r"});
    assert_eq!(private_field(ToolId::GhSearchCode, &public), None);
}

/// A page of a tool's rows that has a next page says what the reply's `next`
/// costs: the next page is judged, so it bills as this page did. The count
/// of rows left is stated only from an exact total, offset and length.
#[test]
fn a_page_with_a_next_page_says_its_next_bills() {
    let counted = json!({"pagination": {"total": 179, "offset": 20, "length": 20}});
    let note = billed_next_note("artifactSearch", 1, 20, &counted, true);
    assert!(note.contains("bills again"), "{note}");
    assert!(note.contains("1 judge request for 20 rows"), "{note}");
    assert!(
        note.contains("139 of artifactSearch's rows remain"),
        "{note}"
    );
    assert!(note.starts_with("The reply's next "), "{note}");
    for uncounted in [
        json!({}),
        json!({"pagination": {"offset": 20}}),
        json!({"pagination": {"total": 179, "offset": 20, "length": 20, "countScope": "capped"}}),
        // A total below what was already read is no count of what is left.
        json!({"pagination": {"total": 30, "offset": 20, "length": 20}}),
    ] {
        let note = billed_next_note("ghSearchCode", 3, 12, &uncounted, false);
        assert!(note.contains("3 judge requests for 12 rows"), "{note}");
        assert!(
            note.contains("more of ghSearchCode's rows remain"),
            "{uncounted}: {note}"
        );
        // Without saved work the page's call is the row's related call.
        assert!(note.starts_with("The related[] call "), "{note}");
    }
}
