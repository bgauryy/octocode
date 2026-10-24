//! localFetch through the runtime: pages, snapshots, contract checks, binary and redacted reads.

use crate::support::{self, Workspace, call, query_path, row_data};
use octocode_contract_types::ErrorCode;
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
    for extra in [json!({}), json!({"ranges":[{"startLine":2,"endLine":2}]})] {
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
