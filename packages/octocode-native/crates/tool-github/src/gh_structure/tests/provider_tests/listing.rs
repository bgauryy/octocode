//! Tree listings: normalized paths, row anchors, truncated trees, the contents cap and non-directory paths.
use super::*;
use octocode_test_support::json_fixture;

/// A listing path is read as the repository names it (`./`, `//`
/// and `a/../` resolved), and its leads name that path; a path that
/// climbs above the root is invalid input, before any request.
#[tokio::test]
async fn listing_paths_are_normalized_and_escapes_rejected() {
    let server = server().await;
    let entries =
        json_fixture(r#"[{"name": "a.rs", "path": "src/lib/a.rs", "type": "file", "size": 3}]"#);
    mount_folder_tree(&server, "/api/v3/repos/a/b", SHA, "src/lib", &entries).await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src%2Flib",
        200,
        entries,
    )
    .await;
    let home = tempfile::tempdir().expect("home");
    let provider = mock_provider(&server, RetryPolicy::default());
    let context = default_context();
    for requested in ["./src/lib", "src//lib/", "src/x/../lib"] {
        let out = run(
            &provider,
            &query(json!({"operation":"tree","owner":"a","repo":"b","path":requested})),
            Ok(&context),
            home.path(),
            None,
        )
        .await
        .unwrap_or_else(|failure| panic!("{requested}: {}", failure.message));
        assert_eq!(
            listed_paths(&out.data),
            ["src/lib/a.rs (3)"],
            "{requested}: {}",
            out.data
        );
    }
    let before = server.received_requests().await.unwrap_or_default().len();
    for escape in ["../..", "../../etc", "src/../../x"] {
        let failure = run(
            &provider,
            &query(json!({"operation":"tree","owner":"a","repo":"b","path":escape})),
            Ok(&context),
            home.path(),
            None,
        )
        .await
        .expect_err(escape);
        assert_eq!(
            failure.error.kind,
            ProviderErrorKind::Validation,
            "{escape}"
        );
    }
    let after = server.received_requests().await.unwrap_or_default().len();
    assert_eq!(before, after, "an escape sends no request");
}

/// The happy-path anchors: each `dir` with its entry names (the
/// next read's path), the resolved branch and its `commitSha` (the
/// next read's ref), with repository identity for saved results.
#[tokio::test]
async fn a_listing_row_carries_dir_entries_commit_and_resolved_ref() {
    let server = server().await;
    let entries = json_fixture(
        r#"[
            {"name": "lib.rs", "path": "lib.rs", "type": "file", "size": 3},
            {"name": "src", "path": "src", "type": "dir"}
        ]"#,
    );
    mount_folder_tree(&server, "/api/v3/repos/a/b", SHA, "", &entries).await;
    mount_json(&server, "/api/v3/repos/a/b/contents", 200, entries).await;
    let out = list(
        &server,
        json_fixture(r#"{"operation": "tree", "owner": "a", "repo": "b"}"#),
    )
    .await
    .expect("listing");
    let data = &out.data;
    assert_eq!(
        data["files"],
        json_fixture(r#"[{"dir": ".", "files": ["lib.rs (3)", "src/"]}]"#),
        "{data}"
    );
    assert_eq!(data["ref"], "main", "{data}");
    assert_eq!(data["commitSha"], SHA, "{data}");
    assert_eq!(data["owner"], "a", "{data}");
    assert_eq!(data["repo"], "b", "{data}");
    // `ref` is the branch the listing resolved (the default branch here),
    // not an echo: the request named none.
    for echo in ["path", "operation"] {
        assert!(data.get(echo).is_none(), "{echo} echoed: {data}");
    }
    assert!(data.get("related").is_none(), "{data}");
    let read =
        json!({"owner":data["owner"],"repo":data["repo"],"path":"lib.rs","ref":data["commitSha"]});
    octocode_runtime_base::contracts::validate_query("ghGetFileContent", read)
        .expect("listed source identity is sufficient");
}

/// A recursive tree past GitHub's limit is read subtree by subtree:
/// a folder's whole subtree comes from one recursive read by its
/// tree SHA (no folder-by-folder walk), so a bare-word `include`
/// finds names at any depth. Pages are disjoint, and each page's
/// continuation is the whole query pinned to the listed commit.
#[tokio::test]
async fn a_truncated_tree_is_read_by_subtree_and_pages_without_overlap() {
    let server = server().await;
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/git/trees/{SHA}"),
        200,
        json!({"sha":SHA,"truncated":true,"tree":[]}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents",
        200,
        json!([
            {"name":"README.md","path":"README.md","type":"file","size":1},
            {"name":"src","path":"src","type":"dir","sha":"d".repeat(40)}
        ]),
    )
    .await;
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/git/trees/{}", "d".repeat(40)),
        200,
        json!({"sha":"d".repeat(40),"truncated":false,"tree":[
            {"path":"a","type":"tree"},
            {"path":"a/b","type":"tree"},
            {"path":"a/b/handler.rs","type":"blob","size":5},
            {"path":"a/b/other.rs","type":"blob","size":5},
            {"path":"a/handler_test.rs","type":"blob","size":5}
        ]}),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(0)
        .mount(&server)
        .await;
    let base = json_fixture(
        r#"{
            "operation": "tree",
            "owner": "a",
            "repo": "b",
            "ref": "main",
            "include": ["**/*handler*.rs"]
        }"#,
    );
    let out = list(&server, base).await.expect("subtree listing");
    assert!(out.data.get("terminalLimit").is_none(), "{}", out.data);
    assert!(out.data["next"].get("nextPage").is_none(), "{}", out.data);
    assert_eq!(
        out.data["files"],
        json_fixture(r#"["src/a/handler_test.rs (5)", "src/a/b/handler.rs (5)"]"#)
    );
}

/// A recursive tree larger than one response may hold (Linux's root tree
/// is 17.6 MB) is a truncated tree, not a missing one: its folders are
/// read whole from their own recursive subtrees, never folder by folder.
#[tokio::test]
async fn an_oversized_recursive_tree_is_read_by_subtree() {
    let server = server().await;
    let limit = default_context().budget.max_body_bytes;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/trees/{SHA}")))
        .and(wiremock::matchers::query_param("recursive", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": SHA, "truncated": false, "tree": [], "padding": "x".repeat(limit + 1)
        })))
        .mount(&server)
        .await;
    let src = "d".repeat(40);
    mount_folder_tree(
        &server,
        "/api/v3/repos/a/b",
        SHA,
        "",
        &json!([
            {"name":"README.md","path":"README.md","type":"file","size":1},
            {"name":"src","path":"src","type":"dir","sha":src}
        ]),
    )
    .await;
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/git/trees/{src}"),
        200,
        json!({"sha":src,"truncated":false,"tree":[
            {"path":"a","type":"tree"},
            {"path":"a/handler.rs","type":"blob","size":5}
        ]}),
    )
    .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/trees/{SHA}:src")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(0)
        .mount(&server)
        .await;
    let row = json_fixture(
        r#"{"operation": "tree", "owner": "a", "repo": "b", "ref": "main", "include": ["**/*.rs"]}"#,
    );
    let data = list(&server, row).await.expect("listing").data;
    assert!(data.get("isPartial").is_none(), "{data}");
    assert_eq!(
        data["files"],
        json_fixture(r#"["src/a/handler.rs (5)"]"#),
        "{data}"
    );
}

/// No count of reads stops a walk: one the call's time pauses
/// ([`Walked::Paused`]) is resumed by the walk that replays its reads, each
/// resumed call gets at least one read further, and the walk that ends
/// lists every entry the unpaused walk lists, once.
#[tokio::test]
async fn a_paused_walk_resumes_to_every_entry_once() {
    use crate::gh_structure::traverse::{Listing, Pace, Walked, walk_contents};
    let server = server().await;
    let dir = |name: &str| json!({"name":name,"path":name,"type":"dir"});
    let file = |path: &str| json!({"path":path,"type":"file","size":1});
    mount_folder_tree(
        &server,
        "/api/v3/repos/a/b",
        SHA,
        "",
        &json!([dir("src"), dir("docs"), file("README.md")]),
    )
    .await;
    mount_folder_tree(
        &server,
        "/api/v3/repos/a/b",
        SHA,
        "src",
        &json!([dir("src/core"), file("src/lib.rs")]),
    )
    .await;
    mount_folder_tree(
        &server,
        "/api/v3/repos/a/b",
        SHA,
        "docs",
        &json!([file("docs/guide.md")]),
    )
    .await;
    mount_folder_tree(
        &server,
        "/api/v3/repos/a/b",
        SHA,
        "src/core",
        &json!([file("src/core/walk.rs")]),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let context = default_context();
    let listing = Listing {
        owner: "a",
        repo: "b",
        branch: SHA,
        root: "",
        max_depth: crate::gh_structure::traverse::max_listing_depth(),
    };
    let paths = |traversal: &Traversal| {
        let mut paths = traversal
            .entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        paths.sort();
        paths
    };
    let unpaused = Pace {
        resume_from: 0,
        stop_at: None,
    };
    let Walked::Done(whole) = walk_contents(&provider, &listing, false, unpaused, &context)
        .await
        .expect("whole walk")
    else {
        panic!("an unpaused walk ends");
    };
    let mut resume_from = 0;
    let mut calls = 0;
    let resumed = loop {
        // A stop already passed: each call makes the replayed reads and one more.
        let pace = Pace {
            resume_from,
            stop_at: Some(std::time::Instant::now()),
        };
        calls += 1;
        match walk_contents(&provider, &listing, false, pace, &context)
            .await
            .expect("paced walk")
        {
            Walked::Done(traversal) => break traversal,
            Walked::Paused { reads } => {
                assert!(reads > resume_from, "a resumed walk gets further: {reads}");
                resume_from = reads;
            }
        }
    };
    assert!(calls > 1, "a passed stop pauses a walk of several folders");
    assert_eq!(paths(&resumed), paths(&whole));
    let mut once = paths(&resumed);
    once.dedup();
    assert_eq!(once, paths(&resumed), "every entry once");
}

/// A paused walk's page lists nothing yet and says why: partial at the
/// walk's time budget, with `next.nextPage` the same pinned query resuming
/// after the reads made.
#[test]
fn a_paused_walk_page_resumes_after_its_reads() {
    let pinned: GhStructureQueryTree = serde_json::from_value(json!({
        "operation":"tree","owner":"a","repo":"b","ref":SHA,"include":["**/*.rs"]
    }))
    .expect("tree query");
    let data = crate::gh_structure::tree::paused_listing(&pinned, "main", SHA, 7);
    assert!(data.get("files").is_none(), "{data}");
    assert!(data.get("summary").is_none(), "{data}");
    assert_eq!(data["partialReasons"], json!(["walkDeadline"]), "{data}");
    assert_eq!(
        data["warnings"],
        json!([
            "Walk paused at its time budget after 7 folder reads: nothing is listed until next ends the walk."
        ]),
        "{data}"
    );
    let next = &data["next"]["nextPage"]["input"]["queries"][0];
    assert_eq!(next["walkOffset"], 7, "{data}");
    assert_eq!(next["ref"], SHA, "{data}");
    assert_eq!(next["include"], json!(["**/*.rs"]), "{data}");
    octocode_runtime_base::test_support::assert_row_contract(ToolId::GhStructure, &data);
}

/// A directory past the Contents API's 1,000-entry cap is listed
/// whole from its own git tree (the root's is the commit's; a scoped
/// directory's SHA comes from its parent listing) and paged like any
/// listing: every entry once, no provider limit, no clone lead.
#[tokio::test]
async fn a_directory_past_the_contents_cap_is_listed_whole_and_paged() {
    let names = |count: usize| (0..count).map(|n| format!("f{n:04}.rs"));
    let contents: Vec<Value> = names(1000)
        .map(|name| json!({"name":name,"path":name,"type":"file","size":1}))
        .collect();
    let tree: Vec<Value> = names(1200)
        .map(|name| json!({"path":name,"type":"blob","size":1}))
        .collect();
    let server = server().await;
    mount_json(&server, "/api/v3/repos/a/b/contents", 200, json!(contents)).await;
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/git/trees/{SHA}"),
        200,
        json!({"sha":SHA,"truncated":false,"tree":tree}),
    )
    .await;
    let mut listed = Vec::new();
    let mut row = json_fixture(r#"{"operation": "tree", "owner": "a", "repo": "b"}"#);
    let pages = 1200_u64.div_ceil(
        octocode_runtime_base::contracts::query_limits::gh_structure::tree::LIMIT_DEFAULT as u64,
    );
    for page in 1..=pages {
        let out = list(&server, row.clone()).await.expect("listing");
        let data = &out.data;
        assert!(data.get("terminalLimit").is_none(), "page {page}: {data}");
        assert!(data.get("providerLimit").is_none(), "page {page}: {data}");
        assert!(
            octocode_runtime_base::test_support::checks::find_related_call(
                data,
                "ghCloneRepo",
                |_| true
            )
            .is_none(),
            "page {page}: {data}"
        );
        for name in data["files"][0]["files"].as_array().expect("files") {
            listed.push(name.as_str().unwrap_or_default().to_owned());
        }
        let next = data["next"].get("nextPage");
        match next {
            Some(next) => row = next["input"]["queries"][0].clone(),
            None => {
                assert_eq!(page, pages, "{data}");
                break;
            }
        }
    }
    let expected: Vec<String> = names(1200).map(|name| format!("{name} (1)")).collect();
    assert_eq!(listed, expected);

    // A scoped directory: its own tree at the commit lists it whole.
    let scoped = |count: usize| {
        names(count)
            .map(|name| json!({"name":name,"path":format!("big/{name}"),"type":"file","size":1}))
            .collect::<Vec<_>>()
    };
    let server = self::server().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/big",
        200,
        json!(scoped(1000)),
    )
    .await;
    mount_folder_tree(
        &server,
        "/api/v3/repos/a/b",
        SHA,
        "big",
        &json!(scoped(1200)),
    )
    .await;
    let out = list(
        &server,
        json_fixture(
            r#"{
                "operation": "tree",
                "owner": "a",
                "repo": "b",
                "path": "big",
                "page": 3
            }"#,
        ),
    )
    .await
    .expect("scoped listing");
    assert!(out.data.get("providerLimit").is_none(), "{}", out.data);
    assert_eq!(out.data["files"][0]["dir"], "big", "{}", out.data);
    assert_eq!(
        out.data["files"][0]["files"].as_array().map(Vec::len),
        Some(octocode_runtime_base::contracts::query_limits::gh_structure::tree::LIMIT_DEFAULT),
        "{}",
        out.data
    );
}

/// A listing of a file is typed; its one repair is that file's read
/// at the commit the listing resolved (not the movable ref), with no
/// prose beside it.
#[tokio::test]
async fn a_listing_of_a_file_leads_to_its_read() {
    let server = server().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src%2Flib.rs",
        200,
        json_fixture(
            r#"{
                "name": "lib.rs",
                "path": "src/lib.rs",
                "type": "file",
                "size": 3
            }"#,
        ),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let context = default_context();
    let home = tempfile::tempdir().expect("home");
    let failure = run(
        &provider,
        &query(json_fixture(
            r#"{
                "operation": "tree",
                "owner": "a",
                "repo": "b",
                "path": "src/lib.rs",
                "ref": "main"
            }"#,
        )),
        Ok(&context),
        home.path(),
        None,
    )
    .await
    .expect_err("a file is not a tree");
    assert_eq!(failure.error.reason, Some(ProviderErrorReason::PathIsFile));
    assert_eq!(
        failure.message,
        "Path \"src/lib.rs\" is a file, not a directory"
    );
    assert!(failure.hints.is_empty(), "{:?}", failure.hints);
    let next = failure.calls.expect("read lead");
    assert_eq!(next["read"]["tool"], "ghGetFileContent", "{next}");
    assert_eq!(
        next["read"]["input"]["queries"][0],
        json!({"owner":"a","repo":"b","path":"src/lib.rs","ref":SHA}),
        "{next}"
    );
}

/// A listing of a submodule leads to the submodule's repository at
/// the commit the parent pins, as a read of it does; a symlink is
/// read (the read follows it to its file). Neither gets the generic
/// invalid-field tip.
#[tokio::test]
async fn a_listing_of_a_submodule_or_symlink_leads_to_its_content() {
    let server = server().await;
    let pinned = "89abcdef0123456789abcdef0123456789abcdef";
    // Their folders' trees type them, as GitHub's do: a submodule a commit,
    // a symlink a blob of mode 120000.
    mount_folder_tree(
        &server,
        "/api/v3/repos/a/b",
        SHA,
        "vendor",
        &json!([{"name":"lib","path":"vendor/lib","type":"submodule","sha":pinned}]),
    )
    .await;
    mount_folder_tree(
        &server,
        "/api/v3/repos/a/b",
        SHA,
        "docs",
        &json_fixture(r#"[{"name": "link", "path": "docs/link", "type": "symlink"}]"#),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/vendor%2Flib",
        200,
        json!({"type":"submodule","name":"lib","path":"vendor/lib","sha":pinned,
            "submodule_git_url":format!("{}/o2/r2.git", server.uri())}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/docs%2Flink",
        200,
        json_fixture(
            r#"{
                "type": "symlink",
                "name": "link",
                "path": "docs/link",
                "target": "../README.md"
            }"#,
        ),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let context = default_context();
    let home = tempfile::tempdir().expect("home");
    let listing = |path: &str| {
        query(json!({"operation":"tree","owner":"a","repo":"b","path":path,"ref":"main"}))
    };
    let submodule = run(
        &provider,
        &listing("vendor/lib"),
        Ok(&context),
        home.path(),
        None,
    )
    .await
    .expect_err("a submodule is not a tree");
    assert_eq!(submodule.error.reason, Some(ProviderErrorReason::Submodule));
    assert!(submodule.hints.is_empty(), "{:?}", submodule.hints);
    let next = submodule.calls.expect("submodule lead");
    let row = &next["viewTree"]["input"]["queries"][0];
    assert_eq!(next["viewTree"]["tool"], "ghStructure", "{next}");
    assert_eq!(
        (&row["owner"], &row["repo"]),
        (&json!("o2"), &json!("r2")),
        "{next}"
    );
    assert_eq!(row["ref"], pinned, "{next}");
    let symlink = run(
        &provider,
        &listing("docs/link"),
        Ok(&context),
        home.path(),
        None,
    )
    .await
    .expect_err("a symlink is not a tree");
    assert_eq!(
        symlink.message,
        "Path \"docs/link\" is a symlink, not a directory"
    );
    let next = symlink.calls.expect("read lead");
    assert_eq!(
        next["read"]["input"]["queries"][0],
        json!({"owner":"a","repo":"b","path":"docs/link","ref":SHA}),
        "{next}"
    );
}
