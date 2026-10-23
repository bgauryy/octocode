//! localFetch through the runtime: pages, snapshots, contract checks, binary and redacted reads.

use crate::support::{self, Run as _, Workspace, call, query_path, row_data, row_status};
use octocode_contract_types::ErrorCode;
use serde_json::json;

#[tokio::test]
async fn local_fetch_pages_and_unions_through_the_runtime() {
    let workspace = Workspace::new();
    let path = workspace.write("source.txt", "one\ntwo 😀\nthree\n");
    let runtime = workspace.runtime(&[]);
    let first = call(
        &runtime,
        "localFetch",
        query_path(&path, json!({"unit":"lines","length":1})),
    )
    .await
    .expect("first page");
    assert_eq!(row_status(&first), "success");
    let data = row_data(&first);
    assert_eq!(data["content"].as_str(), Some("1\tone\n"));
    let next = data["next"]["continueWindow"]["query"].clone();
    assert!(next.is_object(), "executable continuation");

    let second = call(&runtime, "localFetch", next)
        .await
        .expect("second page");
    assert_eq!(row_data(&second)["content"].as_str(), Some("2\ttwo 😀\n"));
    runtime.close().await;
}

/// A byte page states its view's total (`pagination.total`), as a
/// line page's total rides `totalLines`: `sourceBytes` is a stats extra, so
/// without it the page would name no total at all.
#[tokio::test]
async fn local_fetch_byte_pages_state_their_total() {
    let workspace = Workspace::new();
    let text = "one\ntwo 😀\nthree\n";
    let path = workspace.write("bytes.txt", text);
    let runtime = workspace.runtime(&[]);
    let page = call(
        &runtime,
        "localFetch",
        query_path(&path, json!({"unit":"bytes","length":5,"extras":[]})),
    )
    .await
    .expect("byte page");
    let data = row_data(&page);
    assert!(data.get("sourceBytes").is_none(), "{data}");
    assert_eq!(data["pagination"]["total"], text.len(), "{data}");
    assert_eq!(data["pagination"]["hasMore"], true, "{data}");
    runtime.close().await;
}

#[tokio::test]
async fn mcp_local_fetch_snapshots_stale_only_the_mutated_batch_row() {
    let workspace = Workspace::new();
    let first_path = workspace.write("snapshot-first.txt", "first-1\nfirst-2\n");
    let second_path = workspace.write("snapshot-second.txt", "second-1\nsecond-2\n");
    let runtime = workspace.runtime(&[]);
    let initial = runtime
        .run_mcp(
            "mcp-local-snapshot-batch".into(),
            "localFetch".into(),
            json!({"queries":[
                {
                    "path":first_path,
                    "unit":"lines",
                    "length":1,
                    "mainGoal": "test", "reasoning":"Page the first snapshot fixture."
                },
                {
                    "path":second_path,
                    "unit":"lines",
                    "length":1,
                    "mainGoal": "test", "reasoning":"Page the second snapshot fixture."
                }
            ]}),
        )
        .await
        .expect("MCP localFetch batch");
    let structured = &initial["structuredContent"];
    support::checks::assert_absent(structured, "cursor");
    let rows = structured["results"].as_array().expect("MCP result rows");
    let continuation =
        |row: usize| rows[row]["data"]["next"]["continueWindow"]["query"]["queries"][0].clone();
    let (first_next, second_next) = (continuation(0), continuation(1));
    assert_eq!(
        first_next["snapshot"].as_str().map(str::len),
        Some(64),
        "{first_next}"
    );

    std::fs::write(&first_path, "changed-1\nchanged-2\n").expect("mutate first source");
    let resumed = runtime
        .run(
            "resume-local-snapshot-batch".into(),
            "localFetch".into(),
            json!({"queries":[first_next, second_next]}),
        )
        .await
        .expect("typed batch rows");
    let rows = resumed.structured_content["results"]
        .as_array()
        .expect("resumed rows");
    assert_eq!(rows[0]["status"], "error", "{}", rows[0]);
    assert_eq!(rows[0]["data"]["errorCode"], "staleSnapshot", "{}", rows[0]);
    let restart = &rows[0]["data"]["next"]["restart"]["query"]["queries"][0];
    assert_eq!(restart["path"], first_next["path"], "{}", rows[0]);
    assert!(restart.get("snapshot").is_none(), "{restart}");
    assert!(restart.get("offset").is_none(), "{restart}");
    assert_eq!(rows[1]["data"]["content"], "2\tsecond-2\n", "{}", rows[1]);
    runtime.close().await;
}

/// A streamed (over 10 MiB) read of several ranges whose snapshot no longer
/// names the source restarts the whole read: every requested range, without
/// the snapshot, from the runtime's one restart owner.
#[tokio::test]
async fn a_stale_streamed_range_read_restarts_every_range() {
    let workspace = Workspace::new();
    let line = "streamed payload payload payload payload\n";
    let path = workspace.write("big.log", line.repeat(11 * 1024 * 1024 / line.len()));
    let runtime = workspace.runtime(&[]);
    let stale = call(
        &runtime,
        "localFetch",
        query_path(
            &path,
            json!({"ranges": ["1-2", "5-6"], "snapshot": "0".repeat(64)}),
        ),
    )
    .await
    .expect("typed stale row");
    let data = row_data(&stale);
    assert_eq!(data["errorCode"], "staleSnapshot", "{data}");
    let restart = &data["next"]["restart"]["query"]["queries"][0];
    assert_eq!(restart["ranges"], json!(["1-2", "5-6"]), "{data}");
    assert!(restart.get("snapshot").is_none(), "{data}");
    let again = call(
        &runtime,
        "localFetch",
        data["next"]["restart"]["query"].clone(),
    )
    .await
    .expect("restart");
    let again = row_data(&again);
    let numbered = again["content"]
        .as_str()
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once('\t')?.0.parse::<u32>().ok())
        .collect::<Vec<_>>();
    assert_eq!(numbered, [1, 2, 5, 6], "{again}");
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
    for extra in [json!({}), json!({"unit":"lines","length":5})] {
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
        let warning = data["warnings"]
            .as_array()
            .and_then(|warnings| {
                warnings
                    .iter()
                    .filter_map(|w| w.as_str())
                    .find(|w| w.starts_with("redactedContent"))
            })
            .unwrap_or_else(|| panic!("{extra}: missing redaction warning: {data}"));
        assert!(warning.contains("not verbatim"), "{warning}");
    }
    let outcome = call(&runtime, "localFetch", query_path(&clean, json!({})))
        .await
        .expect("clean read");
    support::checks::assert_absent(row_data(&outcome), "redactedContent");
    runtime.close().await;
}
