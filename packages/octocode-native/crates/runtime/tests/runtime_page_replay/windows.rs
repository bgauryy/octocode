//! Explicit `responseLength` windows: automatic pages, row-sized source pages and row-paged searches.

use super::{SHA, lines};
use crate::support::{self, Run as _, Workspace};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// An offset or snapshot without `responseLength` is no explicit window: the
/// response keeps the automatic page and read share, exactly as the same call
/// without it, and is never served unpaged past the page.
#[tokio::test]
async fn an_offset_or_snapshot_alone_keeps_the_automatic_page() {
    let workspace = Workspace::new();
    let rows: Vec<_> = (0..5)
        .map(|index| {
            let file = workspace.write(&format!("src/f{index}.txt"), lines(&"w".repeat(80)));
            json!({"path": file.to_string_lossy(), "fullContent": true,
                "mainGoal": "Read the files.", "reasoning": "Exercise automatic paging."})
        })
        .collect();
    let runtime = workspace.runtime(&[]);
    let call = |id: &str, extra: serde_json::Value| {
        let mut input = json!({"queries": rows});
        input
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        runtime.run(id.into(), "localFetch".into(), input)
    };
    let default = call("auto", json!({})).await.expect("default");
    let size = |outcome: &octocode_native::response::outcome::ToolOutcome| {
        outcome.structured_content.to_string().len()
    };
    for (id, extra) in [("offset", json!({"responseOffset": 0}))] {
        let outcome = call(id, extra).await.expect(id);
        assert!(
            size(&outcome) <= size(&default) + 64,
            "{id}: {} vs automatic {}",
            size(&outcome),
            size(&default)
        );
    }
    assert!(
        size(&default) < 40_000,
        "automatic page: {}",
        size(&default)
    );
    runtime.close().await;
}

/// A localFetch row fills its share of the call's response page and pages by
/// its own continuation: an explicit `responseLength` returns the row itself,
/// never an emptied `results` with a re-cut text window.
#[tokio::test]
async fn local_fetch_rows_page_by_the_calls_response_window() {
    let workspace = Workspace::new();
    let wide: String = (0..2_000)
        .map(|line| format!("line {line:05} of wide text\n"))
        .collect();
    let file = workspace.write("src/wide.txt", &wide);
    let runtime = workspace.runtime(&[]);
    let outcome = runtime
        .run(
            "fetch-window".into(),
            "localFetch".into(),
            json!({
                "queries":[{"path":file.to_string_lossy(),"fullContent":true,
                    "mainGoal":"Read the file.","reasoning":"Exercise row windows."}],
                "responseLength": 5000
            }),
        )
        .await
        .expect("page");
    let page = &outcome.structured_content;
    assert!(page.get("responseWindow").is_none(), "{page}");
    let row = &page["results"][0]["data"];
    let content = row["content"].as_str().expect("row content");
    assert!(
        !content.is_empty() && content.len() <= 5_000,
        "{}",
        content.len()
    );
    assert!(
        row["next"].is_object(),
        "the row continues by its own offset: {row}"
    );
    runtime.close().await;
}

/// A 5,000-line file of short lines: its line-number gutters outweigh its
/// text, so a page sized without them overflows.
fn short_lines() -> String {
    (0..5_000).map(|line| format!("l{line}\n")).collect()
}

/// One staged MCP response to a whole-file read under an explicit
/// `responseLength`: both channels, measured as sent (numbered content, so
/// gutters included), fit the page; the row is returned whole with its own
/// `next.continueWindow`, never an emptied `results` beside a text
/// `responseWindow`.
fn assert_sized_source_page(result: &Value, length: usize) {
    let page = &result["structuredContent"];
    assert!(page.get("responseWindow").is_none(), "{length}: {page}");
    assert!(page.get("responsePagination").is_none(), "{length}: {page}");
    let row = &page["results"][0]["data"];
    assert!(
        row["content"]
            .as_str()
            .is_some_and(|content| !content.is_empty()),
        "{length}: {row}"
    );
    assert!(row["next"]["continueWindow"].is_object(), "{length}: {row}");
    let structured = page.to_string().encode_utf16().count();
    let text: usize = result["content"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|item| item["text"].as_str().unwrap_or("").encode_utf16().count())
        .sum();
    assert!(structured <= length, "{length}: structured {structured}");
    assert!(text <= length, "{length}: text {text}");
}

const EXPLICIT_LENGTHS: [usize; 4] = [5_000, 30_000, 40_000, 50_000];

#[tokio::test]
async fn explicit_lengths_return_local_source_rows_sized_to_the_page() {
    let workspace = Workspace::new();
    let file = workspace.write("src/short.txt", short_lines());
    let runtime = workspace.runtime(&[]);
    for length in EXPLICIT_LENGTHS {
        let result = runtime
            .run_mcp(
                format!("fetch-{length}"),
                "localFetch".into(),
                json!({"queries":[{"path":file.to_string_lossy(),"fullContent":true}],
                    "responseLength": length}),
            )
            .await
            .expect("page");
        assert_sized_source_page(&result, length);
    }
    runtime.close().await;
}

#[tokio::test]
async fn explicit_lengths_return_github_source_rows_sized_to_the_page() {
    let server = MockServer::builder().start().await;
    let body = short_lines();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Fshort.rs"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file",
            "encoding": "base64",
            "content": STANDARD.encode(&body),
            "size": body.len(),
            "sha": "f".repeat(40),
            "path": "src/short.rs"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for length in EXPLICIT_LENGTHS {
        let result = runtime
            .run_mcp(
                format!("gh-{length}"),
                "ghGetFileContent".into(),
                json!({"queries":[{"owner":"a","repo":"b","path":"src/short.rs","ref":SHA,
                    "fullContent":true}],
                    "responseLength": length}),
            )
            .await
            .expect("page");
        assert_sized_source_page(&result, length);
    }
    runtime.close().await;
}

/// A wide search under an explicit `responseLength` pages whole rows: the
/// page carries its rows and a `responseScope:"rows"` continuation, never a
/// text `responseWindow`.
#[tokio::test]
async fn an_explicit_length_pages_a_wide_search_by_rows() {
    let workspace = Workspace::new();
    for index in 0..40 {
        workspace.write(&format!("src/f{index:02}.txt"), lines(&"w".repeat(60)));
    }
    let root = workspace
        .workspace
        .join("src")
        .to_string_lossy()
        .into_owned();
    let runtime = workspace.runtime(&[]);
    let outcome = runtime
        .run(
            "wide".into(),
            "localSearch".into(),
            json!({"queries":[{"path":root,"matchString":"needle","pageSize":40}],
                "responseLength": 30_000}),
        )
        .await
        .expect("page");
    let page = &outcome.structured_content;
    assert!(page.get("responseWindow").is_none(), "{page}");
    assert!(
        page["results"][0]["data"].is_object(),
        "the page carries its rows: {page}"
    );
    let pagination = &page["responsePagination"];
    assert_eq!(pagination["scope"], "rows", "{pagination}");
    support::assert_cursor_only(&pagination["next"]["query"]);
    runtime.close().await;
}
