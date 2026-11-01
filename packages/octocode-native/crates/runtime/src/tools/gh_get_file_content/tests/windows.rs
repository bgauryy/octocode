//! Read windows: heads, ranges, minified walks and the response window sizing a page.
use super::*;

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
    matched["matchString"] = json!(["line 12"]);
    matched["contextLines"] = json!({"before":1,"after":1});
    let window = read(&provider, matched).await;
    let file = &window.file.content;
    assert_eq!(file.matched_lines, vec![12]);
    assert_eq!(
        file.source_line_ranges,
        vec![crate::tools::source_view::LineRange { start: 11, end: 13 }]
    );
    for minify in [None, Some("compact")] {
        let mut ranged = base();
        ranged["ranges"] = serde_json::json!([{"startLine":1,"endLine":15}]);
        if let Some(mode) = minify {
            ranged["view"] = mode.into();
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
        serde_json::json!({"owner":"a","repo":"b","path":"lib/r.js","ref":sha,"ranges":[{"startLine": 1, "endLine": 3}]}),
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
        serde_json::json!({"owner":"a","repo":"b","path":"lib/r.js","ref":sha}),
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
        serde_json::json!({"owner":"a","repo":"b","path":"small.py","ref":sha,"ranges":[{"startLine": 99, "endLine": 103}, {"startLine": 200, "endLine": 201}]}),
    )
    .await;
    let file = &past.file;
    assert!(file.content.error.is_some(), "{:?}", file.content);
    let repair = &file.calls.as_ref().expect("repair lead")["repair"];
    assert_eq!(repair["tool"], "ghGetFileContent", "{repair}");
    let row = &repair["input"]["queries"][0];
    assert_eq!(
        row["ranges"],
        json!([{ "startLine":26,"endLine":30}]),
        "{repair}"
    );
    assert_eq!(row["ref"], sha, "{repair}");
    assert_eq!(row["path"], "small.py", "{repair}");
    // A range that reaches into the file is read (clamped), not repaired.
    let inside = read(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"small.py","ref":sha,"ranges":[{"startLine": 29, "endLine": 40}]}),
    )
    .await;
    assert!(inside.file.content.error.is_none());
    assert!(
        inside
            .file
            .calls
            .as_ref()
            .is_none_or(|next| next.get("repair").is_none()),
        "{:?}",
        inside.file.calls
    );
}

/// A path-only read of a file over one page gets localFetch's guarded
/// head, not a 2,000-line page; the continuation reaches the rest, and
/// an explicit whole-file read stays one call away.
#[tokio::test]
async fn default_read_uses_budget_and_canonical_cursor_progress() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let body = "ordinary source line\n".repeat(20_000);
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/big.txt",
        200,
        json!({"type":"file","encoding":"base64","content":STANDARD.encode(&body)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let result = read(
        &provider,
        json!({"owner":"a","repo":"b","path":"big.txt","ref":sha}),
    )
    .await;
    let next = result.file.calls.expect("budget requires continuation");
    let row = &next["nextPage"]["input"]["queries"][0];
    assert_eq!(row["ref"], sha);
    assert!(
        row["readProgress"]["offset"]
            .as_u64()
            .is_some_and(|offset| offset > 0)
    );
    assert!(row.get("offset").is_none());
    assert!(row.get("fullContent").is_none());
}

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
        match file.calls.as_ref().map(|next| next["nextPage"].clone()) {
            Some(next) if !next.is_null() => {
                query = next["input"]["queries"][0].clone();
            }
            _ => break,
        }
    }
    assert!(pages >= 18, "{pages} pages");
    assert!(text == body, "pages join into the file exactly once");
}

#[tokio::test]
async fn oversized_full_content_is_partial_not_empty() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(&server, "/api/v3/repos/a/b/contents/big.txt", 200, serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("line of text\n".repeat(6000))})).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let query: GhGetFileContentQuery = serde_json::from_value(serde_json::json!({
        "owner": "a", "repo": "b", "path": "big.txt", "ref": sha}))
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
    let window = &file.calls.as_ref().expect("next")["nextPage"]["input"]["queries"][0];
    // Line pages: the offset is the first unread line.
    let line = "line of text\n".len();
    let offset = window["readProgress"]["offset"].as_u64().expect("offset") as usize;
    assert!(offset > 0 && offset < 6000, "{window}");
    assert_eq!(file.content.returned_bytes, Some(offset * line), "{window}");
    assert_eq!(window["ref"], sha, "{window}");
}

/// The row's response window sizes a ranged read's page: a smaller
/// window serves fewer lines, a larger one more, and each page ends in
/// the tool's own `nextPage` at the read commit, so a small
/// `responseChars` never falls through to text pages of the response.
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
        "owner":"a","repo":"b","path":"src/big.rs","ref":sha,"ranges":[{"startLine": 1, "endLine": 3000}]}))
    .expect("query");
    let mut lines = Vec::new();
    for window in [5_000, 20_000, 50_000] {
        let read = execute_default_regex(&provider, &query, &default_context(), Some(window))
            .await
            .expect("read");
        let file = &read.file;
        let chars = file
            .content
            .content
            .as_deref()
            .unwrap_or_default()
            .encode_utf16()
            .count();
        assert!(chars <= window, "window {window}: content of {chars} chars");
        let next = file.calls.as_ref().expect("next");
        let page = &next["nextPage"]["input"]["queries"][0];
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

#[tokio::test]
async fn saved_windows_reuse_cached_bytes_or_reacquire_only_the_pinned_commit() {
    let sha = "0123456789abcdef0123456789abcdef01234567";
    for cached in [false, true] {
        let server = MockServer::builder().start().await;
        mount_json(
            &server,
            "/api/v3/repos/a/b/commits/main",
            200,
            json!({"sha":sha}),
        )
        .await;
        let body = "immutable source line\n".repeat(20_000);
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/contents/source.txt"))
            .and(query_param("ref", sha))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"type":"file","encoding":"base64","content":STANDARD.encode(body)}),
            ))
            .expect(if cached { 1 } else { 3 })
            .mount(&server)
            .await;
        let provider = mock_provider(&server, RetryPolicy::default());
        let provider = if cached {
            GitHubProvider::new(
                provider.transport,
                crate::tools::gh_shared::test_support::MemoryCache::default(),
            )
        } else {
            provider
        };
        let first = read(
            &provider,
            json!({"owner":"a","repo":"b","path":"source.txt","ref":"main","refresh":true}),
        )
        .await;
        let row =
            first.file.calls.expect("saved window")["nextPage"]["input"]["queries"][0].clone();
        assert_eq!(row["ref"], sha);
        assert_eq!(row["refresh"], true);
        assert!(
            row["readProgress"]["offset"]
                .as_u64()
                .is_some_and(|offset| offset > 0)
        );
        let before = server.received_requests().await.expect("requests").len();
        let resumed = read(&provider, row.clone()).await;
        let repeated = read(&provider, row).await;
        assert_eq!(resumed.file.content.content, repeated.file.content.content);
        let requests = server.received_requests().await.expect("requests");
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.url.path().ends_with("/commits/main"))
                .count(),
            1
        );
        if cached {
            assert_eq!(
                requests.len(),
                before,
                "cached saved reads make no provider requests"
            );
        }
        server.verify().await;
    }
}
