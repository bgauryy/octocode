//! A read at a full SHA sends no ref resolution, so its bare 404
//! cannot tell a missing repository from a missing path. One repository
//! read tells them apart; a read whose ref resolved never spends it.
use super::*;

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

async fn not_found_elsewhere(server: &MockServer) {
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(server)
        .await;
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
        json!(["b"]),
        "{calls}"
    );
    assert!(calls.get("viewTree").is_none(), "{calls}");
    assert_eq!(repository_reads(&server).await, 1);
}

#[tokio::test]
async fn a_missing_path_at_a_sha_in_an_existing_repository_stays_a_path_failure() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json!({"default_branch":"main","full_name":"a/b","private":false,"archived":false}),
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
        json!({"owner":"a","repo":"b","path":"README.md","ref":"main"}),
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
/// recovery walk: one ref resolution, then one listing per level from the
/// parent up to the first directory that exists, and nothing else (no
/// repository read, no repeated ref resolution).
#[tokio::test]
async fn a_missing_path_sends_the_minimum_requests() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"README.md","path":"README.md","type":"file","sha":"x","size":1}
        ])))
        .mount(&server)
        .await;
    not_found_elsewhere(&server).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let failure = run_default(
        &provider,
        json!({"owner":"a","repo":"b","path":"docs/guide/missing.md","ref":"main"}),
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
    // The path read once, the ref resolved once, one listing per level.
    for (suffix, count) in [
        ("", 0),
        ("/commits/main", 1),
        ("/contents/docs%2Fguide%2Fmissing.md", 1),
        ("/contents/docs%2Fguide", 1),
        ("/contents/docs", 1),
        ("/contents", 1),
    ] {
        assert_eq!(seen(suffix).await, count, "{suffix}");
    }
}
