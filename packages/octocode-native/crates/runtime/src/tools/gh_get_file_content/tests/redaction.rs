//! Content scanning: page windows scan only themselves and keep redaction.
use super::*;
use octocode_contract_types::LocalFetchQuery;

#[derive(Default)]
struct Counting {
    scans: std::sync::atomic::AtomicUsize,
    sizes: std::sync::Mutex<Vec<usize>>,
}

impl ContentScan for Counting {
    fn sanitize(
        &self,
        text: &str,
        path: &Path,
    ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        self.scans.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.sizes.lock().expect("sizes").push(text.len());
        Safe.sanitize(text, path)
    }
    fn redact_key_blocks<'a>(&self, content: &'a str) -> (std::borrow::Cow<'a, str>, bool) {
        (content.replace("KEYBODY", "[KEY]").into(), true)
    }
}

#[test]
fn line_pages_scan_only_their_window_and_keep_redaction() {
    let body = format!("TOKEN KEYBODY\n{}", "line of text\n".repeat(4000));
    let scanner = Counting::default();
    let security = &scanner;
    let mut pages = Vec::new();
    for offset in [0, 100, 200] {
        let request: LocalFetchQuery = serde_json::from_value(json!({
            "path":"big.txt", "ranges":[{"startLine":offset+1,"endLine":offset+100}]
        }))
        .expect("canonical line range");
        let page = process_fetched_content(
            &request,
            body.as_bytes(),
            Path::new("big.txt"),
            &Default::default(),
            security,
            &NeverCancel,
        );
        pages.push(page.content.unwrap_or_default());
    }
    assert!(
        scanner
            .sizes
            .lock()
            .expect("sizes")
            .iter()
            .all(|size| *size < body.len())
    );
    let before = scanner.scans.load(std::sync::atomic::Ordering::SeqCst);
    assert!(pages[0].starts_with("[REDACTED] [KEY]\n"), "{}", pages[0]);
    assert_eq!(pages[1], "line of text\n".repeat(100));
    // Independent reads scan the current bytes every time.
    security
        .sanitize("x".repeat(20_000).as_str(), Path::new("other"))
        .expect("scan");
    security
        .sanitize("x".repeat(20_000).as_str(), Path::new("other"))
        .expect("scan");
    assert_eq!(
        scanner.scans.load(std::sync::atomic::Ordering::SeqCst),
        before + 2
    );
}

#[tokio::test]
async fn shared_pipeline_ranges_redacts_and_pins_remote_identity() {
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
        "ranges":[{"startLine":1,"endLine":2}]}))
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
    assert!(result.file.calls.is_none(), "bounded range is complete");
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
        "matchString":["TOKEN"], "contextLines":{"before":0,"after":0}}))
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

/// RFC 4716 armor through the shared read pipeline: a range inside the block
/// and a body line's own text return none of it.
#[tokio::test]
async fn an_rfc4716_key_body_is_never_returned() {
    let body = "UvImZaYMEtKJGF2VDuiBNgkWb2sRPReNbA/TkB/yOaGglfIPk5VlDPk4C47bIkpr";
    let source = format!(
        "one\n---- BEGIN SSH2 ENCRYPTED PRIVATE KEY ----\nComment: \"fake\"\n{body}\n{body}\n---- END SSH2 ENCRYPTED PRIVATE KEY ----\nlast\n"
    );
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(
        &server,
        "/api/v3/repos/a/b/commits/main",
        200,
        serde_json::json!({"sha":sha}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/k.txt",
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&source)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    for fields in [
        serde_json::json!({"ranges":[{"startLine":4,"endLine":5}]}),
        serde_json::json!({"ranges":[{"startLine":1,"endLine":7}]}),
        serde_json::json!({"matchString":[&body[..24]],"contextLines":{"before":1,"after":1}}),
    ] {
        let mut query =
            serde_json::json!({"owner": "a", "repo": "b", "path": "k.txt", "ref": "main"});
        for (key, value) in fields.as_object().expect("fields") {
            query[key] = value.clone();
        }
        let query: GhGetFileContentQuery = serde_json::from_value(query).expect("query");
        let result = execute_default_regex(
            &provider,
            &query,
            &fixture_context(Duration::from_secs(2), 4096),
            None,
        )
        .await
        .expect("result");
        let shown = result.file.content.content.unwrap_or_default();
        assert!(!shown.contains(&body[..24]), "{fields}: {shown}");
    }
}

/// A scanner of real redaction that counts its scans of a whole source.
#[derive(Clone, Default)]
struct CountedSecurity(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl ContentScan for CountedSecurity {
    fn sanitize(
        &self,
        text: &str,
        path: &Path,
    ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        if text.len() > 100_000 {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        crate::security::ContentSecurity::new().sanitize(text, path)
    }
}

/// A repeat of a remote range read answers the same row without scanning the
/// file again, and identical bytes at another path keep that path's redaction.
#[tokio::test]
async fn a_repeated_range_reuses_its_scan_under_its_own_path() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(
        &server,
        "/api/v3/repos/a/b/commits/main",
        200,
        serde_json::json!({"sha":sha}),
    )
    .await;
    let body = format!(
        "# shared gh bytes\ndb:\n  password: hunter2hunter2hunter2\n{}",
        "const FILLER: u8 = 0;\n".repeat(5_000)
    );
    for file in ["src%2Fa.rs", "conf%2Fa.yaml"] {
        mount_json(
            &server,
            &format!("/api/v3/repos/a/b/contents/{file}"),
            200,
            serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&body)}),
        )
        .await;
    }
    let provider = mock_provider(&server, RetryPolicy::default());
    let scan = CountedSecurity::default();
    let read = async |file: &str| {
        let query: GhGetFileContentQuery = serde_json::from_value(serde_json::json!({
            "owner": "a", "repo": "b", "path": file, "ref": "main",
            "ranges":[{"startLine":1,"endLine":3}]}))
        .expect("query");
        let selection = crate::tools::test_support::selection_fake::runtime(
            crate::tools::test_support::selection_fake::SelectAll,
            tokio::runtime::Handle::current(),
            Duration::from_secs(10),
        );
        let result = execute(
            &provider,
            &query,
            &default_context(),
            Some(50_000),
            Shaping {
                security: scan.clone(),
                context: selection.context.clone(),
                selection,
            },
            &mut None,
        )
        .await
        .expect("result");
        serde_json::to_value(&result.file).expect("row")
    };
    let scans = || scan.0.load(std::sync::atomic::Ordering::SeqCst);
    let code = read("src/a.rs").await;
    assert_eq!(scans(), 1);
    assert_eq!(read("src/a.rs").await, code);
    assert_eq!(scans(), 1);
    let yaml = read("conf/a.yaml").await;
    assert_eq!(scans(), 2);
    assert_eq!(read("conf/a.yaml").await, yaml);
    assert_eq!(scans(), 2);
    assert!(
        code["content"].as_str().unwrap().contains("hunter2"),
        "{code}"
    );
    assert!(
        !yaml["content"].as_str().unwrap().contains("hunter2"),
        "{yaml}"
    );
    assert_eq!(read("src/a.rs").await, code);
}
