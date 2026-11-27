//! Row shape: receipts, block reads and zero-byte files.
use super::*;
use octocode_test_support::json_fixture;

/// A read asks GitHub for the file alone, and leads nowhere for when it
/// last changed: that is the file's history, one documented call away
/// (ghSearchHistory), on a first page, a later page and a failed read alike.
#[tokio::test]
async fn a_read_asks_only_the_file_and_offers_no_history_lead() {
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
    let provider = mock_provider(&server, RetryPolicy::default());
    let base = json!({"owner":"a","repo":"b","path":"a.txt","ref":sha});
    let history_leads = |row: &GhGetFileContentResult| {
        row.file
            .calls
            .as_ref()
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|calls| calls.values())
            .filter(|call| call["tool"] == "ghSearchHistory")
            .count()
    };
    let first = read(&provider, base.clone()).await;
    assert_eq!(history_leads(&first), 0, "{:?}", first.file.calls);
    let mut failed = base.clone();
    failed["ranges"] = json_fixture(r#"[{"startLine": 100, "endLine": 120}]"#);
    let failed = read(&provider, failed).await;
    assert_eq!(failed.file.content.status, ReadStatus::Error);
    assert_eq!(history_leads(&failed), 0, "{:?}", failed.file.calls);
    let mut later = base;
    later["readProgress"] = json_fixture(r#"{"unit": "lines", "offset": 20}"#);
    assert_eq!(history_leads(&read(&provider, later).await), 0);
    let requests = server.received_requests().await.unwrap_or_default();
    assert!(
        requests.iter().all(|request| {
            let route = request.url.path();
            route == "/api/v3/repos/a/b/contents/a.txt"
                || route.starts_with("/api/v3/repos/a/b/git/trees/")
        }),
        "a read asked beyond its file and the tree that says it is no link: {requests:?}"
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
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "path": "src/main.rs",
                "ref": "main",
                "matchString": ["let v"],
                "block": true
            }"#,
        ),
    )
    .await
    .expect("read");
    let mut data = read.data.clone();
    assert_eq!(data["path"], "src/main.rs", "{data}");
    assert_eq!(data["commitSha"], sha, "{data}");
    assert_eq!(
        data["blocks"],
        json_fixture(r#"[{"symbolName": "main", "line": 7, "endLine": 10}]"#),
        "{data}"
    );
    let mut staged = serde_json::json!({"results":[{"index":0,"data":data.take()}]});
    octocode_runtime_base::response::numbered::number_read_rows(
        octocode_runtime_base::id::ToolId::GhGetFileContent,
        &mut staged,
    );
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
        json_fixture(
            r#"{
                "type": "file",
                "encoding": "base64",
                "content": "",
                "size": 0
            }"#,
        ),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let read = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"py.typed","ref":sha}),
    )
    .await
    .unwrap_or_else(|failure| panic!("{}", failure.message));
    assert_eq!(read.status, None, "{}", read.data);
    assert_eq!(read.data["content"], "", "{}", read.data);
}
