//! Commit rows: the commit SHA, path+stat files, movable refs pinned once, flagged patchless files, patch streams and secret-safe cuts.
use super::*;

#[tokio::test]
async fn commit_patch_continuations_preserve_match_context_and_exact_stream() {
    let server = MockServer::builder().start().await;
    let patch = std::iter::once("@@ -1,47 +1,50 @@".to_owned())
        .chain((1..=50).map(|line| {
            if [10, 25, 40].contains(&line) {
                format!("+needle{line:02}")
            } else {
                format!(" context{line:02}")
            }
        }))
        .collect::<Vec<_>>()
        .join("\n");
    let mut raw = commit_raw(HEAD, "matching patches");
    raw["files"] = json!([{
        "filename":"sample.rs","status":"modified","additions":3,"deletions":0,
        "patch":patch
    }]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    for context in [0, 5] {
        let mut query = json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,
            "sections":["patches"],"matchString":["needle"],"contextLines":{"before":context,"after":context}});
        let whole = read(&server, query.clone()).await;
        let expected = whole["files"][0]["patch"].as_str().expect("whole patch");
        let mut actual = String::new();
        let mut pages = 0;
        loop {
            pages += 1;
            assert!(pages < 100, "patch walk must terminate");
            let data = read_with_budget(&server, query.clone(), Some(40)).await;
            assert_history_page_contract(&query, &data);
            actual.push_str(data["files"][0]["patch"].as_str().expect("patch window"));
            let Some(next) = data.pointer("/next/continuePatch/input") else {
                break;
            };
            let normalized = crate::contracts::validate_built("ghGetHistoryItem", next.clone())
                .expect("executable copied patch continuation");
            query = normalized["queries"][0].clone();
            assert_eq!(
                query["contextLines"],
                json!({"before":context,"after":context}),
                "{query}"
            );
        }
        assert!(pages > 1, "the fixture must exercise continuation");
        assert_eq!(actual, expected, "contextLines={context}");
    }
}

#[tokio::test]
async fn commit_file_batches_resolve_a_movable_ref_once_before_paging() {
    let server = MockServer::builder().start().await;
    let base_url = server.uri();
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex("/api/v3/repos/o/r/commits/"))
        .respond_with(move |request: &wiremock::Request| {
            let reference = request.url.path().rsplit('/').next().expect("ref");
            if request.headers.get("accept").is_some_and(|accept| {
                accept == "application/vnd.github.sha"
            }) {
                return ResponseTemplate::new(200).set_body_string(HEAD);
            }
            let page = request
                .url
                .query_pairs()
                .find(|(key, _)| key == "page")
                .map(|(_, value)| value.into_owned())
                .unwrap_or_else(|| "1".to_owned());
            let moved = reference == "main" && page != "1";
            let mut raw = commit_raw(if moved { MERGE_BASE } else { HEAD }, "moving branch");
            raw["files"] = if page == "1" {
                json!((0..100).map(|i| changed_file(&format!("old-{i:03}.rs"))).collect::<Vec<_>>())
            } else {
                json!([changed_file(if moved { "new-only.rs" } else { "old-last.rs" })])
            };
            let mut response = ResponseTemplate::new(200).set_body_json(raw);
            if page == "1" {
                response = response.insert_header(
                    "Link",
                    format!("<{base_url}/api/v3/repos/o/r/commits/{reference}?page=2&per_page=100>; rel=\"next\""),
                );
            }
            response
        })
        .mount(&server)
        .await;
    let query = json!({"operation":"commit","owner":"o","repo":"r","ref":"main",
        "sections":["files"],"include":["old-last.rs"]});
    let data = read(&server, query.clone()).await;
    assert_history_page_contract(&query, &data);
    assert_eq!(data["commitSha"], HEAD, "{data}");
    assert_eq!(data["files"], json!(["M +1 -1 old-last.rs"]), "{data}");
    let requests = server.received_requests().await.expect("requests");
    assert_eq!(requests.len(), 3, "one resolution and two pinned batches");
    assert!(requests[0].url.path().ends_with("/main"));
    assert!(
        requests[1..]
            .iter()
            .all(|request| request.url.path().ends_with(HEAD))
    );
}

/// A read by branch whose page one batch holds reads the commit by that
/// name once: the answer names the commit, and every continuation is
/// pinned to it, so no separate resolution is sent.
#[tokio::test]
async fn a_one_batch_commit_read_by_branch_sends_no_separate_resolution() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "Fix parser");
    raw["files"] = json!(
        (0..30)
            .map(|i| changed_file(&format!("f-{i:02}.rs")))
            .collect::<Vec<_>>()
    );
    mount_json(&server, "/api/v3/repos/o/r/commits/main", 200, raw).await;
    let query = json!({"operation":"commit","owner":"o","repo":"r","ref":"main",
        "sections":["files"],"pageSize":10});
    let data = read(&server, query.clone()).await;
    assert_history_page_contract(&query, &data);
    assert_eq!(data["commitSha"], HEAD, "{data}");
    let requests = server.received_requests().await.expect("requests");
    assert_eq!(requests.len(), 1, "one read of the commit by its name");
    assert_eq!(
        data.pointer("/next/nextFilePage/input/queries/0/ref"),
        Some(&json!(HEAD)),
        "{data}"
    );
}

#[tokio::test]
async fn commit_row_carries_its_sha_and_path_stat_files() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "Fix parser");
    raw["parents"] = json!([{"sha":BASE}]);
    raw["stats"] = json!({"additions":1,"deletions":1});
    raw["files"] = json!([changed_file("src/a.rs")]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let row = json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD});
    let out = read(&server, row.clone()).await;
    assert_eq!(out["commitSha"], HEAD, "{out}");
    assert_eq!(out["files"], json!(["M +1 -1 src/a.rs"]), "{out}");
    // A caller's SHA is not restated as its ref; a page listing every file
    // restates no count.
    assert!(out.get("ref").is_none(), "{out}");
    assert!(out.get("changedFilesCount").is_none(), "{out}");
    // Patches: each line numbered on its own side.
    let mut patched = row;
    patched["sections"] = json!(["patches"]);
    let out = read(&server, patched).await;
    let file = &out["files"][0];
    assert_eq!(file["path"], "src/a.rs", "{out}");
    let patch = file["patch"].as_str().expect("patch");
    assert!(patch.contains("10\t keep"), "{patch}");
    assert!(patch.contains("11\t-old"), "{patch}");
    assert!(patch.contains("11\t+new"), "{patch}");
}

/// A commit's file inventory flags a file GitHub sent without a patch, as
/// a pull request's does: a binary file's `+0 -0` never reads as an empty
/// change (react v18.2.0...v19.0.0: favicon.ico, graphvizlib.wasm).
#[tokio::test]
async fn commit_inventory_flags_files_without_a_patch() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "Add assets");
    raw["files"] = json!([
        {"filename":"public/favicon.ico","status":"added","additions":0,"deletions":0,
            "sha":"4444444444444444444444444444444444444444"},
        {"filename":"src/old.rs","status":"renamed","additions":0,"deletions":0,
            "previous_filename":"src/older.rs","sha":"5555555555555555555555555555555555555555"},
    ]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD}),
    )
    .await;
    assert_eq!(
        out["files"],
        json!([
            "A +0 -0 !binary public/favicon.ico",
            "R +0 -0 src/old.rs <- src/older.rs"
        ]),
        "{out}"
    );
}

/// A long patch line is cut to windows around the `matchString` hits; a
/// secret the cut splits (its `ghp_` prefix outside the window) must not
/// show in part: the patch is sanitized before its lines are cut.
#[tokio::test]
async fn a_cut_patch_line_never_shows_part_of_a_secret() {
    let secret = format!("ghp_{}", "Zq7".repeat(12));
    let server = MockServer::builder().start().await;
    for gap in [130, 140, 150] {
        let line = format!(
            "+{}const t = \"{secret}\";{}needle{}",
            "a".repeat(300),
            " ".repeat(gap - 3),
            "b".repeat(300)
        );
        let mut raw = commit_raw(HEAD, "Add token");
        raw["files"] = json!([{"filename":"src/a.js","status":"modified","additions":1,
            "deletions":0,"patch":format!("@@ -1,0 +1,1 @@\n{line}")}]);
        server.reset().await;
        mount_json(
            &server,
            format!("/api/v3/repos/o/r/commits/{HEAD}"),
            200,
            raw,
        )
        .await;
        let provider = mock_provider(&server, RetryPolicy::default());
        let context = fixture_context(Duration::from_secs(10), 4 << 20);
        let request = HistoryItemRequest::from_row(
            json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,"matchString":["needle"]}),
        )
        .expect("valid row");
        let out = execute(
            &provider,
            &request,
            &context,
            &crate::security::ContentSecurity::new(),
        )
        .await
        .expect("history item")
        .data;
        let shown = out["files"].to_string();
        assert!(shown.contains("needle"), "{out}");
        assert!(!shown.contains(&secret[8..28]), "gap {gap}: {out}");
    }
}
