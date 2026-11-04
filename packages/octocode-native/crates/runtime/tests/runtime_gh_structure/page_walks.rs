//! A tree walk: only the first page lists the tree, a later page answers from
//! what its cursor owns, and no page makes a date lookup.
use super::*;

/// Walking every opaque continuation lists each entry exactly once at the
/// default `limit`, and costs the one listing however many pages it takes.
#[tokio::test]
async fn a_large_tree_lists_every_entry_once_through_opaque_continuations() {
    let names = (0..650)
        .map(|n| (format!("f{n:03}.rs"), "file"))
        .collect::<Vec<_>>();
    let server = listing_server(&names).await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let mut pending = Some(json!({"queries":[{"operation":"tree","owner":"a","repo":"b"}]}));
    let mut listed = Vec::new();
    let mut sizes = Vec::new();
    let reads = async || server.received_requests().await.unwrap().len();
    while let Some(input) = pending.take() {
        let before = reads().await;
        let outcome = call(&runtime, "ghStructure", input).await.expect("listing");
        let sent = reads().await - before;
        assert_eq!(
            sent == 0,
            !sizes.is_empty(),
            "only the first page lists the tree: {sent} requests on page {}",
            sizes.len() + 1
        );
        let data = row_data(&outcome);
        assert!(data.get("warnings").is_none(), "{data}");
        let page = data["files"][0]["files"].as_array().expect("entries");
        sizes.push(page.len());
        listed.extend(page.iter().map(|entry| entry.as_str().unwrap().to_owned()));
        pending = data.pointer("/next/nextPage/input").cloned();
        if let Some(input) = &pending {
            support::assert_cursor_only(input);
        }
        assert!(sizes.len() <= 3, "bounded pages");
    }
    assert_eq!(sizes, [300, 300, 50]);
    let expected = names
        .iter()
        .map(|(name, _)| format!("{name} (10)"))
        .collect::<Vec<_>>();
    assert_eq!(listed, expected);
    assert!(graphql_requests(&server).await.is_empty());
    runtime.close().await;
}

/// A listing under `path` names each row's `dir` repo-relative, the same base
/// as `path` and `include`.
#[tokio::test]
async fn a_scoped_listing_names_repo_relative_dirs() {
    let server = listing_server(&[]).await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/trees/{SHA}:pkg")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": SHA, "truncated": false, "tree": [
                {"path":"a.rs","type":"blob","size":1},
                {"path":"src","type":"tree"},
                {"path":"src/lib.rs","type":"blob","size":1}
            ]
        })))
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
    assert_eq!(
        data["files"],
        json!([
            {"dir": "pkg", "files": ["a.rs (1)", "src/"]},
            {"dir": "pkg/src", "files": ["lib.rs (1)"]},
        ]),
        "{data}"
    );
    assert!(graphql_requests(&server).await.is_empty());
    runtime.close().await;
}
