//! Commit rows: the commit SHA, path+stat files, movable refs pinned once, flagged patchless files, patch streams and secret-safe cuts.
use super::*;
use octocode_test_support::json_fixture;

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
            assert_history_page_contract(&data);
            actual.push_str(data["files"][0]["patch"].as_str().expect("patch window"));
            let Some(next) = data.pointer("/next/patches/input") else {
                break;
            };
            let normalized =
                octocode_runtime_base::contracts::validate_saved("ghGetHistoryItem", next.clone())
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
    let query = json_fixture(
        r#"{
            "operation": "commit",
            "owner": "o",
            "repo": "r",
            "ref": "main",
            "sections": ["files"],
            "include": ["old-last.rs"]
        }"#,
    );
    let data = read(&server, query.clone()).await;
    assert_history_page_contract(&data);
    assert_eq!(data["commitSha"], HEAD, "{data}");
    assert_eq!(
        data["files"],
        json_fixture(r#"["M +1 -1 old-last.rs"]"#),
        "{data}"
    );
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
    let query = json_fixture(
        r#"{
            "operation": "commit",
            "owner": "o",
            "repo": "r",
            "ref": "main",
            "sections": ["files"],
            "pageSize": 10
        }"#,
    );
    let data = read(&server, query.clone()).await;
    assert_history_page_contract(&data);
    assert_eq!(data["commitSha"], HEAD, "{data}");
    let requests = server.received_requests().await.expect("requests");
    assert_eq!(requests.len(), 1, "one read of the commit by its name");
    assert_eq!(
        data.pointer("/next/files/input/queries/0/ref"),
        Some(&json!(HEAD)),
        "{data}"
    );
}

#[tokio::test]
async fn commit_row_carries_its_sha_and_path_stat_files() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "Fix parser");
    raw["parents"] = json!([{"sha":BASE}]);
    raw["stats"] = json_fixture(r#"{"additions": 1, "deletions": 1}"#);
    raw["files"] = json!([changed_file("src/a.rs")]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let row = json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD});
    let mut listed = row.clone();
    listed["sections"] = json_fixture(r#"["files"]"#);
    let out = read(&server, listed).await;
    assert_eq!(out["commitSha"], HEAD, "{out}");
    assert_eq!(
        out["files"],
        json_fixture(r#"["M +1 -1 src/a.rs"]"#),
        "{out}"
    );
    // A caller's SHA is not restated as its ref. A page listing every file
    // still states the count: rows group by directory, so a claim cites
    // `changedFileCount`, never a tally of the rows.
    assert!(out.get("ref").is_none(), "{out}");
    assert_eq!(out["changedFileCount"], 1, "{out}");
    // Patches: each line numbered on its own side.
    let summary = read(&server, row).await;
    let patched = &summary["next"]["patches"]["input"]["queries"][0];
    assert_eq!(patched["ref"], HEAD, "{summary}");
    assert_eq!(patched["sections"], json!(["patches"]), "{summary}");
    let out = read(&server, patched.clone()).await;
    let file = &out["files"][0];
    assert_eq!(file["path"], "src/a.rs", "{out}");
    let patch = file["patch"].as_str().expect("patch");
    assert!(patch.contains("10\t keep"), "{patch}");
    assert!(patch.contains("11\t-old"), "{patch}");
    assert!(patch.contains("11\t+new"), "{patch}");
}

/// A commit read that names no section is the summary the contract states
/// ("metadata and counts, no body"): the commit's facts and its counts, no
/// file row, and the call that opens the file list as a `files` read. A
/// file filter or a literal asks about files and still answers with them.
#[tokio::test]
async fn a_commit_read_without_sections_is_a_summary_that_lists_no_file() {
    let server = MockServer::builder().start().await;
    let files = (0..40)
        .map(|i| changed_file(&format!("f{i:02}.rs")))
        .collect::<Vec<_>>();
    let mut raw = commit_raw(HEAD, "Fix parser (#12)\n\nWhy it broke.");
    raw["parents"] = json!([{"sha":BASE}]);
    raw["stats"] = json!({"additions": 40, "deletions": 40});
    raw["files"] = json!(files);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let row = json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD});
    let summary = read(&server, row.clone()).await;
    assert!(summary.get("files").is_none(), "no file row: {summary}");
    assert_eq!(summary["changedFileCount"], files.len(), "{summary}");
    assert_eq!(
        (&summary["additions"], &summary["deletions"]),
        (&json!(40), &json!(40)),
        "{summary}"
    );
    assert_eq!(summary["pullRequest"], 12, "{summary}");
    assert!(
        summary["message"]
            .as_str()
            .is_some_and(|message| message.contains("Why it broke")),
        "{summary}"
    );
    assert!(summary.get("sectionPagination").is_none(), "{summary}");
    // The list is one call away, as a `files` read of the same commit.
    let open = &summary["next"]["files"]["input"]["queries"][0];
    assert_eq!(open["sections"], json!(["files"]), "{summary}");
    assert_eq!(open["ref"], HEAD, "{summary}");
    let listed = read(&server, open.clone()).await;
    assert_eq!(
        listed["files"].as_array().map(Vec::len),
        Some(files.len()),
        "{listed}"
    );
    // The summary is smaller than the read that lists the files.
    assert!(
        summary.to_string().len() < listed.to_string().len(),
        "{summary}"
    );
    // A file filter asks about files: it answers with them, sections or not.
    let mut scoped = row;
    scoped["include"] = json!(["f07.rs"]);
    let scoped = read(&server, scoped).await;
    assert_eq!(scoped["files"], json!(["M +1 -1 f07.rs"]), "{scoped}");
}

/// A files walk states the commit once: its first page carries the message,
/// people, parents and line counts; every later page names the commit and
/// carries its rows only. Joined, the pages hold the first page's facts and
/// every file exactly once.
#[tokio::test]
async fn a_files_walk_states_the_commit_on_its_first_page_only() {
    let server = MockServer::builder().start().await;
    let names = (0..7).map(|i| format!("f{i}.rs")).collect::<Vec<_>>();
    let message = format!("Fix parser\n\n{}", "A long explanation. ".repeat(200));
    let mut raw = commit_raw(HEAD, &message);
    raw["parents"] = json!([{"sha":BASE}]);
    raw["stats"] = json!({"additions": 7, "deletions": 7});
    raw["files"] = json!(
        names
            .iter()
            .map(|name| changed_file(name))
            .collect::<Vec<_>>()
    );
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let mut row = json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,
        "sections":["files"],"pageSize":3});
    let mut pages = Vec::new();
    loop {
        let page = read(&server, row.clone()).await;
        let next = page["next"]["files"]["input"]["queries"][0].clone();
        pages.push(page);
        if next.is_null() {
            break;
        }
        assert!(pages.len() < 10, "the walk ends");
        row = next;
    }
    assert_eq!(pages.len(), 3, "7 files, 3 per page");
    let first = &pages[0];
    for key in [
        "message",
        "author",
        "parentCommitShas",
        "additions",
        "deletions",
    ] {
        assert!(
            first.get(key).is_some(),
            "the first page states {key}: {first}"
        );
    }
    for later in &pages[1..] {
        assert_eq!(later["commitSha"], HEAD, "{later}");
        for key in [
            "message",
            "author",
            "committer",
            "parentCommitShas",
            "additions",
            "deletions",
            "pullRequest",
            "warnings",
            "related",
        ] {
            assert!(
                later.get(key).is_none(),
                "a later page repeats {key}: {later}"
            );
        }
        // A later page costs its rows, not the commit's message again.
        assert!(later.to_string().len() < message.len(), "{later}");
    }
    let walked = pages
        .iter()
        .flat_map(|page| page["files"].as_array().expect("files").clone())
        .map(|row| {
            row.as_str()
                .expect("file row")
                .rsplit(' ')
                .next()
                .expect("path")
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(walked, names, "every file exactly once, in order");
}

/// A commit's file inventory flags a file GitHub sent without a patch, as
/// a pull request's does: a binary file's `+0 -0` never reads as an empty
/// change (react v18.2.0...v19.0.0: favicon.ico, graphvizlib.wasm).
#[tokio::test]
async fn commit_inventory_flags_files_without_a_patch() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "Add assets");
    raw["files"] = json_fixture(
        r#"[
            {
                "filename": "public/favicon.ico",
                "status": "added",
                "additions": 0,
                "deletions": 0,
                "sha": "4444444444444444444444444444444444444444"
            },
            {
                "filename": "src/old.rs",
                "status": "renamed",
                "additions": 0,
                "deletions": 0,
                "previous_filename": "src/older.rs",
                "sha": "5555555555555555555555555555555555555555"
            }
        ]"#,
    );
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,"sections":["files"]}),
    )
    .await;
    assert_eq!(
        out["files"],
        json_fixture(
            r#"[
                "A +0 -0 !binary public/favicon.ico",
                "R +0 -0 src/old.rs <- src/older.rs"
            ]"#
        ),
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
            &octocode_runtime_base::security::ContentSecurity::new(),
        )
        .await
        .expect("history item")
        .data;
        let shown = out["files"].to_string();
        assert!(shown.contains("needle"), "{out}");
        assert!(!shown.contains(&secret[8..28]), "gap {gap}: {out}");
    }
}

/// A dotenv file's patch is a credential file's text, which the default
/// policy withholds whole: the row keeps the path and shows no patch.
#[tokio::test]
async fn dotenv_patch_rows_are_withheld_and_keep_their_path() {
    let server = MockServer::builder().start().await;
    let patch = |n: u8| {
        format!(
            "@@ -24,4 +24,4 @@\n TLS_KEYSTORE_FILE=shepherdKeystore.p12\n TLS_KEYSTORE_PASS=CowSaysMoo{n}\n-DB_PASS=OldMoo{n}\n+DB_PASS=NewMoo{n}\n DB_HOST=db{n}"
        )
    };
    let mut raw = commit_raw(HEAD, "dotenv patch");
    raw["files"] = json!([
        {"filename":".env","status":"modified","additions":1,"deletions":1,"patch":patch(1)},
        {"filename":"config/prod.env","status":"modified","additions":1,"deletions":1,"patch":patch(2)}
    ]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    // The production scanner, not the anchor reads' passthrough double.
    let provider = mock_provider(&server, RetryPolicy::default());
    let context = fixture_context(Duration::from_secs(10), 4 << 20);
    let request = HistoryItemRequest::from_row(
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,"sections":["patches"]}),
    )
    .expect("valid row");
    let out = execute(
        &provider,
        &request,
        &context,
        &octocode_runtime_base::security::ContentSecurity::new(),
    )
    .await
    .expect("history item")
    .data;
    // A credential-named file's patch is withheld whole by default, its
    // path kept: the row says nothing of its text.
    let served = out.to_string();
    for secret in [
        "CowSaysMoo",
        "OldMoo",
        "NewMoo",
        "shepherdKeystore.p12",
        "DB_HOST=db",
    ] {
        assert!(!served.contains(secret), "{secret} leaked: {out}");
    }
    for (at, name) in [".env", "config/prod.env"].iter().enumerate() {
        assert_eq!(out["files"][at]["path"], *name, "{out}");
        assert!(out["files"][at].get("patch").is_none(), "{out}");
    }
}

/// A commit states the pull request its message names (`pullRequest`) and
/// carries no call for it; with none named, GitHub's pull-request search by
/// the commit's SHA finds it (`findPullRequest`), never both.
#[tokio::test]
async fn a_commit_states_its_pull_request_or_carries_its_sha_search() {
    for (message, named) in [("Fix parser (#12)", true), ("Fix parser", false)] {
        let server = MockServer::builder().start().await;
        let mut raw = commit_raw(HEAD, message);
        raw["parents"] = json!([{"sha": BASE}]);
        raw["files"] = json!([changed_file("src/a.rs")]);
        mount_json(
            &server,
            format!("/api/v3/repos/o/r/commits/{HEAD}"),
            200,
            raw,
        )
        .await;
        let row = json!({"operation": "commit", "owner": "o", "repo": "r", "ref": HEAD});
        let out = read(&server, row.clone()).await;
        let pull_request_read = checks::find_related_call(&out, "ghGetHistoryItem", |input| {
            input["queries"][0]["operation"] == "pullRequest"
        });
        let sha_search = checks::find_related_call(&out, "ghSearchHistory", |input| {
            input["queries"][0]["keywords"] == json!([HEAD])
        });
        assert!(pull_request_read.is_none(), "{out}");
        assert_eq!(
            out.get("pullRequest").cloned(),
            named.then(|| json!(12)),
            "{out}"
        );
        assert_eq!(sha_search.is_some(), !named, "{out}");
        if let Some(search) = sha_search {
            let row = &search["input"]["queries"][0];
            assert_eq!(row["operation"], "pullRequest", "{out}");
            assert_eq!((&row["owner"], &row["repo"]), (&json!("o"), &json!("r")));
        }
        // The search is required by a warning, so the served row keeps it:
        // the shared guidance pass drops an advisory related call once a
        // row has evidence.
        let mut served = json!({"results": [{"index": 0, "data": out.clone()}]});
        octocode_runtime_base::response::workflow_lead::retain_guidance(
            ToolId::GhGetHistoryItem,
            &mut served,
            &json!({"queries": [row]}),
        );
        let kept =
            checks::find_related_call(&served["results"][0]["data"], "ghSearchHistory", |_| true);
        assert_eq!(kept.is_some(), !named, "{served}");
    }
}

/// A message can mention a pull request in prose ("submitted as part of PR
/// #2483") without stating the one that holds the commit: the SHA search
/// stays required, and its warning claims only what was checked, never that
/// the message names no pull request.
#[tokio::test]
async fn a_pull_request_mentioned_in_prose_is_not_denied_by_the_warning() {
    let server = MockServer::builder().start().await;
    let message = "printer: add hyperlinks\n\nThis commit was submitted as part of PR #2483.";
    let mut raw = commit_raw(HEAD, message);
    raw["parents"] = json!([{"sha": BASE}]);
    raw["files"] = json!([changed_file("src/a.rs")]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let out = read(
        &server,
        json!({"operation": "commit", "owner": "o", "repo": "r", "ref": HEAD}),
    )
    .await;
    assert!(
        checks::find_related_call(&out, "ghSearchHistory", |input| {
            input["queries"][0]["keywords"] == json!([HEAD])
        })
        .is_some(),
        "{out}"
    );
    let warnings = out["warnings"].as_array().expect("warnings");
    assert!(!warnings.is_empty(), "{out}");
    for warning in warnings.iter().filter_map(Value::as_str) {
        assert!(
            !warning.contains("names no pull request"),
            "the message names #2483: {warning}"
        );
    }
    // The mention is named, so the reader need not guess what it is.
    assert!(
        warnings
            .iter()
            .filter_map(Value::as_str)
            .any(|warning| warning.contains("#2483")),
        "{out}"
    );
}
