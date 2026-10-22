//! ghSearchCode against the mock server: hit rows, ref reads, partial and empty pages.
use crate::tools::gh_shared::test_support::backed::*;

/// A renamed repository is followed, as ghSearchHistory does: the same
/// search (every filter) runs against the canonical name and says so,
/// instead of an empty, partial row with a retry lead.
#[tokio::test]
async fn renamed_repository_is_followed_with_every_filter() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(query_param_contains("q", "repo:c/d"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"lib.rs","path":"src/lib.rs","sha":"1","html_url":"https://x",
                 "repository":{"full_name":"c/d","html_url":"https://x","url":"https://x"},
                 "text_matches":[{"fragment":"fn needle() {}","matches":[{"text":"needle","indices":[3,9]}]}]}
            ]})))
        .mount(&server)
        .await;
    empty_code_search(&server, true).await;
    renamed_to_c_d(&server).await;
    let out = run(
            &server,
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],
                   "extensions":["rs"],"path":"src","pageSize":10}),
        )
        .await
        .expect("search");
    let data = &out.data;
    assert_ne!(out.status, Some("empty"), "{data}");
    assert_eq!(data["files"][0]["path"], "src/lib.rs", "{data}");
    assert!(data["next"].get("retryRenamed").is_none(), "{data}");
    assert!(data["next"].get("retry").is_none(), "{data}");
    assert!(data.get("isPartial").is_none(), "{data}");
    assert_eq!(
        data["warnings"][0],
        crate::tools::gh_shared::renamed_warning("a", "b", &("c".into(), "d".into())),
        "one shared rename wording: {data}"
    );
    let searched = server
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|request| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == "q")
                .map(|(_, q)| q.into_owned())
        })
        .filter(|q| q.contains("repo:c/d"))
        .collect::<Vec<_>>();
    assert_eq!(searched.len(), 1, "{searched:?}");
    for filter in ["extension:rs", "path:src", "needle"] {
        assert!(searched[0].contains(filter), "{filter}: {searched:?}");
    }
}

/// GitHub answers a code search of a renamed repository's old name with a
/// 422 "cannot be searched" too: that answer is followed like an empty one.
#[tokio::test]
async fn an_unsearchable_renamed_repository_is_followed() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(query_param_contains("q", "repo:c/d"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "total_count":1,"incomplete_results":false,"items":[
            {"name":"lib.rs","path":"src/lib.rs","sha":"1","html_url":"https://x",
             "repository":{"full_name":"c/d","html_url":"https://x","url":"https://x"}}
        ]})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(query_param_contains("q", "repo:a/b"))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({
            "message": "Validation Failed",
            "errors": [{"message": "The listed users and repositories cannot be searched either because the resources do not exist or you do not have permission to view them.",
                "resource": "Search", "field": "q", "code": "invalid"}]
        })))
        .mount(&server)
        .await;
    renamed_to_c_d(&server).await;
    let out = run(
        &server,
        json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
               "keywords":["needle"],"matchIn":"path"}),
    )
    .await
    .expect("search");
    assert_eq!(out.data["files"][0]["path"], "src/lib.rs", "{}", out.data);
    assert_eq!(
        out.data["warnings"][0],
        crate::tools::gh_shared::renamed_warning("a", "b", &("c".into(), "d".into())),
        "{}",
        out.data
    );
}

/// An inaccessible repository gets no `retry` for its incomplete
/// result (a retry cannot help) and says why the search is empty.
#[tokio::test]
async fn inaccessible_repository_drops_the_incomplete_retry() {
    let server = MockServer::builder().start().await;
    empty_code_search(&server, true).await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        404,
        json!({"message":"Not Found"}),
    )
    .await;
    // A missing repository is a not-found failure (never an empty,
    // partial search a retry could fix) that names repository access.
    let error = run(
            &server,
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"]}),
        )
        .await
        .expect_err("missing repository");
    assert_eq!(error.kind, ProviderErrorKind::NotFound);
    assert_eq!(
        error.reason,
        Some(octocode_github::ProviderErrorReason::RepositoryNotFound)
    );
    let failure = crate::tools::gh_shared::search_failure(error, None);
    assert!(failure.message.contains("private"), "{}", failure.message);
}

/// Repo-scoped hits list every keyword line of the top files with its
/// line number (`grep -n` over the fetched blob), name owner/repo
/// once, and read the top hit by line range.
#[tokio::test]
async fn repo_scoped_hits_list_every_occurrence_with_line_numbers() {
    let server = MockServer::builder().start().await;
    mount_code_search(&server).await;
    mount_ref(&server, "HEAD").await;
    mount_content(
        &server,
        "src/handler.py",
        "import x\n\ndef wrap_app(app):\n    return app\n",
    )
    .await;
    mount_content(
            &server,
            "src/routing.py",
            "from h import wrap_app\n\nasync def a():\n    await wrap_app(app)(scope)\n\nasync def b():\n    await wrap_app(s)(scope)\n",
        )
        .await;
    let out = run(
            &server,
            json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["wrap_app"]}),
        )
        .await
        .expect("search");
    let data = &out.data;
    // A repo-scoped page echoes no owner/repo: they are the query's.
    assert!(data.get("owner").is_none(), "{data}");
    assert!(data.get("repo").is_none(), "{data}");
    assert_eq!(data["commitSha"], TREE_SHA, "{data}");
    let files = data["files"].as_array().expect("files");
    let routing = files
        .iter()
        .find(|row| row["path"] == "src/routing.py")
        .expect("routing row");
    assert_eq!(
        routing["lines"],
        json!([
            "1\tfrom h import wrap_app",
            "4\t    await wrap_app(app)(scope)",
            "7\t    await wrap_app(s)(scope)"
        ]),
        "{data}"
    );
    for row in files {
        assert!(
            row.get("owner").is_none() && row.get("repo").is_none(),
            "{row}"
        );
        assert!(row.get("matches").is_none(), "{row}");
    }
    let read = &data["next"]["read"]["query"]["queries"][0];
    assert_eq!(read["owner"], "a", "{data}");
    // The top hit declares `wrap_app`, so the read is that whole
    // declaration, never a fixed line window that may cut it.
    assert_eq!(read["path"], "src/handler.py", "{data}");
    assert_eq!(read["matchString"], "def wrap_app(app):", "{data}");
    assert_eq!(read["block"], true, "{data}");
    assert!(
        read.get("startLine").is_none() && read.get("endLine").is_none(),
        "{data}"
    );
    crate::contracts::validate_query("ghGetFileContent", {
        let mut query = read.clone();
        query["mainGoal"] = json!("g");
        query
    })
    .expect("read is a valid ghGetFileContent query");
}

/// A file whose hits are capped or whose hit lines are cut carries an
/// executable read of every keyword line at the resolved commit, so
/// no hit stays unreachable; a fully shown file gets none.
#[tokio::test]
async fn capped_or_clipped_hit_files_carry_a_read_of_every_hit() {
    let server = MockServer::builder().start().await;
    mount_code_search(&server).await;
    mount_ref(&server, "HEAD").await;
    mount_content(
        &server,
        "src/handler.py",
        &format!("x = '{}' + wrap_app\n", "a".repeat(400)),
    )
    .await;
    mount_content(&server, "src/routing.py", &"wrap_app()\n".repeat(25)).await;
    let out = run(
            &server,
            json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["wrap_app"]}),
        )
        .await
        .expect("search");
    let data = &out.data;
    let reads: Vec<&Value> = ["readHits", "readHits2"]
        .iter()
        .filter_map(|key| data["next"].get(*key))
        .collect();
    assert_eq!(reads.len(), 2, "{data}");
    assert!(data["next"].get("readHits3").is_none(), "{data}");
    let mut paths: Vec<&str> = reads
        .iter()
        .map(|read| read["query"]["queries"][0]["path"].as_str().expect("path"))
        .collect();
    paths.sort_unstable();
    assert_eq!(paths, ["src/handler.py", "src/routing.py"], "{data}");
    for read in reads {
        assert_eq!(read["tool"], "ghGetFileContent", "{data}");
        let query = &read["query"]["queries"][0];
        assert_eq!(query["ref"], TREE_SHA, "{data}");
        assert_eq!(query["matchString"], "wrap_app", "{data}");
        assert_eq!(query["contextLines"], 0, "{data}");
        // A file whose first five hits are shown resumes after them
        // (lines 6-25 hold the other twenty); a file whose shown line
        // was cut re-reads its hits whole.
        match query["path"].as_str().expect("path") {
            "src/routing.py" => assert_eq!(query["ranges"], json!(["6-25"]), "{data}"),
            _ => assert!(query.get("ranges").is_none(), "{data}"),
        }
        crate::contracts::validate_query("ghGetFileContent", {
            let mut query = query.clone();
            query["mainGoal"] = json!("g");
            query
        })
        .expect("readHits is a valid ghGetFileContent query");
    }
}

/// The listing matches keywords case-insensitively, so the hit read
/// does too: an uppercase keyword under smart case would skip counted
/// lowercase hits.
#[tokio::test]
async fn hit_reads_match_case_like_the_listing() {
    let server = MockServer::builder().start().await;
    mount_code_search(&server).await;
    mount_ref(&server, "HEAD").await;
    mount_content(&server, "src/handler.py", "def wrap_app(app):\n").await;
    mount_content(&server, "src/routing.py", &"wrap_app()\n".repeat(25)).await;
    let out = run(
            &server,
            json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["Wrap_App"]}),
        )
        .await
        .expect("search");
    let query = &out.data["next"]["readHits"]["query"]["queries"][0];
    assert_eq!(query["path"], "src/routing.py", "{}", out.data);
    assert_eq!(query["caseMode"], "insensitive", "{}", out.data);
}

/// A fully shown hit list needs no extra read.
#[tokio::test]
async fn fully_shown_hit_files_get_no_hit_read() {
    let server = MockServer::builder().start().await;
    mount_code_search(&server).await;
    mount_ref(&server, "HEAD").await;
    mount_content(&server, "src/handler.py", "def wrap_app(app):\n").await;
    mount_content(&server, "src/routing.py", "wrap_app()\n").await;
    let out = run(
            &server,
            json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["wrap_app"]}),
        )
        .await
        .expect("search");
    assert!(out.data["next"].get("readHits").is_none(), "{}", out.data);
}

/// Owner-wide searches spend no contents quota: rows keep their
/// owner/repo, and fragment offsets stay out of default output.
#[tokio::test]
async fn owner_wide_hits_keep_fragments_without_offsets() {
    let server = MockServer::builder().start().await;
    mount_code_search(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(TREE_SHA))
        .expect(0)
        .mount(&server)
        .await;
    let out = run(
            &server,
            json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","keywords":["wrap_app"]}),
        )
        .await
        .expect("search");
    // Every row is in a/b: the page names the repository once.
    let row = &out.data["files"][0];
    assert_eq!(out.data["owner"], "a", "{}", out.data);
    assert_eq!(out.data["repo"], "b", "{}", out.data);
    assert!(row.get("owner").is_none(), "{}", out.data);
    assert!(row["matches"][0]["value"].is_string(), "{}", out.data);
    // Offsets are a diagnostics extra (core field class): the default
    // response drops them, `extras:["diagnostics"]` keeps them.
    assert!(
        crate::response::extras::all_paths(crate::id::ToolId::GhSearchCode)
            .contains(&"results[].data.files[].matches[].matchIndices"),
        "{}",
        out.data
    );
    assert!(out.data.get("commitSha").is_none(), "{}", out.data);
    // The diagnostics extra keeps the offsets.
    let diagnostics = run(
            &server,
            json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","keywords":["wrap_app"],"extras":["diagnostics"]}),
        )
        .await
        .expect("search");
    assert!(
        diagnostics.data["files"][0]["matches"][0]["matchIndices"].is_array(),
        "{}",
        diagnostics.data
    );
}

/// `branch` verifies the top files at that ref: lines come from the
/// ref, a path absent there is flagged, and the page says its
/// candidates came from the default-branch index.
#[tokio::test]
async fn branch_hits_are_verified_at_the_ref_and_labeled() {
    let server = MockServer::builder().start().await;
    mount_code_search(&server).await;
    mount_ref(&server, "dev").await;
    mount_content(&server, "src/handler.py", "def wrap_app(app):\n").await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src%2Frouting.py",
        404,
        json!({"message":"Not Found"}),
    )
    .await;
    let out = run(
            &server,
            json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["wrap_app"],"ref":"dev"}),
        )
        .await
        .expect("search");
    let data = &out.data;
    // The caller's own ref is not echoed back.
    assert!(data.get("ref").is_none(), "{data}");
    assert!(data.get("indexRef").is_none(), "{data}");
    let files = data["files"].as_array().expect("files");
    let handler = files
        .iter()
        .find(|row| row["path"] == "src/handler.py")
        .expect("handler");
    assert_eq!(handler["lines"], json!(["1\tdef wrap_app(app):"]), "{data}");
    let routing = files
        .iter()
        .find(|row| row["path"] == "src/routing.py")
        .expect("routing");
    assert_eq!(routing["atRef"], false, "{data}");
    assert!(
        routing.get("matches").is_none(),
        "default-branch text is not shown as the ref: {data}"
    );
    assert_eq!(
        data["next"]["read"]["query"]["queries"][0]["ref"], TREE_SHA,
        "the read is pinned to the commit the lines came from: {data}"
    );
}

/// A search pinned to a non-default ref warns that its files come
/// from the default-branch index at the index commit and leads to
/// the ref's own listing first; a ref that is the default-branch head
/// adds neither.
#[tokio::test]
async fn a_non_default_ref_discloses_the_index_commit_and_leads_to_the_ref() {
    const HEAD_SHA: &str = "fedcba9876543210fedcba9876543210fedcba98";
    let server = MockServer::builder().start().await;
    mount_code_search(&server).await;
    mount_ref(&server, "dev").await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(HEAD_SHA))
        .mount(&server)
        .await;
    mount_content(&server, "src/handler.py", "def wrap_app(app):\n").await;
    mount_content(&server, "src/routing.py", "await wrap_app(app)\n").await;
    let out = run(
        &server,
        json!({"operation":"code","owner":"a","repo":"b","keywords":["wrap_app"],
                "path":"src","ref":"dev"}),
    )
    .await
    .expect("search");
    let data = &out.data;
    // Facts only: the static "files only at the ref are missing"
    // advice is the description's; `viewRepo` lists them.
    assert_eq!(
        data["warnings"],
        json!(["Files from the default-branch index at fedcba9, not dev; `lines` read at dev."]),
        "{data}"
    );
    let lead = &data["next"]["viewRepo"];
    assert_eq!(lead["tool"], "ghStructure", "{data}");
    for (key, value) in [
        ("owner", "a"),
        ("repo", "b"),
        ("path", "src"),
        ("ref", TREE_SHA),
    ] {
        assert_eq!(lead["query"]["queries"][0][key], value, "{data}");
    }
    assert_eq!(
        data["next"]
            .as_object()
            .and_then(|next| next.keys().next())
            .map(String::as_str),
        Some("viewRepo"),
        "the ref's listing is the first lead: {data}"
    );

    let default_head = MockServer::builder().start().await;
    mount_code_search(&default_head).await;
    mount_ref(&default_head, "main").await;
    mount_ref(&default_head, "HEAD").await;
    mount_content(&default_head, "src/handler.py", "def wrap_app(app):\n").await;
    mount_content(&default_head, "src/routing.py", "await wrap_app(app)\n").await;
    let out = run(
        &default_head,
        json!({"operation":"code","owner":"a","repo":"b","keywords":["wrap_app"],
                "ref":"main"}),
    )
    .await
    .expect("search");
    assert!(out.data.get("warnings").is_none(), "{}", out.data);
    assert!(out.data.pointer("/next/viewRepo").is_none(), "{}", out.data);
}

/// A file the index lists but the requested ref lacks offers no
/// read: the fragment came from the default branch, and reading it
/// there would cross the requested scope.
#[tokio::test]
async fn a_file_missing_at_the_requested_ref_offers_no_default_branch_read() {
    let server = MockServer::builder().start().await;
    app_rs_hit(&server).await;
    mount_ref(&server, "dev").await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/app.rs",
        404,
        json!({"message":"Not Found"}),
    )
    .await;
    let out = run(
        &server,
        json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                "keywords":["wrap_app"],"ref":"dev"}),
    )
    .await
    .expect("search");
    let data = &out.data;
    assert_eq!(data["files"][0]["atRef"], false, "{data}");
    assert!(
        data.pointer("/next/read").is_none(),
        "no read outside the requested ref: {data}"
    );
    // The ref listing searches the moved file's name tree-wide.
    let listing = &data["next"]["viewRepo"]["query"]["queries"][0];
    assert_eq!(listing["include"], json!(["app.rs"]), "{data}");
    assert_eq!(listing["ref"], TREE_SHA, "{data}");
    assert!(listing.get("path").is_none(), "{data}");

    // The file exists at the ref but no line holds the keyword.
    let unmatched = MockServer::builder().start().await;
    app_rs_hit(&unmatched).await;
    mount_ref(&unmatched, "dev").await;
    mount_content(&unmatched, "app.rs", "fn renamed() {}\n").await;
    let out = run(
        &unmatched,
        json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                "keywords":["wrap_app"],"ref":"dev"}),
    )
    .await
    .expect("search");
    // The file exists at the ref but no line holds the keyword: the
    // hit is not at the ref, and the default-branch fragment is not
    // shown as the ref's text.
    let row = &out.data["files"][0];
    assert_eq!(row["atRef"], false, "{}", out.data);
    assert!(row.get("matches").is_none(), "{}", out.data);
    assert!(out.data.pointer("/next/read").is_none(), "{}", out.data);
}

/// A default-branch page reads every row's hit lines, not only the
/// top files: no row is a fragment without line numbers.
#[tokio::test]
async fn every_row_of_a_default_branch_page_has_numbered_lines() {
    let server = MockServer::builder().start().await;
    let names = ["a", "b", "c", "d", "e", "f", "g"];
    wrap_app_hits(&server, &names).await;
    mount_ref(&server, "HEAD").await;
    // Distinct bodies: rows with identical evidence would list once.
    for name in names {
        mount_content(
            &server,
            &format!("src/{name}.rs"),
            &format!("fn wrap_app() {{}} // {name}\n"),
        )
        .await;
    }
    let out = run(
        &server,
        json!({"operation":"code","owner":"a","repo":"b","keywords":["wrap_app"]}),
    )
    .await
    .expect("search");
    let files = out.data["files"].as_array().expect("files");
    assert_eq!(files.len(), names.len(), "{}", out.data);
    for (row, name) in files.iter().zip(names) {
        assert_eq!(
            row["lines"],
            json!([format!("1\tfn wrap_app() {{}} // {name}")]),
            "{row}"
        );
        assert!(row.get("matches").is_none(), "{row}");
    }
}

/// With `branch`, every row of the page is read at the ref, not only
/// the top files: each has numbered `lines`, or `atRef:false` (the
/// hit is not at the ref), or `lineResolved:false` with a read of
/// its keyword lines at the ref. No row shows default-branch index
/// text as the ref's.
#[tokio::test]
async fn every_row_of_a_ref_page_is_read_at_the_ref() {
    let server = MockServer::builder().start().await;
    let names = ["a", "b", "c", "d", "e", "f", "g", "h"];
    wrap_app_hits(&server, &names).await;
    mount_ref(&server, "dev").await;
    for name in &names[..6] {
        mount_content(
            &server,
            &format!("src/{name}.rs"),
            &format!("\nfn wrap_app() {{}} // {name}\n"),
        )
        .await;
    }
    mount_content(&server, "src/g.rs", "fn renamed() {}\n").await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Fh.rs"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let out = run(
        &server,
        json!({"operation":"code","owner":"a","repo":"b",
                "keywords":["wrap_app"],"ref":"dev"}),
    )
    .await
    .expect("search");
    let data = &out.data;
    let files = data["files"].as_array().expect("files");
    assert_eq!(files.len(), names.len(), "{data}");
    for row in files {
        assert!(row.get("matches").is_none(), "{row}");
    }
    for (row, name) in files[..6].iter().zip(names) {
        assert_eq!(
            row["lines"],
            json!([format!("2\tfn wrap_app() {{}} // {name}")]),
            "{row}"
        );
    }
    assert_eq!(files[6]["atRef"], false, "{data}");
    assert_eq!(files[7]["lineResolved"], false, "{data}");
    let reads = data["next"]
        .as_object()
        .expect("next")
        .values()
        .filter(|read| read["query"]["queries"][0]["path"] == "src/h.rs")
        .collect::<Vec<_>>();
    assert_eq!(reads.len(), 1, "{data}");
    assert_eq!(reads[0]["query"]["queries"][0]["ref"], TREE_SHA, "{data}");
}

/// A non-empty page GitHub marks incomplete says so in its data,
/// without extras, separately from whether another page exists.
#[tokio::test]
async fn a_non_empty_incomplete_page_reports_partial_coverage() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/code",
        200,
        json!({
            "total_count":1,"incomplete_results":true,"items":[
                {"name":"app.rs","path":"app.rs","sha":"1","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                 "text_matches":[]}
            ]
        }),
    )
    .await;
    let out = run(
        &server,
        json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a",
                "keywords":["wrap_app"],"matchIn":"path"}),
    )
    .await
    .expect("search");
    let data = &out.data;
    assert_eq!(data["isPartial"], true, "{data}");
    assert_eq!(
        data["partialReasons"],
        json!(["providerIncompleteResults"]),
        "{data}"
    );
    assert!(data.pointer("/pagination/hasMore").is_none(), "{data}");
    assert_eq!(
        data["next"]["retry"]["query"]["queries"][0]["page"], 1,
        "{data}"
    );
    assert!(out.diagnostics.partial);
}

/// Empty searches name the default-branch index only when a branch
/// was requested; otherwise the hint names the scope that came up empty.
#[tokio::test]
async fn empty_code_search_names_the_default_branch_index() {
    let server = MockServer::builder().start().await;
    empty_code_search(&server, false).await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json!({"default_branch":"main","full_name":"a/b"}),
    )
    .await;
    for (query, cause) in [
        (
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],"ref":"dev"}),
            "default branch",
        ),
        (
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"]}),
            "in a/b",
        ),
        (
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","keywords":["needle"]}),
            "any a repository",
        ),
        (
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],"extensions":["rs"]}),
            "extensions",
        ),
        (
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],"matchIn":"path"}),
            "path",
        ),
    ] {
        let out = run(&server, query).await.expect("search");
        assert_eq!(out.status, Some("empty"));
        // Only a requested ref is a per-call fact; every other cause is
        // the row's leads and the description.
        if cause == "default branch" {
            let hint = out.data["hints"][0].as_str().unwrap_or_default();
            assert!(hint.contains("`dev` was not searched"), "{}", out.data);
            assert!(hint.len() <= 120, "{hint}");
        } else {
            assert!(out.data.get("hints").is_none(), "{cause}: {}", out.data);
        }
    }
}

/// Empty-search hints name the actual cause, and a repository
/// known to exist gets no root viewTree.
#[tokio::test]
async fn empty_code_search_hints_are_cause_specific() {
    let server = MockServer::builder().start().await;
    empty_code_search(&server, false).await;
    for (repo, full_name) in [("a/b", "a/b"), ("a/old", "c/d"), ("c/d", "c/d")] {
        mount_json(
            &server,
            format!("/api/v3/repos/{repo}"),
            200,
            json!({"default_branch":"main","full_name":full_name}),
        )
        .await;
    }
    let renamed = run(
            &server,
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"old","keywords":["needle"]}),
        )
        .await
        .expect("search");
    // The rename is followed: the warning names it, and the empty
    // answer is about the canonical repository.
    assert_eq!(
        renamed.data["warnings"],
        json!(["Repository a/old was renamed to c/d; leads use the canonical name."]),
        "{}",
        renamed.data
    );
    assert!(renamed.data.get("hints").is_none(), "{}", renamed.data);
    assert!(
        renamed.data["next"].get("retryRenamed").is_none(),
        "{}",
        renamed.data
    );

    let existing = run(
            &server,
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"]}),
        )
        .await
        .expect("search");
    // Beside the clone lead (dropped where clone is unavailable), the
    // root listing is a route every surface runs.
    assert_eq!(
        existing.data["next"]["viewTree"]["query"]["queries"][0],
        json!({"owner":"a","repo":"b"}),
        "{}",
        existing.data
    );
    assert!(
        existing.data["next"].get("cloneRepo").is_some(),
        "{}",
        existing.data
    );
    let scoped = run(
            &server,
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],"path":"src/io"}),
        )
        .await
        .expect("search");
    assert_eq!(
        scoped.data["next"]["viewTree"]["query"]["queries"][0]["path"], "src/io",
        "{}",
        scoped.data
    );

    let path_mode = run(
        &server,
        json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b",
                   "keywords":["fn spawn_blocking"],"matchIn":"path"}),
    )
    .await
    .expect("search");
    // The lead is the recovery; no sentence restates it.
    assert!(path_mode.data.get("hints").is_none(), "{}", path_mode.data);
    let content = &path_mode.data["next"]["findCode"]["query"]["queries"][0];
    assert_eq!(content["matchIn"], "file", "{}", path_mode.data);
    assert_eq!(content["keywords"], json!(["fn spawn_blocking"]));
}

/// A zero-hit recovery for a requested branch inspects that branch,
/// not the default branch the index covers.
#[tokio::test]
async fn empty_scoped_recovery_keeps_the_requested_branch() {
    let server = MockServer::builder().start().await;
    empty_code_search(&server, false).await;
    for (repo, archived) in [("b", false), ("old", true)] {
        mount_json(
            &server,
            format!("/api/v3/repos/a/{repo}"),
            200,
            json!({"default_branch":"main","full_name":format!("a/{repo}"),"archived":archived}),
        )
        .await;
    }
    for repo in ["b", "old"] {
        let out = run(
            &server,
            json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":repo,
                       "keywords":["needle"],"path":"src","ref":"nondefault"}),
        )
        .await
        .expect("search");
        let view = &out.data["next"]["viewTree"]["query"]["queries"][0];
        assert_eq!(view["ref"], "nondefault", "{}", out.data);
    }
    let default = run(
        &server,
        json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                   "keywords":["needle"],"path":"src"}),
    )
    .await
    .expect("search");
    assert!(
        default.data["next"]["viewTree"]["query"]["queries"][0]
            .get("ref")
            .is_none(),
        "{}",
        default.data
    );
}

/// Path matches carry no empty snippet list.
#[tokio::test]
async fn path_match_rows_omit_empty_matches() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/code",
        200,
        json!({
            "total_count":1,"incomplete_results":false,"items":[{
                "name":"pool.rs","path":"src/pool.rs","sha":"1","html_url":"https://x",
                "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}
            }]
        }),
    )
    .await;
    let out = run(
        &server,
        json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b",
                   "keywords":["pool"],"matchIn":"path"}),
    )
    .await
    .expect("search");
    let row = &out.data["files"][0];
    assert_eq!(row["path"], "src/pool.rs", "{}", out.data);
    assert!(row.get("matches").is_none(), "{}", out.data);
}

#[tokio::test]
async fn capped_search_is_terminal_only_on_the_last_reachable_page() {
    let server = MockServer::builder().start().await;
    let item = json!({"name":"a.rs","path":"a.rs","sha":"s","html_url":"h",
            "repository":{"full_name":"o/r","html_url":"h","url":"u"}});
    mount_json(
        &server,
        "/api/v3/search/code",
        200,
        json!({
            "total_count":5000,"incomplete_results":false,"items":[item]
        }),
    )
    .await;
    let first = run(
            &server,
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"o","keywords":["x"],"page":1,"pageSize":100}),
        )
        .await
        .expect("page 1");
    assert!(first.data["next"]["nextPage"].is_object());
    assert!(first.data.get("terminalLimit").is_none(), "{}", first.data);
    let last = run(
            &server,
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"o","keywords":["x"],"page":10,"pageSize":100}),
        )
        .await
        .expect("page 10");
    assert_eq!(last.data["terminalLimit"], true, "{}", last.data);
    assert!(last.data["next"].get("nextPage").is_none());
}

/// Several `extensions` are one call for the agent: one indexed search
/// per extension (GitHub ANDs repeated qualifiers), merged into one page.
/// The searches share `pageSize`, so the page does not grow with them.
#[tokio::test]
async fn several_extensions_share_one_page() {
    let server = MockServer::builder().start().await;
    for (extension, file) in [("ts", "a.ts"), ("js", "b.js")] {
        Mock::given(method("GET"))
            .and(path("/api/v3/search/code"))
            .and(wiremock::matchers::query_param_contains(
                "q",
                format!("extension:{extension}"),
            ))
            .and(wiremock::matchers::query_param("per_page", "5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count":1,"incomplete_results":false,"items":[code_item(file)]
            })))
            .expect(1)
            .mount(&server)
            .await;
    }
    let out = run(
        &server,
        json!({"operation":"code","owner":"a","keywords":["needle"],"extensions":["ts","js"],"pageSize":10}),
    )
    .await
    .expect("search");
    // Both extensions' rows carry the same fragment: listed once, with
    // the other path in `alsoAt`.
    let paths = out.data["files"]
        .as_array()
        .expect("files")
        .iter()
        .flat_map(|row| {
            std::iter::once(&row["path"]).chain(row["alsoAt"].as_array().into_iter().flatten())
        })
        .map(|path| path.as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["a.ts", "b.js"], "{}", out.data);
}

/// A ref page discloses the default-branch index once (page 1); later
/// pages carry neither the warning nor its HEAD lookup.
#[tokio::test]
async fn the_index_warning_is_stated_on_the_first_ref_page_only() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/code",
        200,
        json!({
            "total_count":40,"incomplete_results":false,"items":[code_item("a.rs")]
        }),
    )
    .await;
    mount_ref(&server, "dev").await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string("f".repeat(40)))
        .expect(1)
        .mount(&server)
        .await;
    mount_content(&server, "a.rs", "needle\n").await;
    for page in [1, 2] {
        let out = run(
            &server,
            json!({"operation":"code","owner":"a","repo":"b","keywords":["needle"],
                "ref":"dev","page":page,"pageSize":10}),
        )
        .await
        .expect("search");
        let warned = out.data["warnings"]
            .to_string()
            .contains("default-branch index");
        assert_eq!(warned, page == 1, "page {page}: {}", out.data);
    }
}

/// An empty search of an existing repository leads to its structure:
/// the index may lag, so verify the path before concluding absence.
#[tokio::test]
async fn an_empty_scoped_search_leads_to_its_structure() {
    let server = MockServer::builder().start().await;
    empty_code_search(&server, false).await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json!({"default_branch":"main","full_name":"a/b"}),
    )
    .await;
    let out = run(
        &server,
        json!({"operation":"code","owner":"a","repo":"b","keywords":["absent"],"path":"src"}),
    )
    .await
    .expect("search");
    assert_eq!(out.status, Some("empty"));
    let lead = &out.data["next"]["viewTree"];
    assert_eq!(lead["tool"], "ghStructure", "{}", out.data);
    assert_eq!(lead["query"]["queries"][0]["repo"], "b", "{}", out.data);
    assert_eq!(lead["query"]["queries"][0]["path"], "src", "{}", out.data);
}
