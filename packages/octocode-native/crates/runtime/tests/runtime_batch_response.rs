#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use serde_json::json;
use support::Workspace;

fn search(root: &str, extra: serde_json::Value) -> serde_json::Value {
    let mut query = json!({"path":root,"searchText":"needle","reasoning":"Find needles."});
    for (key, value) in extra.as_object().unwrap() {
        query[key] = value.clone();
    }
    query
}

#[tokio::test]
async fn invalid_rows_become_error_rows_while_valid_rows_execute() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", "needle\n");
    let root = file.parent().unwrap().to_string_lossy().into_owned();
    let runtime = workspace.runtime(&[]);
    let input = json!({"queries":[
        search(&root, json!({})),
        search(&root, json!({"serchText":"typo"})),
        search(&root, json!({"resultView":"files"}))
    ]});
    let outcome = runtime
        .execute("isolate".into(), "localSearch".into(), input)
        .await
        .expect("a partially invalid batch still executes");
    let rows = outcome.structured_content["results"].as_array().unwrap();
    assert_eq!(rows.len(), 3, "{}", outcome.structured_content);
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["index"], index);
    }
    assert_ne!(rows[0]["status"], "error");
    assert_eq!(rows[1]["status"], "error");
    assert_eq!(rows[1]["data"]["errorCode"], "invalidInput");
    let hints = rows[1]["data"]["hints"].to_string();
    assert!(
        hints.contains("query 2") || hints.contains("serchText"),
        "{hints}"
    );
    assert!(hints.contains("searchText"), "{hints}");
    assert_ne!(rows[2]["status"], "error");
    assert!(!outcome.all_failed);
    octocode_native::contracts::validate_output("localSearch", &outcome.structured_content)
        .expect("isolated batch output contract");
    runtime.close().await;
}

#[tokio::test]
async fn an_all_invalid_or_envelope_invalid_batch_still_fails_as_a_whole() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", "needle\n");
    let root = file.parent().unwrap().to_string_lossy().into_owned();
    let runtime = workspace.runtime(&[]);
    let all_invalid = json!({"queries":[
        search(&root, json!({"bogus":1})),
        search(&root, json!({"alsoBogus":2}))
    ]});
    let error = runtime
        .execute("all-invalid".into(), "localSearch".into(), all_invalid)
        .await
        .expect_err("no valid row to run");
    assert_eq!(error.code, "invalidInput");
    let too_many = json!({"queries": (0..6).map(|_| search(&root, json!({}))).collect::<Vec<_>>()});
    let error = runtime
        .execute("too-many".into(), "localSearch".into(), too_many)
        .await
        .expect_err("envelope limits are not row-scoped");
    assert_eq!(error.code, "invalidInput");
    runtime.close().await;
}

fn assert_whole_row_page(envelope: &serde_json::Value) {
    let rows = envelope["results"]
        .as_array()
        .expect("rows page keeps results");
    assert!(!rows.is_empty(), "{envelope}");
    assert!(envelope.get("responseWindow").is_none());
    for row in rows {
        assert!(row["index"].is_number() && row["data"].is_object(), "{row}");
    }
    octocode_native::contracts::validate_output("localSearch", envelope)
        .expect("row page output contract");
}

#[tokio::test]
async fn oversized_cli_output_pages_by_whole_rows_with_an_executable_continuation() {
    let workspace = Workspace::new();
    let mut file = None;
    for index in 0..20 {
        file = Some(workspace.write(&format!("src/f{index}.txt"), "needle ".repeat(400) + "\n"));
    }
    let root = file
        .unwrap()
        .parent()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let runtime = workspace.runtime(&[("OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH", "4000".into())]);
    let first = runtime
        .execute(
            "auto-page".into(),
            "localSearch".into(),
            search(&root, json!({"pageSize":20,"matchContentLength":2000})),
        )
        .await
        .unwrap();
    let envelope = &first.structured_content;
    assert_whole_row_page(envelope);
    let pagination = &envelope["responsePagination"];
    assert_eq!(pagination["scope"], "rows", "{pagination}");
    assert_eq!(pagination["hasMore"], true);
    let files_on_first = envelope["results"][0]["data"]["files"]
        .as_array()
        .unwrap()
        .len();
    assert!(
        (1..20).contains(&files_on_first),
        "one row split across pages"
    );
    assert_eq!(envelope["results"][0]["rowPart"]["part"], 1);
    let next = pagination["next"]["query"].clone();
    assert_eq!(next["responseScope"], "rows");
    assert_eq!(next["responseCharOffset"], 1);
    let mut files_seen = files_on_first;
    let mut next = Some(next);
    let mut pages = 1;
    while let Some(query) = next.take() {
        let page = runtime
            .execute(format!("auto-page-{pages}"), "localSearch".into(), query)
            .await
            .expect("continuation executes unchanged");
        assert_whole_row_page(&page.structured_content);
        files_seen += page.structured_content["results"][0]["data"]["files"]
            .as_array()
            .unwrap()
            .len();
        pages += 1;
        next = page.structured_content["responsePagination"]["next"]
            .get("query")
            .cloned();
        assert!(pages < 50, "paging terminates");
    }
    assert_eq!(
        files_seen, 20,
        "every file appears exactly once across pages"
    );
    runtime.close().await;
}

#[tokio::test]
async fn small_output_is_not_paginated_and_mcp_auto_pages_by_whole_rows() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", "needle\n");
    let root = file.parent().unwrap().to_string_lossy().into_owned();
    let runtime = workspace.runtime(&[("OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH", "2000".into())]);
    let small = runtime
        .execute(
            "small".into(),
            "localSearch".into(),
            search(&root, json!({})),
        )
        .await
        .unwrap();
    assert!(small.structured_content.get("responsePagination").is_none());
    assert!(
        small.structured_content["results"]
            .as_array()
            .unwrap()
            .len()
            == 1
    );

    for index in 0..20 {
        workspace.write(&format!("src/big{index}.txt"), "needle ".repeat(400) + "\n");
    }
    let mcp = runtime
        .execute_mcp(
            "mcp-auto".into(),
            "localSearch".into(),
            json!({"queries":[search(&root, json!({"pageSize":20,"matchContentLength":2000}))]}),
        )
        .await
        .unwrap();
    let text = mcp["content"][0]["text"].as_str().unwrap();
    assert!(text.encode_utf16().count() < 2600, "text page is bounded");
    let structured = &mcp["structuredContent"];
    // Implicit pages are whole rows: structuredContent-only clients see the
    // page instead of an emptied results array.
    assert!(!structured["results"].as_array().unwrap().is_empty());
    assert_eq!(structured["responsePagination"]["scope"], "rows");
    assert_eq!(structured["responsePagination"]["hasMore"], true);
    assert_eq!(
        structured["responsePagination"]["next"]["query"]["responseScope"],
        "rows"
    );
    runtime.close().await;
}
