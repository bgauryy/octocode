use super::*;
use crate::execution::cancel::NeverCancel;
use crate::security::scan::{MemoizedScan, SanitizedViewMemo};
use crate::tools::gh_shared::test_support::{default_context, fixture_context, mock_provider};
use crate::tools::local_fetch::WindowUnit;
use crate::tools::local_fetch::types::test_local_fetch_query;
use crate::tools::local_fetch::wire_positive;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_contract_types::ErrorCode;
use octocode_github::RetryPolicy;
use octocode_github::mock_http::mount_json;
use std::{path::Path, time::Duration};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

/// Shaping over the test scanner [`Safe`]; a ranked read judges with
/// `judge` (a plain read never calls it).
fn shaping(judge: impl crate::tools::selection::SelectionJudge + 'static) -> Shaping<Safe> {
    let selection = crate::tools::test_support::selection_fake::runtime(
        judge,
        tokio::runtime::Handle::current(),
        Duration::from_secs(10),
    );
    Shaping {
        security: Safe,
        memo: Arc::new(SanitizedViewMemo::new()),
        context: selection.context.clone(),
        selection,
    }
}

async fn execute_default_regex(
    provider: &GitHubProvider,
    query: &GhGetFileContentQuery,
    request_context: &RequestContext,
    window: Option<usize>,
) -> Result<GhGetFileContentResult, ProviderError> {
    let select_all = crate::tools::test_support::selection_fake::SelectAll;
    execute(
        provider,
        query,
        request_context,
        window,
        shaping(select_all),
    )
    .await
}

#[derive(Clone)]
struct Safe;
impl ContentScan for Safe {
    fn sanitize(&self, text: &str, _: &Path) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        Ok((text.replace("TOKEN", "[REDACTED]"), vec![]))
    }
}

struct TimedJudge(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl crate::tools::selection::SelectionJudge for TimedJudge {
    fn identity(&self) -> Value {
        json!({"model":"timed-test"})
    }

    fn select(
        &self,
        call: crate::tools::selection::JudgeCall<'_>,
    ) -> crate::tools::selection::Judgments {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        call.handle.block_on(async {
            tokio::time::sleep(Duration::from_millis(5)).await;
        });
        Ok(call
            .admitted
            .iter()
            .map(|&index| (index, crate::tools::selection::Decision::Selected))
            .collect())
    }
}

async fn selected_body_is_shaped_off_the_acquisition_worker() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(&server, "/api/v3/repos/a/b/contents/large.txt", 200, json!({
        "type":"file", "encoding":"base64", "content":STANDARD.encode("line with useful evidence and a long suffix\n".repeat(300))
    })).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let query: GhGetFileContentQuery = serde_json::from_value(json!({
        "owner":"a", "repo":"b", "path":"large.txt", "ref":sha,
        "unit":"lines", "length":100, "clasifyContext":{"classifiers":["find useful evidence"]}
    }))
    .expect("ranked query");
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        execute(
            &provider,
            &query,
            &fixture_context(Duration::from_secs(10), 1 << 20),
            Some(50_000),
            shaping(TimedJudge(calls.clone())),
        ),
    )
    .await
    .expect("ranked read must not block the reactor")
    .expect("ranked read");
    assert_eq!(
        result.file.content.selection.as_ref().map(|r| &r.status),
        Some(&octocode_contract_types::SelectionStatus::Complete)
    );
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

/// Explicit compact views cannot supply raw classification evidence. They
/// fail clearly and offer unfiltered recovery pinned to the fetched commit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explicit_minification_refuses_selection_and_offers_pinned_recovery() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let body: String = (1..=1050)
        .map(|i| {
            if i % 3 == 0 {
                format!("// note {i}\n")
            } else {
                format!("var value_{i} = compute({i}, 'some longer argument text');\n")
            }
        })
        .collect();
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/lib%2Fr.js",
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&body)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let owned = shaping(TimedJudge(calls));
    let mut pages = Vec::new();
    for classifiers in [
        None,
        Some(serde_json::json!(["where is value 500 computed"])),
    ] {
        let mut row = serde_json::json!({
            "owner":"a","repo":"b","path":"lib/r.js","ref":sha,"minify":"standard","mainGoal":"test","reasoning":"test"
        });
        if let Some(classifiers) = classifiers {
            row["clasifyContext"] = json!({"classifiers":classifiers});
        }
        let query: GhGetFileContentQuery = serde_json::from_value(row).expect("query");
        let result = execute(
            &provider,
            &query,
            &fixture_context(Duration::from_secs(10), 1 << 20),
            Some(50_000),
            owned.clone(),
        )
        .await
        .expect("read");
        pages.push(result.file.clone());
    }
    let (plain, classifiers) = (&pages[0], &pages[1]);
    assert!(plain.content.pagination.is_some(), "{plain:?}");
    assert_eq!(
        classifiers.content.error_code,
        Some(ErrorCode::InvalidInput)
    );
    assert!(classifiers.content.content.is_none());
    let recovery =
        &classifiers.next.as_ref().expect("recovery")["unfiltered"]["query"]["queries"][0];
    assert_eq!(recovery["ref"], sha);
    assert!(recovery.get("clasifyContext").is_none());
}

#[test]
fn selected_gh_body_runs_on_current_and_multithread_runtimes() {
    let current = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("current runtime");
    current.block_on(selected_body_is_shaped_off_the_acquisition_worker());
    let multi = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("multithread runtime");
    multi.block_on(selected_body_is_shaped_off_the_acquisition_worker());
}

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

async fn read(provider: &GitHubProvider, query: Value) -> GhGetFileContentResult {
    let mut query = query;
    query["mainGoal"] = "test".into();
    query["reasoning"] = "test".into();
    let query: GhGetFileContentQuery = serde_json::from_value(query).expect("query");
    execute_default_regex(provider, &query, &default_context(), Some(50_000))
        .await
        .expect("result")
}

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

/// A window is exactly what was asked: a `ranges` or `matchString`
/// read of a small file returns those lines, never the whole file, in
/// the default view and verbatim alike. A `ranges` read can always ask
/// for more.
#[tokio::test]
async fn windows_stay_windows_on_small_files() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let small: String = (1..=30).map(|i| format!("line {i}\n")).collect();
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/small.py".to_owned(),
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&small)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let base = || serde_json::json!({"owner":"a","repo":"b","path":"small.py","ref":sha});
    let mut matched = base();
    matched["matchString"] = "line 12".into();
    matched["contextLines"] = 1.into();
    let window = read(&provider, matched).await;
    let file = &window.file.content;
    assert_eq!(file.matched_lines, vec![12]);
    assert_eq!(
        file.source_line_ranges,
        vec![crate::tools::local_fetch::LineRange { start: 11, end: 13 }]
    );
    for minify in [None, Some("standard")] {
        let mut ranged = base();
        ranged["ranges"] = serde_json::json!(["1-15"]);
        if let Some(mode) = minify {
            ranged["minify"] = mode.into();
        }
        let window = read(&provider, ranged).await;
        let content = window.file.content.content.clone().unwrap_or_default();
        assert!(content.contains("line 15"), "{content}");
        assert!(!content.contains("line 16"), "{minify:?}: {content}");
    }
}

/// Explicitly requested lines come back verbatim under the default view:
/// a `ranges` read of comment lines is not emptied by the standard view,
/// and a whole-file read keeps every line.
#[tokio::test]
async fn ranges_and_full_content_are_line_faithful_under_the_default_view() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let body = "/*!\n * express\n * MIT Licensed\n */\n\n'use strict';\n\n// a note\nvar a = 1;\n";
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/lib%2Fr.js",
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(body)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let ranged = read(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"lib/r.js","ref":sha,"ranges":["1-3"]}),
    )
    .await;
    let file = &ranged.file.content;
    assert_eq!(file.status, ReadStatus::Success, "{file:?}");
    assert_eq!(
        file.content.as_deref(),
        Some("/*!\n * express\n * MIT Licensed\n"),
        "{file:?}"
    );
    let whole = read(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"lib/r.js","ref":sha,"fullContent":true}),
    )
    .await;
    let file = &whole.file.content;
    assert_eq!(file.content.as_deref(), Some(body), "{file:?}");
    assert_eq!(file.returned_lines, Some(9), "{file:?}");
}

/// `ranges` that all start past the file end fail with a runnable
/// repair: the same read of the file's last window, as wide as the
/// widest requested range, at the read commit.
#[tokio::test]
async fn ranges_past_the_end_repair_to_the_last_window() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let small: String = (1..=30).map(|i| format!("line {i}\n")).collect();
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/small.py".to_owned(),
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&small)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let past = read(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"small.py","ref":sha,"ranges":["99-103","200-201"]}),
    )
    .await;
    let file = &past.file;
    assert!(file.content.error.is_some(), "{:?}", file.content);
    let repair = &file.next.as_ref().expect("repair lead")["repair"];
    assert_eq!(repair["tool"], "ghGetFileContent", "{repair}");
    let row = &repair["query"]["queries"][0];
    assert_eq!(row["ranges"], serde_json::json!(["26-30"]), "{repair}");
    assert_eq!(row["ref"], sha, "{repair}");
    assert_eq!(row["path"], "small.py", "{repair}");
    // A range that reaches into the file is read (clamped), not repaired.
    let inside = read(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"small.py","ref":sha,"ranges":["29-40"]}),
    )
    .await;
    assert!(inside.file.content.error.is_none());
    assert!(
        inside
            .file
            .next
            .as_ref()
            .is_none_or(|next| next.get("repair").is_none()),
        "{:?}",
        inside.file.next
    );
}

/// A path-only read of a file over one page gets localFetch's guarded
/// head, not a 2,000-line page; the continuation reaches the rest, and
/// an explicit whole-file read stays one call away.
#[tokio::test]
async fn a_path_only_read_of_a_large_file_returns_a_head_and_a_continuation() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let body: String = (1..=1_500).map(|i| format!("line of text {i}\n")).collect();
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/big.txt",
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&body)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let base = serde_json::json!({"owner":"a","repo":"b","path":"big.txt","ref":sha});
    let head = read(&provider, base.clone()).await;
    let file = &head.file;
    assert_eq!(
        file.content.returned_lines,
        Some(crate::id::local_fetch_policy::HEAD_LINES),
        "{:?}",
        file.content.warnings
    );
    assert!(
        file.content.warnings.is_empty(),
        "{:?}",
        file.content.warnings
    );
    let next = file.next.clone().expect("continuation");
    let page = &next["continueWindow"]["query"]["queries"][0];
    assert_eq!(
        page["offset"],
        crate::id::local_fetch_policy::HEAD_LINES,
        "{next}"
    );
    assert_eq!(page["path"], "big.txt", "{next}");
    // Explicit whole-file read, and anchored reads, are not cut to a head.
    let mut whole = base.clone();
    whole["fullContent"] = true.into();
    let whole = read(&provider, whole).await;
    assert!(whole.file.content.returned_lines.expect("lines") > 50);
    let mut ranged = base;
    ranged["ranges"] = serde_json::json!(["10-200"]);
    let ranged = read(&provider, ranged).await;
    assert_eq!(ranged.file.content.returned_lines, Some(191));
}

/// A path-only read of a minified file (one oversized line) pages by
/// bytes; every `next.continueWindow` carries the page's unit, so following
/// them verbatim reaches the end byte-identical.
#[tokio::test]
async fn a_minified_file_walks_to_its_end_through_next_continue() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let body: String = (0..40_000)
        .map(|i| format!("needle{i} "))
        .collect::<String>()
        .chars()
        .take(300_000)
        .collect();
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/min.js",
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&body)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let mut query = serde_json::json!({"owner":"a","repo":"b","path":"min.js","ref":sha});
    let (mut text, mut pages) = (String::new(), 0);
    loop {
        let result = read(&provider, query).await;
        let file = &result.file;
        assert_eq!(file.content.error_code, None, "{:?}", file.content);
        assert!(!file.content.out_of_range, "{:?}", file.content);
        text.push_str(file.content.content.as_deref().unwrap_or_default());
        pages += 1;
        assert!(pages < 40, "runaway paging");
        match file
            .next
            .as_ref()
            .map(|next| next["continueWindow"].clone())
        {
            Some(next) if !next.is_null() => {
                query = next["query"]["queries"][0].clone();
            }
            _ => break,
        }
    }
    assert!(pages >= 18, "{pages} pages");
    assert!(text == body, "pages join into the file exactly once");
}

/// Continuations and leads carry only fields that change the read: the
/// default view (`fullContent` off, raw `minify: none`, sent or not) is
/// omitted, a chosen `minify: standard` is kept, and a whole-file read
/// pages without `fullContent` in the raw view its first page used.
#[tokio::test]
async fn continuations_omit_default_view_fields() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let body: String = (1..=6000).map(|i| format!("line of text {i}\n")).collect();
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/big.txt",
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&body)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let base = serde_json::json!({"owner":"a","repo":"b","path":"big.txt","ref":sha});
    let mut paged = base.clone();
    paged["unit"] = "lines".into();
    paged["length"] = 100.into();
    let mut standard = paged.clone();
    standard["minify"] = "standard".into();
    let mut whole = base;
    whole["fullContent"] = true.into();
    for (query, minify) in [(paged, None), (standard, Some("standard")), (whole, None)] {
        let result = read(&provider, query).await;
        let next = result.file.next.clone().expect("continuation");
        let page = &next["continueWindow"]["query"]["queries"][0];
        assert!(page.get("fullContent").is_none(), "{next}");
        assert_eq!(page.get("minify").and_then(Value::as_str), minify, "{next}");
        assert_eq!(page["ref"], sha, "{next}");
    }
}

/// An unsized page is paged as localFetch pages it: every later page's
/// continuation stays unsized (no injected `length`), and the chain
/// still reaches the file's end.
#[tokio::test]
async fn unsized_page_continuations_stay_unsized() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let body: String = (1..=6000).map(|i| format!("line of text {i}\n")).collect();
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/big.txt",
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&body)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let mut query = serde_json::json!({"owner":"a","repo":"b","path":"big.txt","ref":sha});
    let mut pages = 0;
    loop {
        let result = read(&provider, query.clone()).await;
        pages += 1;
        let Some(next) = result.file.next.clone() else {
            break;
        };
        let page = next["continueWindow"]["query"]["queries"][0].clone();
        assert!(page.get("length").is_none(), "page {pages}: {next}");
        assert!(page.get("unit").is_none(), "page {pages}: {next}");
        assert!(pages < 100, "the chain never ends: {next}");
        query = page;
    }
    assert!(pages > 2, "the file spans several pages");
}

#[tokio::test]
async fn oversized_full_content_is_partial_not_empty() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(&server, "/api/v3/repos/a/b/contents/big.txt", 200, serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("line of text\n".repeat(6000))})).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let query: GhGetFileContentQuery = serde_json::from_value(serde_json::json!({
        "owner": "a", "repo": "b", "path": "big.txt", "ref": sha,
        "fullContent": true, "mainGoal": "test", "reasoning": "test"
    }))
    .expect("query");
    let result = execute_default_regex(
        &provider,
        &query,
        &fixture_context(Duration::from_secs(2), 1 << 20),
        Some(50_000),
    )
    .await
    .expect("result");
    let file = &result.file;
    // The first bounded page comes back inline, not an empty body.
    assert_eq!(file.content.error_code, None);
    assert_eq!(file.content.status, ReadStatus::Success);
    assert!(
        file.content
            .content
            .as_deref()
            .is_some_and(|text| !text.is_empty())
    );
    assert_eq!(file.content.is_partial, Some(true));
    // The continuation resumes exactly where the page ends: no line of
    // the file falls between the two.
    let window = &file.next.as_ref().expect("next")["continueWindow"]["query"]["queries"][0];
    // Line pages: the offset is the first unread line.
    let line = "line of text\n".len();
    let offset = window["offset"].as_u64().expect("offset") as usize;
    assert!(offset > 0 && offset < 6000, "{window}");
    assert_eq!(file.content.returned_bytes, Some(offset * line), "{window}");
    assert_eq!(window["ref"], sha, "{window}");
}

/// The row's response window sizes a ranged read's page: a smaller
/// window serves fewer lines, a larger one more, and each page ends in
/// the tool's own `continueWindow` at the read commit, so a small
/// `responseLength` never falls through to text pages of the response.
#[tokio::test]
async fn the_response_window_sizes_the_read_page() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let body: String = (1..=3000)
        .map(|line| format!("let value_{line} = compute({line});\n"))
        .collect();
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src%2Fbig.rs",
        200,
        json!({"type":"file","encoding":"base64","content":STANDARD.encode(&body)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let query: GhGetFileContentQuery = serde_json::from_value(json!({
        "owner":"a","repo":"b","path":"src/big.rs","ref":sha,"ranges":["1-3000"],
        "mainGoal":"test","reasoning":"test"
    }))
    .expect("query");
    let mut lines = Vec::new();
    for window in [5_000, 20_000, 50_000] {
        let read = execute_default_regex(&provider, &query, &default_context(), Some(window))
            .await
            .expect("read");
        let file = &read.file;
        let row = serde_json::to_value(&read).expect("row");
        let chars = octocode_engine::json_size::utf16_units(&row).unwrap_or(0);
        assert!(chars <= window, "window {window}: row of {chars} chars");
        let next = file.next.as_ref().expect("next");
        let page = &next["continueWindow"]["query"]["queries"][0];
        assert_eq!(page["ref"], sha, "{next}");
        lines.push(
            file.content
                .content
                .as_deref()
                .unwrap_or_default()
                .lines()
                .count(),
        );
    }
    assert!(lines[0] < lines[1] && lines[1] < lines[2], "{lines:?}");
}

async fn run_default(
    provider: &GitHubProvider,
    query: Value,
) -> Result<FileRead, crate::tools::gh_shared::GhFailure> {
    let mut query = query;
    query["mainGoal"] = "test".into();
    query["reasoning"] = "test".into();
    let query: GhGetFileContentQuery = serde_json::from_value(query).expect("query");
    run(
        provider,
        &query,
        Ok(&default_context()),
        None,
        shaping(crate::tools::test_support::selection_fake::SelectAll),
    )
    .await
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

/// ghGetFileContent reads a path by the one repository-path rule
/// ghStructure lists by: `.`, `..` and repeated slashes resolve before
/// the one contents request, and the row names the resolved path; a
/// path above the root, or the root itself, is invalid input with no
/// request sent.
#[tokio::test]
async fn paths_resolve_like_listings_or_are_refused_unsent() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/README.md",
        200,
        json!({"type":"file", "encoding":"base64", "content":STANDARD.encode("# Title\n")}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let read = run_default(
        &provider,
        json!({"owner":"a","repo":"b","path":"src/.//../README.md","ref":sha}),
    )
    .await
    .unwrap_or_else(|failure| panic!("{}", failure.message));
    assert_eq!(
        read.output.data["path"], "README.md",
        "{}",
        read.output.data
    );
    let requests = server.received_requests().await.expect("requests");
    assert_eq!(requests.len(), 1, "{requests:?}");
    for refused in ["../x.md", "src/../../x.md", "src/..", "./"] {
        let Err(failure) = run_default(
            &provider,
            json!({"owner":"a","repo":"b","path":refused,"ref":sha}),
        )
        .await
        else {
            panic!("{refused} is refused");
        };
        assert_eq!(
            failure.error.kind,
            octocode_github::ProviderErrorKind::Validation,
            "{refused}"
        );
        assert!(
            failure.message.contains(refused),
            "{refused}: {}",
            failure.message
        );
    }
    let requests = server.received_requests().await.expect("requests");
    assert_eq!(requests.len(), 1, "refusals send nothing: {requests:?}");
}

/// A path whose case differs is answered by one listing walk at the
/// resolved commit: one case-corrected `read` lead, and the one hint
/// names where that lead is. No listing at `HEAD` is spent.
#[tokio::test]
async fn wrong_case_path_leads_once_to_the_case_corrected_read() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sha))
        .mount(&server)
        .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src%2FLib.rs",
        404,
        serde_json::json!({"message":"Not Found"}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src",
        200,
        serde_json::json!([
            {"name":"lib.rs","path":"src/lib.rs","type":"file"},
            {"name":"main.rs","path":"src/main.rs","type":"file"}
        ]),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let failure = match run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"src/Lib.rs"}),
    )
    .await
    {
        Err(failure) => failure,
        Ok(_) => panic!("a missing path is a failure"),
    };
    let next = failure.next.expect("recovery leads");
    assert_eq!(
        next["read"]["query"]["queries"][0]["path"], "src/lib.rs",
        "{next}"
    );
    assert_eq!(
        failure.hints,
        vec!["Only the path's case differs.".to_owned()]
    );
    let requests = server.received_requests().await.unwrap_or_default();
    assert!(
        !requests
            .iter()
            .any(|request| request.url.query().is_some_and(|q| q.contains("ref=HEAD"))),
        "a listing at HEAD was spent: {requests:?}"
    );
}

/// A matchString with no hit in an existing file is `matchNotFound` with
/// an empty `content` (the contract requires it, as in localFetch) and
/// its leads; it carries no prose beside them, no zero counters and no
/// echo of the request.
#[tokio::test]
async fn no_match_read_keeps_its_leads_without_empty_echo() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(&server, "/api/v3/repos/a/b/contents/a.txt", 200, serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("one\ntwo\n")}),).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let read = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"a.txt","ref":sha,"matchString":"absent"}),
    )
    .await
    .expect("read");
    let data = &read.output.data;
    assert_eq!(read.output.status, Some("empty"), "{data}");
    assert_eq!(data["matchNotFound"], true, "{data}");
    assert_eq!(data["content"], "", "{data}");
    crate::contracts::validate_output(
        "ghGetFileContent",
        &serde_json::json!({"results":[{"index":0,"status":"empty","data":data}]}),
    )
    .expect("an empty match row satisfies the output contract");
    for junk in [
        "returnedLines",
        "returnedChars",
        "searchedFor",
        "pagination",
        "hints",
        "warnings",
    ] {
        assert!(data.get(junk).is_none(), "{junk}: {data}");
    }
    // FIX §0 #4: the runnable lead is the next step: a repository search
    // for a small file.
    let lead = &data["next"]["findCode"];
    assert_eq!(lead["tool"], "ghSearchCode", "{data}");
    assert_eq!(
        lead["query"]["queries"][0]["keywords"],
        serde_json::json!(["absent"]),
        "{data}"
    );
    // A miss that no case-insensitive read would select has no
    // ignoreCase lead.
    assert!(data["next"].get("ignoreCase").is_none(), "{data}");
    // The shared localFetch miss offers a localSearch of the file's
    // directory; it has no GitHub equivalent, so it must not be rewritten
    // into a ghGetFileContent read of that directory (an invalidInput
    // "is a directory" row when run verbatim).
    assert!(data["next"].get("findText").is_none(), "{data}");
    for (name, lead) in data["next"].as_object().into_iter().flatten() {
        if lead["tool"] == "ghGetFileContent" {
            assert_eq!(
                lead["query"]["queries"][0]["path"], "a.txt",
                "{name} reads another path: {data}"
            );
        }
    }

    // A case-sensitive miss that a case-insensitive read selects runs
    // the same read, ignoring case, at the read's commit; the count of
    // lines it selects is the fact beside the lead.
    let sensitive = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"a.txt","ref":sha,"matchString":"Two"}),
    )
    .await
    .expect("read");
    let data = &sensitive.output.data;
    assert_eq!(
        data["warnings"],
        serde_json::json!(["1 line matches ignoring case."]),
        "{data}"
    );
    assert!(data.get("hints").is_none(), "{data}");
    let row = &data["next"]["ignoreCase"]["query"]["queries"][0];
    assert_eq!(data["next"]["ignoreCase"]["tool"], "ghGetFileContent");
    assert_eq!(row["caseMode"], "insensitive", "{data}");
    assert_eq!(row["matchString"], "Two", "{data}");
    assert_eq!(row["ref"], sha, "{data}");
    crate::contracts::validate_output(
        "ghGetFileContent",
        &serde_json::json!({"results":[{"index":0,"status":"empty","data":data}]}),
    )
    .expect("an ignoreCase lead satisfies the output contract");
}

/// A miss offers only reads that differ from the request and that select
/// lines of the redacted text: no ignoreCase lead for a read already
/// case-insensitive, none for a hit hidden behind a redaction, and a
/// `ranges` miss with hits elsewhere in this file leads to the same read
/// without `ranges`, not back to a repository search.
#[tokio::test]
async fn a_miss_leads_only_to_reads_that_select_visible_lines() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(&server, "/api/v3/repos/a/b/contents/a.txt", 200, serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("one\nkey TOKEN\nthree\nfour needle\n")}),).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let base = serde_json::json!({"owner":"a","repo":"b","path":"a.txt","ref":sha});
    let with = |fields: Value| {
        let mut row = base.clone();
        for (key, value) in fields.as_object().into_iter().flatten() {
            row[key] = value.clone();
        }
        row
    };
    for fields in [
        serde_json::json!({"matchString":"token"}),
        serde_json::json!({"matchString":"TOKEN","caseMode":"insensitive"}),
    ] {
        let read = run_default(&provider, with(fields.clone()))
            .await
            .expect("read");
        let data = &read.output.data;
        assert_eq!(data["matchNotFound"], true, "{fields}: {data}");
        assert!(data["next"].get("ignoreCase").is_none(), "{fields}: {data}");
        assert!(data.get("warnings").is_none(), "{fields}: {data}");
    }
    let ranged = run_default(
        &provider,
        with(serde_json::json!({"matchString":"needle","ranges":["1-2"]})),
    )
    .await
    .expect("read");
    let data = &ranged.output.data;
    assert_eq!(data["matchNotFound"], true, "{data}");
    assert_eq!(
        data["warnings"],
        serde_json::json!(["1 line matches outside the requested ranges."]),
        "{data}"
    );
    let lead = &data["next"]["read"];
    assert_eq!(lead["tool"], "ghGetFileContent", "{data}");
    let row = &lead["query"]["queries"][0];
    assert_eq!(row["matchString"], "needle", "{data}");
    assert!(row.get("ranges").is_none(), "{data}");
    assert_eq!(row["ref"], sha, "{data}");
    assert!(data["next"].get("findCode").is_none(), "{data}");
}

/// A submodule path leads to the submodule's repository on the
/// configured GitHub host at the commit the parent pins, not to the
/// generic invalid-field tip; a submodule hosted elsewhere gets no
/// GitHub lead.
#[tokio::test]
async fn a_submodule_path_leads_to_its_repository_at_the_pinned_commit() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let pinned = "89abcdef0123456789abcdef0123456789abcdef";
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/vendor%2Flib",
        200,
        serde_json::json!({"type":"submodule","submodule_git_url":format!("{}/o2/r2.git", server.uri()),"sha":pinned}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/vendor%2Fext",
        200,
        serde_json::json!({"type":"submodule","submodule_git_url":"https://gitlab.com/o3/r3.git","sha":pinned}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let Err(failure) = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"vendor/lib","ref":sha}),
    )
    .await
    else {
        panic!("a submodule path is a failure");
    };
    let next = failure.next.expect("submodule lead");
    let lead = &next["viewTree"];
    assert_eq!(lead["tool"], "ghStructure", "{next}");
    let row = &lead["query"]["queries"][0];
    assert_eq!(row["owner"], "o2", "{next}");
    assert_eq!(row["repo"], "r2", "{next}");
    assert_eq!(row["ref"], pinned, "{next}");
    assert!(failure.hints.is_empty(), "{:?}", failure.hints);
    let Err(elsewhere) = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"vendor/ext","ref":sha}),
    )
    .await
    else {
        panic!("a submodule path is a failure");
    };
    assert!(elsewhere.next.is_none(), "{:?}", elsewhere.next);
    assert_eq!(elsewhere.hints.len(), 1, "{:?}", elsewhere.hints);
    assert!(
        !elsewhere.hints[0].contains("rejected field"),
        "{:?}",
        elsewhere.hints
    );
}

/// A submodule URL names a repository only on the host the configured
/// endpoint serves, in every clone URL form.
#[test]
fn submodule_urls_resolve_on_the_configured_host_only() {
    let public = octocode_github::GitHubEndpoint::new(
        url::Url::parse("https://api.github.com").expect("URL"),
    )
    .expect("endpoint");
    let enterprise = octocode_github::GitHubEndpoint::new(
        url::Url::parse("https://ghe.example.com/api/v3").expect("URL"),
    )
    .expect("endpoint");
    let named = Some(("o".to_owned(), "r".to_owned()));
    for url in [
        "https://github.com/o/r.git",
        "http://github.com/o/r",
        "git://github.com/o/r.git",
        "ssh://git@github.com/o/r.git",
        "git@github.com:o/r.git",
    ] {
        assert_eq!(
            crate::tools::gh_shared::hosted_repository(url, &public),
            named,
            "{url}"
        );
        assert_eq!(
            crate::tools::gh_shared::hosted_repository(url, &enterprise),
            None,
            "{url}"
        );
    }
    assert_eq!(
        crate::tools::gh_shared::hosted_repository("git@ghe.example.com:o/r.git", &enterprise),
        named
    );
    assert_eq!(
        crate::tools::gh_shared::hosted_repository("https://gitlab.com/o/r.git", &public),
        None
    );
}

/// The fallback tip follows the query: only a match read is told about
/// matchString.
#[test]
fn the_fallback_hint_names_matchstring_only_for_a_match_read() {
    use crate::response::output::ToolOutput;
    let plain = Output.fallback_hint(&serde_json::json!({"path":"a.js","ranges":["1-3"]}));
    assert!(!plain.contains("matchString"), "{plain}");
    let matched = Output.fallback_hint(&serde_json::json!({"path":"a.js","matchString":"x"}));
    assert!(matched.contains("matchString"), "{matched}");
}

/// A literal that a large file does not contain leads to a repository
/// code search for it (it may live in another file), never to a
/// semantic locate of the same file.
#[tokio::test]
async fn no_match_in_a_large_file_leads_to_a_repository_search() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let text = (0..1200).map(|n| format!("line {n}\n")).collect::<String>();
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/big.txt",
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&text)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let read = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"big.txt","ref":sha,"matchString":"absent"}),
    )
    .await
    .expect("read");
    let data = &read.output.data;
    assert!(data["next"].get("clasify").is_none(), "{data}");
    let lead = &data["next"]["findCode"];
    assert_eq!(lead["tool"], "ghSearchCode", "{data}");
    assert_eq!(
        lead["query"]["queries"][0]["keywords"],
        serde_json::json!(["absent"]),
        "{data}"
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

/// The shared read is cut to the row's window less what a GitHub row adds
/// around it ([`row_envelope_chars`]: the repository and commit beside the
/// content and on each rewritten continuation), so the whole row fits the
/// window its read was given and no response stage reads it again. A
/// whole file just under the window, but over it with the envelope, pages.
#[tokio::test]
async fn the_shared_read_leaves_room_for_the_github_row_envelope() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let body: String = (1..=200)
        .map(|i| format!("line {i:03} of text\n"))
        .collect();
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src%2Flib.rs",
        200,
        json!({"type":"file","encoding":"base64","content":STANDARD.encode(&body)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let query = |fields: Value| -> GhGetFileContentQuery {
        let mut row = json!({"owner":"a","repo":"b","path":"src/lib.rs","ref":sha,
            "mainGoal":"test","reasoning":"test"});
        for (key, value) in fields.as_object().into_iter().flatten() {
            row[key] = value.clone();
        }
        serde_json::from_value(row).expect("query")
    };
    let envelope = row_envelope_chars(&query(json!({})), sha);
    let window = body.len() + envelope / 2;
    let whole = execute_default_regex(
        &provider,
        &query(json!({"fullContent": true})),
        &default_context(),
        Some(window),
    )
    .await
    .expect("whole read");
    assert!(
        whole
            .file
            .next
            .as_ref()
            .is_some_and(|next| next.get("continueWindow").is_some()),
        "a whole file within the window but not its envelope pages: {:?}",
        whole.file.next
    );
    // The envelope covers what the GitHub row adds to the shared read's row.
    let page = query(json!({"unit":"lines","length":20}));
    let github = execute_default_regex(&provider, &page, &default_context(), Some(1 << 20))
        .await
        .expect("line page");
    let shared = process_fetched_content(
        &local_query(&page).expect("local query"),
        body.as_bytes(),
        Path::new("src/lib.rs"),
        &crate::tools::local_fetch::SourceFacts {
            modified: None,
            window: Some(1 << 20),
            beside: 0,
            interior: false,
            byte_origin: None,
        },
        &Safe,
        &NeverCancel,
    );
    let github_chars =
        octocode_engine::json_size::utf16_units(&serde_json::to_value(&github).expect("row"))
            .expect("size");
    let shared_chars = octocode_engine::json_size::utf16_units(&shared).expect("size");
    assert!(
        github_chars <= shared_chars + envelope,
        "{github_chars} > {shared_chars} + {envelope}"
    );
}
