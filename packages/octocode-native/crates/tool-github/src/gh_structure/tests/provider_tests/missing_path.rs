//! A listing of a missing path: what it costs and how it is classified.
use super::*;
use octocode_github::ProviderErrorKind;
use octocode_test_support::json_fixture;

#[tokio::test]
async fn a_missing_path_is_classified_as_missing_and_located_from_its_parent_listing() {
    let server = server().await;
    mount_json(
        &server,
        &format!("/api/v3/repos/a/b/git/trees/{SHA}"),
        200,
        json!({"sha": SHA, "truncated": false, "tree": [
            {"path":"README.md","type":"blob","mode":"100644","size":3},
            {"path":"docs","type":"tree","mode":"040000"},
            {"path":"docs/guide","type":"tree","mode":"040000"}
        ]}),
    )
    .await;
    let docs =
        json_fixture(r#"[{"name": "guide", "path": "docs/guide", "type": "dir", "size": 0}]"#);
    mount_folder_tree(&server, "/api/v3/repos/a/b", SHA, "docs", &docs).await;
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
                "path": "docs/nope/missing"
            }"#,
        )),
        Ok(&context),
        home.path(),
        None,
    )
    .await
    .expect_err("missing path");
    // Classification: the path is missing (not the repository, not a file),
    // and the recovery lists the deepest directory that exists.
    assert_eq!(failure.error.kind, ProviderErrorKind::NotFound);
    assert_eq!(failure.message, "Path not found in a/b: docs/nope/missing");
    let calls = failure.calls.expect("recovery lead");
    assert_eq!(calls["viewTree"]["input"]["queries"][0]["path"], "docs");
    // Cost: the repository and its head, the path's own tree read, the
    // trees of its folders up to the deepest that answers (a Contents read
    // of a link to a file answers the target's bytes, so a tree types the
    // path first), the Contents read, then the folder trees from the missing
    // path's parent up to the directory that exists: never a Contents
    // listing, nor the commit's recursive tree, whose size follows the
    // repository's.
    let requests = server.received_requests().await.unwrap_or_default();
    let mut paths = requests
        .iter()
        .map(|request| {
            request
                .url
                .path()
                .trim_start_matches("/api/v3/repos/a/b")
                .to_owned()
        })
        .collect::<Vec<_>>();
    // The repository and its head are read concurrently: their order is not
    // part of the cost; everything after them is sequential.
    let first_two = paths.len().min(2);
    paths[..first_two].sort();
    assert_eq!(
        paths,
        [
            "",
            "/commits/HEAD",
            &format!("/git/trees/{SHA}:docs%2Fnope%2Fmissing"),
            &format!("/git/trees/{SHA}:docs%2Fnope"),
            &format!("/git/trees/{SHA}:docs"),
            "/contents/docs%2Fnope%2Fmissing",
            &format!("/git/trees/{SHA}:docs%2Fnope"),
            &format!("/git/trees/{SHA}:docs"),
        ],
        "{paths:?}"
    );
}

#[tokio::test]
async fn failure_of_the_commit_tree_stays_retryable_after_parent_fallback() {
    let server = server().await;
    mount_json(
        &server,
        &format!("/api/v3/repos/a/b/git/trees/{SHA}"),
        503,
        json!({"message": "Service Unavailable"}),
    )
    .await;
    let provider = mock_provider(
        &server,
        RetryPolicy {
            max_attempts: 1,
            ..RetryPolicy::default()
        },
    );
    let home = tempfile::tempdir().expect("home");
    let failure = execute(
        &provider,
        &query(json!({"operation": "tree", "owner": "a", "repo": "b", "path": "docs/missing", "maxDepth": 1})),
        &default_context(),
        home.path(),
        None,
    )
    .await
    .expect_err("commit tree unavailable");
    assert_eq!(failure.kind, ProviderErrorKind::Server);
    assert_eq!(failure.status, Some(503));
    assert!(
        server
            .received_requests()
            .await
            .expect("provider requests")
            .iter()
            .all(|request| !request.url.path().contains("/contents/")),
    );
}

/// A missing path asked at a ref names that ref, as a file read does: a 404
/// at one ref says nothing of another (a full SHA by its short form).
#[tokio::test]
async fn a_missing_path_at_a_ref_names_the_ref() {
    let server = server().await;
    // The commit exists: the path, not the ref, is missing.
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{SHA}")))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    mount_json(
        &server,
        &format!("/api/v3/repos/a/b/git/trees/{SHA}"),
        200,
        json!({"sha": SHA, "truncated": false, "tree": [
            {"path":"README.md","type":"blob","mode":"100644","size":3},
            {"path":"docs","type":"tree","mode":"040000"}
        ]}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let context = default_context();
    let home = tempfile::tempdir().expect("home");
    let failure = run(
        &provider,
        &query(json!({
            "operation": "tree",
            "owner": "a",
            "repo": "b",
            "path": "crates/searcher",
            "ref": SHA,
        })),
        Ok(&context),
        home.path(),
        None,
    )
    .await
    .expect_err("missing path");
    assert_eq!(failure.error.kind, ProviderErrorKind::NotFound);
    assert_eq!(
        failure.message,
        format!("Path not found in a/b at {}: crates/searcher", &SHA[..7])
    );
}

/// A missing path whose parent's git tree GitHub truncated may sit past the
/// cut, and the failure says so beside its listing lead.
#[tokio::test]
async fn a_missing_path_in_a_truncated_tree_says_it_may_be_past_the_cut() {
    let server = server().await;
    mount_json(
        &server,
        &format!("/api/v3/repos/a/b/git/trees/{SHA}"),
        200,
        json!({"sha": SHA, "truncated": false, "tree": []}),
    )
    .await;
    mount_json(
        &server,
        &format!("/api/v3/repos/a/b/git/trees/{SHA}:docs"),
        200,
        json!({"sha": SHA, "truncated": true, "tree": [
            {"path": "f0", "mode": "100644", "type": "blob", "sha": "1".repeat(40), "size": 1}
        ]}),
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
                "path": "docs/readme.md"
            }"#,
        )),
        Ok(&context),
        home.path(),
        None,
    )
    .await
    .expect_err("missing path");
    assert_eq!(
        failure.hints,
        [octocode_tools_shared::gh_shared::LISTING_MAY_BE_CUT_HINT]
    );
}
