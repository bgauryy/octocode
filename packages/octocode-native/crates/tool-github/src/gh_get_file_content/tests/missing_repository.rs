//! A read at a full SHA sends no ref resolution, so its bare 404
//! cannot tell a missing repository from a missing path. One repository
//! read tells them apart; a read whose ref resolved never spends it.
use super::*;
use octocode_test_support::json_fixture;

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

async fn not_found_elsewhere(server: &MockServer) {
    Mock::given(wiremock::matchers::any())
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json_fixture(r#"{"message": "Not Found"}"#)),
        )
        .mount(server)
        .await;
}

/// The login `a` exists (`GET /users/a`): only its repository is missing.
async fn owner_exists(server: &MockServer) {
    mount_json(server, "/api/v3/users/a", 200, json!({"login":"a"})).await;
}

async fn repository_reads(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .filter(|request| request.url.path() == "/api/v3/repos/a/b")
        .count()
}

#[tokio::test]
async fn a_missing_repository_at_a_sha_leads_to_a_repository_search() {
    let server = MockServer::builder().start().await;
    owner_exists(&server).await;
    not_found_elsewhere(&server).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let failure = run_default(
        &provider,
        json!({"owner":"a","repo":"b","path":"README.md","ref":SHA}),
    )
    .await
    .expect_err("missing repository");
    assert_eq!(
        failure.error.reason,
        Some(octocode_github::ProviderErrorReason::RepositoryNotFound),
        "{}",
        failure.message
    );
    let calls = failure.calls.as_deref().expect("leads");
    assert_eq!(calls["findRepository"]["tool"], "ghSearchRepo", "{calls}");
    assert_eq!(
        calls["findRepository"]["input"]["queries"][0]["keywords"],
        json_fixture(r#"["b"]"#),
        "{calls}"
    );
    assert!(calls.get("viewTree").is_none(), "{calls}");
    assert_eq!(repository_reads(&server).await, 1);
}

/// A repository named as its clone URL names it (`b.git`) that GitHub does
/// not resolve: `next.repair` is the same read naming `b`, and the name
/// search asks for `b` too.
#[tokio::test]
async fn a_missing_repository_named_with_a_clone_suffix_is_repaired() {
    let server = MockServer::builder().start().await;
    owner_exists(&server).await;
    not_found_elsewhere(&server).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let failure = run_default(
        &provider,
        json!({"owner":"a","repo":"b.git","path":"README.md","ref":SHA}),
    )
    .await
    .expect_err("missing repository");
    let calls = failure.calls.as_deref().expect("leads");
    let repair = &calls["repair"]["input"]["queries"][0];
    assert_eq!(calls["repair"]["tool"], "ghGetFileContent", "{calls}");
    assert_eq!(repair["repo"], "b", "{calls}");
    assert_eq!(repair["path"], "README.md", "{calls}");
    assert_eq!(repair["ref"], SHA, "{calls}");
    assert_eq!(
        calls["findRepository"]["input"]["queries"][0]["keywords"],
        json!(["b"]),
        "{calls}"
    );
}

/// A read under a login GitHub does not know names the owner, not the
/// repository, and offers no call into that owner (its listing, the same
/// read under another repository name): each would fail the same way. The
/// name search across every owner stays.
#[tokio::test]
async fn a_missing_owner_is_named_and_no_lead_asks_for_it() {
    for repo in ["b", "b.git"] {
        let server = MockServer::builder().start().await;
        not_found_elsewhere(&server).await;
        let provider = mock_provider(&server, RetryPolicy::default());
        let failure = run_default(
            &provider,
            json!({"owner":"a","repo":repo,"path":"README.md","ref":SHA}),
        )
        .await
        .expect_err("missing owner");
        assert_eq!(
            failure.message,
            octocode_tools_shared::gh_shared::owner_not_found_message("a"),
            "{repo}"
        );
        let calls = failure.calls.as_deref().cloned().unwrap_or_default();
        for (name, lead) in calls.as_object().into_iter().flatten() {
            for row in lead["input"]["queries"].as_array().into_iter().flatten() {
                assert!(
                    row.get("owner").is_none(),
                    "{name} asks for the owner: {calls}"
                );
            }
        }
        assert_eq!(calls["findRepository"]["tool"], "ghSearchRepo", "{calls}");
        assert!(!failure.hints.is_empty(), "{repo}: the lead is labelled");
    }
}

#[tokio::test]
async fn a_missing_path_at_a_sha_in_an_existing_repository_stays_a_path_failure() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json_fixture(
            r#"{
                "default_branch": "main",
                "full_name": "a/b",
                "private": false,
                "archived": false
            }"#,
        ),
    )
    .await;
    not_found_elsewhere(&server).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let failure = run_default(
        &provider,
        json!({"owner":"a","repo":"b","path":"README.md","ref":SHA}),
    )
    .await
    .expect_err("missing path");
    assert_ne!(
        failure.error.reason,
        Some(octocode_github::ProviderErrorReason::RepositoryNotFound),
        "{}",
        failure.message
    );
    let calls = failure.calls.as_deref().cloned().unwrap_or_default();
    assert!(calls.get("findRepository").is_none(), "{calls}");
    assert_eq!(repository_reads(&server).await, 1, "{}", failure.message);
}

#[tokio::test]
async fn a_resolved_ref_spends_no_repository_read_on_a_missing_path() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    not_found_elsewhere(&server).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let failure = run_default(
        &provider,
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "path": "README.md",
                "ref": "main"
            }"#,
        ),
    )
    .await
    .expect_err("missing path");
    assert_ne!(
        failure.error.reason,
        Some(octocode_github::ProviderErrorReason::RepositoryNotFound)
    );
    assert_eq!(repository_reads(&server).await, 0, "{}", failure.message);
}

/// GFC-3: a missing path under a missing directory costs the read, then the
/// recovery walk: one ref resolution, then one folder tree per level from the
/// parent up to the first directory that exists, and nothing else (no
/// repository read, no repeated ref resolution, no Contents listing).
#[tokio::test]
async fn a_missing_path_sends_the_minimum_requests() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    octocode_test_support::mock_http::mount_folder_tree(
        &server,
        "/api/v3/repos/a/b",
        SHA,
        "",
        &json_fixture(
            r#"[
                {
                    "name": "README.md",
                    "path": "README.md",
                    "type": "file",
                    "sha": "x",
                    "size": 1
                }
            ]"#,
        ),
    )
    .await;
    not_found_elsewhere(&server).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let failure = run_default(
        &provider,
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "path": "docs/guide/missing.md",
                "ref": "main"
            }"#,
        ),
    )
    .await
    .expect_err("missing path");
    assert!(
        failure.message.contains("docs/guide/missing.md"),
        "{}",
        failure.message
    );
    let seen = |suffix: &str| {
        let wanted = format!("/api/v3/repos/a/b{suffix}");
        let server = &server;
        async move {
            server
                .received_requests()
                .await
                .expect("recorded")
                .iter()
                .filter(|request| request.url.path() == wanted)
                .count()
        }
    };
    // The path read once, the ref resolved once, and each folder tree from
    // the parent up to the root twice: once by the read's symlink check, once
    // by the recovery walk. No Contents listing.
    let tree = |dir: &str| format!("/git/trees/{SHA}{dir}");
    let (guide, docs, root) = (tree(":docs%2Fguide"), tree(":docs"), tree(""));
    for (suffix, count) in [
        ("", 0),
        ("/commits/main", 1),
        ("/contents/docs%2Fguide%2Fmissing.md", 1),
        ("/contents/docs%2Fguide", 0),
        ("/contents/docs", 0),
        ("/contents", 0),
        (guide.as_str(), 2),
        (docs.as_str(), 2),
        (root.as_str(), 2),
    ] {
        assert_eq!(seen(suffix).await, count, "{suffix}");
    }
}
