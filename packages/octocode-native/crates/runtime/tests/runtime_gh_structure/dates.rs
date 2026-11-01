//! Tree pages date their entries
//! and the listed commit in one GraphQL request, and a GraphQL failure
//! keeps the listing.
use super::*;

/// A tree page dates every listed
/// file and folder at the listed commit and states that commit's date, in
/// one GraphQL request.
#[tokio::test]
async fn a_tree_page_dates_every_entry_and_the_listed_commit() {
    let server = listing_server(&[
        ("one.rs".into(), "file"),
        ("src".into(), "dir"),
        ("a \"q\"\\b.md".into(), "file"),
    ])
    .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(DatesEcho)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"operation":"tree","owner": "a", "repo": "b"}),
    )
    .await
    .expect("listing");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(data["commitDate"], "2026-01-31", "{data}");
    assert_eq!(
        json!(updated(&data["files"][0])),
        json!({"one.rs": "2025-03-07", "src": "2025-03-04", "a \"q\"\\b.md": "2025-03-11"}),
        "{data}"
    );
    assert!(data.get("warnings").is_none(), "{data}");
    let requests = graphql_requests(&server).await;
    assert_eq!(requests.len(), 1, "one GraphQL request per page");
    assert_eq!(requests[0]["variables"]["oid"], SHA);
    runtime.close().await;
}

/// Date continuations retain the pinned listing and reach every date exactly once.
#[tokio::test]
async fn a_large_tree_reaches_every_entry_date_through_opaque_continuations() {
    let names = (0..250)
        .map(|n| (format!("f{n:03}.rs"), "file"))
        .collect::<Vec<_>>();
    let server = listing_server(&names).await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(DatesEcho)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let mut pending = Some(json!({"queries":[{"operation":"tree","owner":"a","repo":"b"}]}));
    let mut dated = serde_json::Map::new();
    let mut pages = 0;
    while let Some(input) = pending.take() {
        let outcome = call(&runtime, "ghStructure", input)
            .await
            .expect("dated listing");
        let data = row_data(&outcome);
        for group in data["files"].as_array().expect("listing groups") {
            for (name, date) in updated(group) {
                assert!(
                    dated.insert(name.clone(), date).is_none(),
                    "date repeated: {name}"
                );
            }
        }
        pending = data.pointer("/next/nextPage/input").cloned();
        if let Some(input) = &pending {
            support::assert_cursor_only(input);
        }
        pages += 1;
        assert!(pages <= 3, "bounded date batches");
    }
    assert_eq!(dated.len(), 250);
    assert!(names.iter().all(|(name, _)| dated.contains_key(name)));
    assert_eq!(graphql_requests(&server).await.len(), 3);
    runtime.close().await;
}

/// A failed date request never fails the listing: every entry is listed,
/// no entry is dated, and one warning says the dates are missing.
#[tokio::test]
async fn a_graphql_failure_keeps_the_listing_and_warns_once() {
    let server = listing_server(&[("one.rs".into(), "file"), ("src".into(), "dir")]).await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": null,
            "errors": [{"message": "Something unexpected"}]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"operation":"tree","owner": "a", "repo": "b"}),
    )
    .await
    .expect("listing");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(
        data["files"][0]["files"],
        json!(["one.rs (10)", "src/"]),
        "{data}"
    );
    assert!(updated(&data["files"][0]).is_empty(), "{data}");
    assert!(data.get("commitDate").is_none(), "{data}");
    let warnings = data["warnings"].as_array().expect("warnings");
    assert_eq!(warnings.len(), 1, "{data}");
    assert!(
        warnings[0]
            .as_str()
            .is_some_and(|text| text.contains("2 files")),
        "{data}"
    );
    let retry = &data["next"]["retry"]["input"];
    support::assert_cursor_only(retry);
    runtime.close().await;
}

/// A listing under `path` names each row's `dir` repo-relative (the
/// same base as `path` and `include`), and every listed name keeps its date.
#[tokio::test]
async fn a_scoped_listing_names_repo_relative_dirs_and_dates_every_name() {
    let server = listing_server(&[]).await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/trees/{SHA}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": SHA, "truncated": false, "tree": [
                {"path":"pkg","type":"tree"},
                {"path":"pkg/a.rs","type":"blob","size":1},
                {"path":"pkg/src","type":"tree"},
                {"path":"pkg/src/lib.rs","type":"blob","size":1},
                {"path":"other.rs","type":"blob","size":1}
            ]
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(DatesEcho)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"operation":"tree","owner": "a", "repo": "b", "path": "pkg", "maxDepth": 2}),
    )
    .await
    .expect("listing");
    let data = row_data(&outcome);
    let entries = data["files"].as_array().expect("files");
    assert_eq!(entries[0]["dir"], "pkg", "{data}");
    assert_eq!(
        undated(&entries[0], "files"),
        json!(["a.rs (1)", "src/"]),
        "{data}"
    );
    assert_eq!(entries[1]["dir"], "pkg/src", "{data}");
    // Dates key on the full path: `pkg/a.rs` (8) and `pkg/src` (7) and
    // `pkg/src/lib.rs` (14).
    assert_eq!(
        json!(updated(&entries[0])),
        json!({"a.rs": "2025-03-09", "src": "2025-03-08"}),
        "{data}"
    );
    assert_eq!(
        json!(updated(&entries[1])),
        json!({"lib.rs": "2025-03-15"}),
        "{data}"
    );
    runtime.close().await;
}
