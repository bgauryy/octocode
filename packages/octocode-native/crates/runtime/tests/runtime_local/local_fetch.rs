//! localFetch through the runtime: pages, snapshots, contract checks, binary and redacted reads.

use crate::support::{self, Workspace, call, query_path, row_data};
use octocode_contract_types::ErrorCode;
use octocode_test_support::json_fixture;
use serde_json::json;

#[tokio::test]
async fn typed_ranges_preserve_requested_coordinates_and_unicode() {
    let workspace = Workspace::new();
    let path = workspace.write("source.txt", "one\ntwo 😀\nthree\nfour\nfive λ\n");
    let runtime = workspace.runtime(&[]);
    let read = call(
        &runtime,
        "localFetch",
        json!({"path":path,"ranges":[{"startLine":2,"endLine":2},{"startLine":4,"endLine":5}]}),
    )
    .await
    .expect("typed range read");
    let lines = row_data(&read)["content"]
        .as_str()
        .expect("source")
        .lines()
        .filter(|line| {
            line.split_once('\t')
                .is_some_and(|(number, _)| number.parse::<u32>().is_ok())
        })
        .collect::<Vec<_>>();
    assert_eq!(lines, ["2\ttwo 😀", "4\tfour", "5\tfive λ"]);
    runtime.close().await;
}

#[tokio::test]
async fn local_fetch_rejects_unknown_fields_at_the_contract() {
    let workspace = Workspace::new();
    let path = workspace.write("a.txt", "ok\n");
    let runtime = workspace.runtime(&[]);
    let error = call(
        &runtime,
        "localFetch",
        json!({"path": path, "madeUp": true}),
    )
    .await
    .expect_err("unknown field");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    runtime.close().await;
}

#[tokio::test]
async fn local_fetch_binary_hint_does_not_blame_an_absent_match_string() {
    let workspace = Workspace::new();
    let path = workspace.write("blob.ts", b"\x00\x01\x02binary\x00".as_slice());
    let runtime = workspace.runtime(&[]);
    let outcome = call(&runtime, "localFetch", query_path(&path, json!({})))
        .await
        .expect("typed binary row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "binaryFileUnsupported", "{data}");
    let hints = outcome.structured_content.to_string();
    assert!(!hints.contains("remove matchString"), "{hints}");
    runtime.close().await;
}

#[tokio::test]
async fn local_fetch_redacted_content_is_marked_not_verbatim() {
    let workspace = Workspace::new();
    let path = workspace.write(
        "fixture.ts",
        "export const clean = 1;\nconst url = \"https://admin:hunter2secret@db.example.com:5432/app\";\n",
    );
    let clean = workspace.write("clean.ts", "export const clean = 1;\n");
    let runtime = workspace.runtime(&[]);
    for extra in [
        json!({}),
        json_fixture(r#"{"ranges": [{"startLine": 2, "endLine": 2}]}"#),
    ] {
        let outcome = call(&runtime, "localFetch", query_path(&path, extra.clone()))
            .await
            .expect("redacted read");
        let data = row_data(&outcome);
        assert!(
            data["content"]
                .as_str()
                .unwrap_or_default()
                .contains("[REDACTED"),
            "{extra}: {data}"
        );
        assert_eq!(data["redacted"], json!(true), "{extra}: {data}");
        assert!(data.get("warnings").is_none(), "{extra}: {data}");
    }
    let outcome = call(&runtime, "localFetch", query_path(&clean, json!({})))
        .await
        .expect("clean read");
    support::checks::assert_absent(row_data(&outcome), "redacted");
    runtime.close().await;
}

/// A read of a glob path names no file: its recovery lists the directory and
/// never searches for files literally named `*.<ext>`.
#[tokio::test]
async fn a_glob_path_read_lists_the_directory_and_no_literal_wildcard_name() {
    let workspace = Workspace::new();
    workspace.write("w/a.md", "a\n");
    let runtime = workspace.runtime(&[]);
    let outcome = call(
        &runtime,
        "localFetch",
        query_path(&workspace.workspace.join("w/*.md"), json!({})),
    )
    .await
    .expect("typed missing row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "notFound", "{data}");
    let text = outcome.structured_content.to_string();
    assert!(
        text.contains("\"tree\""),
        "the directory listing stays: {text}"
    );
    assert!(!text.contains("[*]"), "no literal wildcard search: {text}");
    runtime.close().await;
}

/// A missing path carries its parent listing in related[] and the one
/// `notFound` hint beside it: the lead is the call, the hint says to retry
/// with the exact listed path.
#[tokio::test]
async fn a_missing_path_carries_its_listing_and_the_not_found_hint() {
    let workspace = Workspace::new();
    workspace.write("w/a.md", "a\n");
    let runtime = workspace.runtime(&[]);
    let outcome = call(
        &runtime,
        "localFetch",
        query_path(&workspace.workspace.join("w/missing.md"), json!({})),
    )
    .await
    .expect("typed missing row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "notFound", "{data}");
    assert!(
        data["related"].as_array().is_some_and(|r| !r.is_empty()),
        "{data}"
    );
    assert!(
        data["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("retry with the exact listed path")),
        "{data}"
    );
    runtime.close().await;
}

/// A long line's byte pages fit the row's share of the response page: all
/// of it for one row, an equal share for each row of a batch
/// (`read_share::row_window`), less the skeleton every page budget keeps. A
/// share that holds 4 KB pages reads them; a narrower one reads smaller
/// pages, never slivers under one hit's window, and no share grows past
/// the page.
#[tokio::test]
async fn a_long_lines_byte_pages_fit_each_rows_share_and_never_shrink_to_slivers() {
    use octocode_runtime_base::{id::ToolId, response::read_share::row_window};
    use octocode_tools_shared::source_view::pagination::{
        MIN_BYTE_PAGE, MIN_LONG_LINE_PAGE, byte_page_room,
    };
    let workspace = Workspace::new();
    let line = (0..2_500).map(|at| format!("{at:07};")).collect::<String>();
    let path = workspace.write("bundle.min.js", format!("{line}\n"));
    let runtime = workspace.runtime(&[]);
    let row = json!({"path": path});
    let page_text = |data: &serde_json::Value| {
        assert_eq!(data["pagination"]["unit"], "bytes", "{data}");
        data["content"].as_str().expect("content").len()
    };
    // (rows, responseChars): one row of a narrow page, a batch's narrow
    // shares of it, and a batch whose shares of the default page hold 4 KB.
    for (rows, page) in [(1, 400), (3, 400), (3, 20_000)] {
        let room = byte_page_room(Some(row_window(ToolId::LocalFetch, page, rows)));
        assert!(
            room >= MIN_BYTE_PAGE || page == 400,
            "{rows} rows of {page}: a share of this page holds 4 KB pages"
        );
        let output = call(
            &runtime,
            "localFetch",
            json!({"queries": vec![row.clone(); rows], "responseChars": page}),
        )
        .await
        .expect("a read");
        let results = output
            .structured_content
            .pointer("/results")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(!results.is_empty(), "{:?}", output.structured_content);
        for result in &results {
            let shown = page_text(&result["data"]);
            assert!(
                (MIN_LONG_LINE_PAGE..=room).contains(&shown),
                "{rows} rows of {page}: {shown} bytes, room {room}"
            );
            if page == 400 {
                assert!(
                    shown < MIN_BYTE_PAGE,
                    "a narrow share stays narrow: {shown}"
                );
            }
        }
    }
    runtime.close().await;
}
