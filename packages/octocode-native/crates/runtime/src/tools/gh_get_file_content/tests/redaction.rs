//! Content scanning: page windows scan only themselves and keep redaction.
use super::*;

#[derive(Default)]
struct Counting {
    scans: std::sync::atomic::AtomicUsize,
}

impl ContentScan for Counting {
    fn sanitize(
        &self,
        text: &str,
        path: &Path,
    ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        self.scans.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Safe.sanitize(text, path)
    }
    fn redact_key_blocks(&self, content: &str) -> (String, bool) {
        (content.replace("KEYBODY", "[KEY]"), true)
    }
}

#[test]
fn line_pages_scan_only_their_window_and_keep_redaction() {
    let body = format!("TOKEN KEYBODY\n{}", "line of text\n".repeat(4000));
    let scanner = Counting::default();
    let memo = SanitizedViewMemo::new();
    let security = MemoizedScan::new(&scanner, &memo);
    let mut pages = Vec::new();
    for offset in [0, 100, 200] {
        let request = LocalFetchQuery {
            path: "big.txt".parse().expect("path"),
            unit: Some(WindowUnit::Lines),
            offset: Some(offset),
            length: wire_positive(100),
            ..test_local_fetch_query()
        };
        let page = process_fetched_content(
            &request,
            body.as_bytes(),
            Path::new("big.txt"),
            &Default::default(),
            &security,
            &NeverCancel,
        );
        pages.push(page.content.unwrap_or_default());
    }
    // Line pages sanitize only their window (plus a margin), one small
    // scan per page, instead of the whole file.
    assert_eq!(
        scanner.scans.load(std::sync::atomic::Ordering::SeqCst),
        3,
        "each line page scans its own window"
    );
    assert!(pages[0].starts_with("[REDACTED] [KEY]\n"), "{}", pages[0]);
    assert_eq!(pages[1], "line of text\n".repeat(100));
    // The memo still serves repeated full views (byte/fullContent pages):
    // a new view is scanned once, then reused.
    security
        .sanitize("x".repeat(20_000).as_str(), Path::new("other"))
        .expect("scan");
    security
        .sanitize("x".repeat(20_000).as_str(), Path::new("other"))
        .expect("scan");
    assert_eq!(scanner.scans.load(std::sync::atomic::Ordering::SeqCst), 4);
}

#[tokio::test]
async fn shared_pipeline_ranges_redacts_and_emits_remote_continuation() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(
        &server,
        "/api/v3/repos/a/b/commits/main",
        200,
        serde_json::json!({"sha":sha}),
    )
    .await;
    Mock::given(method("GET")).and(path("/api/v3/repos/a/b/contents/src%2Flib.rs")).and(query_param("ref",sha)).respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("one\nneedle TOKEN\nthree\n")}))).mount(&server).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let query: GhGetFileContentQuery = serde_json::from_value(serde_json::json!({
        "owner": "a", "repo": "b", "path": "src/lib.rs", "ref": "main",
        "unit": "lines", "length": 2, "mainGoal": "test", "reasoning": "test"
    }))
    .expect("ghGetFileContent query");
    let result = execute_default_regex(
        &provider,
        &query,
        &fixture_context(Duration::from_secs(2), 4096),
        None,
    )
    .await
    .expect("result");
    assert_eq!(
        result.file.content.content.as_deref(),
        Some("one\nneedle [REDACTED]\n")
    );
    assert_eq!(result.file.commit_sha, sha);
    let wire = serde_json::to_value(&result.file).expect("file json");
    assert_eq!(wire["commitSha"], sha);
    assert!(wire.get("resolvedRef").is_none(), "{wire}");
    let next = result.file.next.clone().expect("continuation");
    assert_eq!(next["continueWindow"]["tool"], "ghGetFileContent");
    assert_eq!(next["continueWindow"]["query"]["queries"][0]["owner"], "a");
    assert_eq!(next["continueWindow"]["query"]["queries"][0]["ref"], sha);
}

#[tokio::test]
async fn match_string_runs_on_redacted_text() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(
        &server,
        "/api/v3/repos/a/b/commits/main",
        200,
        serde_json::json!({"sha":sha}),
    )
    .await;
    mount_json(&server, "/api/v3/repos/a/b/contents/src%2Flib.rs", 200, serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("one\nkey TOKEN\nthree\n")})).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let query: GhGetFileContentQuery = serde_json::from_value(serde_json::json!({
        "owner": "a", "repo": "b", "path": "src/lib.rs", "ref": "main",
        "matchString": "TOKEN", "contextLines": 0, "mainGoal": "test", "reasoning": "test"
    }))
    .expect("query");
    let result = execute_default_regex(
        &provider,
        &query,
        &fixture_context(Duration::from_secs(2), 4096),
        None,
    )
    .await
    .expect("result");
    assert_eq!(
        result.file.match_not_found,
        Some(true),
        "{:?}",
        result.file.content
    );
}
