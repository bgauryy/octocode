//! Row shape: receipts, block reads and zero-byte files.
use super::*;

/// Freshness is a receipt: a `receipts` read's first page (offset 0)
/// states when the file last changed and by whom, for one commits
/// request sent after the body arrived; a default read, a later page and
/// a failed read send none.
#[tokio::test]
async fn a_receipts_first_page_states_the_last_change_for_one_extra_request() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let body: String = (1..=40).map(|i| format!("line {i}\n")).collect();
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/a.txt",
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&body)}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/missing.txt",
        404,
        serde_json::json!({"message":"Not Found"}),
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(wiremock::matchers::body_string_contains("a.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!(
            {"data":{"repository":{"object":{"committedDate":"2026-01-03T00:00:00Z",
                "p0":{"nodes":[{"committedDate":"2026-01-02T00:00:00Z",
                    "author":{"name":"Ada","user":{"login":"ada-l"}}}]}}}}}
        )))
        .expect(1)
        .mount(&server)
        .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let plain = read(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"a.txt","ref":sha}),
    )
    .await;
    assert_eq!(plain.file.last_modified, None);
    assert_eq!(plain.file.last_modified_by, None);
    let base =
        serde_json::json!({"owner":"a","repo":"b","path":"a.txt","ref":sha,"extras":["receipts"]});
    let stamped = read(&provider, base.clone()).await;
    assert_eq!(
        stamped.file.last_modified.as_deref(),
        Some("2026-01-02T00:00:00Z")
    );
    assert_eq!(stamped.file.last_modified_by.as_deref(), Some("ada-l"));
    // A read that fails on the fetched body (a range past its end)
    // spends no receipts request (the mock expects exactly one).
    let mut failed = base.clone();
    failed["ranges"] = serde_json::json!(["100-120"]);
    let failed = read(&provider, failed).await;
    assert_eq!(
        failed.file.content.status,
        ReadStatus::Error,
        "{:?}",
        failed.file.content
    );
    assert_eq!(failed.file.last_modified, None);
    let mut later = base;
    later["offset"] = 20.into();
    let later = read(&provider, later).await;
    assert_eq!(later.file.last_modified, None);
    assert_eq!(later.file.last_modified_by, None);
    let mut missing = serde_json::json!({"owner":"a","repo":"b","path":"missing.txt","ref":sha});
    missing["mainGoal"] = "test".into();
    missing["reasoning"] = "test".into();
    let missing: GhGetFileContentQuery = serde_json::from_value(missing).expect("query");
    assert!(
        execute_default_regex(&provider, &missing, &default_context(), None,)
            .await
            .is_err()
    );
    let requests = server.received_requests().await.unwrap_or_default();
    assert!(
        !requests.iter().any(|request| {
            request.url.path() == "/api/v3/repos/a/b/commits"
                && request
                    .url
                    .query()
                    .is_some_and(|q| q.contains("missing.txt"))
        }),
        "a failed read asked for its timestamp: {requests:?}"
    );
}

/// The happy-path anchors of a block read: `path`, the resolved
/// `commitSha` (the next read's ref), gutter-numbered source lines, and
/// `blocks[]` as `{symbolName, line, endLine}`; owner, repo and ref are
/// not echoed.
#[tokio::test]
async fn a_block_read_row_carries_path_commit_gutter_and_blocks() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sha))
        .mount(&server)
        .await;
    let source = "use x;\n\nfn helper() -> u8 {\n    1\n}\n\nfn main() {\n    let v = helper();\n    println!(\"{v}\");\n}\n";
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src%2Fmain.rs",
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(source)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let read = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"src/main.rs","ref":"main",
            "matchString":"let v","block":true}),
    )
    .await
    .expect("read");
    let mut data = read.output.data.clone();
    assert_eq!(data["path"], "src/main.rs", "{data}");
    assert_eq!(data["commitSha"], sha, "{data}");
    assert_eq!(
        data["blocks"],
        serde_json::json!([{"symbolName":"main","line":7,"endLine":10}]),
        "{data}"
    );
    let mut staged = serde_json::json!({"results":[{"index":0,"data":data.take()}]});
    crate::response::numbered::number_read_rows(crate::id::ToolId::GhGetFileContent, &mut staged);
    let row = &staged["results"][0]["data"];
    assert!(
        row["content"]
            .as_str()
            .is_some_and(|text| text.starts_with("7\tfn main() {\n8\t")),
        "{row}"
    );
    for echo in ["ref", "matchString", "block"] {
        assert!(row.get(echo).is_none(), "{echo} echoed: {row}");
    }
}

/// A 0-byte file read whole is a successful read of all of it, not an
/// empty read that sends the caller to re-check the coordinates.
#[tokio::test]
async fn a_zero_byte_file_is_a_successful_read() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/py.typed".to_owned(),
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":"","size":0}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let read = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"py.typed","ref":sha}),
    )
    .await
    .unwrap_or_else(|failure| panic!("{}", failure.message));
    assert_eq!(read.output.status, None, "{}", read.output.data);
    assert_eq!(read.output.data["content"], "", "{}", read.output.data);
    assert_eq!(read.output.data["sourceBytes"], 0, "{}", read.output.data);
}
