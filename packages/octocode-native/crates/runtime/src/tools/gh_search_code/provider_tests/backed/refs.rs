//! Ref searches: hits verified at the ref, the index commit disclosed once, rows missing at the ref.
use super::*;

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
