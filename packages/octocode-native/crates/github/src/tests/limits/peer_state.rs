//! Rate-limit facts other processes share through the state file: what
//! reaches this process's requests, and what never leaves the process that
//! received it.
use super::*;
use octocode_test_support::json_fixture;

/// A transport on `server` mirroring its rate-limit facts under `dir`, with
/// its own budget: another process, as far as the state file can tell.
fn process(server: &MockServer, dir: &std::path::Path) -> (GitHubTransport, GitHubEndpoint) {
    let (mut transport, endpoint) =
        executor_transport(server, GitHubBudget::relaxed(), short_retry());
    transport.set_rate_limit_state_dir(Some(dir.to_owned()));
    (transport, endpoint)
}

/// Requests `server` received for `path`.
async fn sent_to(server: &MockServer, path: &str) -> usize {
    server
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .filter(|request| request.url.path() == path)
        .count()
}

/// A peer's secondary-limit cooldown that this process first meets while it
/// writes a fact of its own (a spent search bucket, learned from a response
/// admitted before the peer's limit) still holds this process: its next core
/// request is refused without being sent.
#[tokio::test]
async fn a_peer_cooldown_met_by_this_process_write_refuses_its_next_core_request() {
    let server = MockServer::builder().start().await;
    let arrived = Arc::new(tokio::sync::Notify::new());
    let spent_search = Delayed {
        arrived: Some(Arc::clone(&arrived)),
        ..Delayed::new(
            Duration::from_millis(1500),
            ResponseTemplate::new(200)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header(
                    "x-ratelimit-reset",
                    (epoch_secs() + 60).to_string().as_str(),
                )
                .insert_header("x-ratelimit-resource", "search")
                .set_body_json(json_fixture(
                    r#"{"total_count": 0, "incomplete_results": false, "items": []}"#,
                )),
        )
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(spent_search)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/peer/limited"))
        .respond_with(secondary_limit(Some("30")))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ok_json())
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("state directory");
    let (this, endpoint) = process(&server, dir.path());
    let (peer, _) = process(&server, dir.path());
    let peer_limited = async {
        arrived.notified().await;
        let error = get_status(&peer, &endpoint, &["repos", "peer", "limited"])
            .await
            .expect_err("the peer meets a secondary limit");
        assert_eq!(error.kind, ProviderErrorKind::RateLimited);
        GitHubBudget::flush_detached_writes().await;
    };
    let (searched, ()) = tokio::join!(
        get_status(&this, &endpoint, &["search", "issues"]),
        peer_limited
    );
    searched.expect("the search admitted before the peer's limit");
    GitHubBudget::flush_detached_writes().await;

    let error = get_status(&this, &endpoint, &["repos", "a", "b"])
        .await
        .expect_err("the peer's cooldown holds this process");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited, "{error:?}");
    let retry_after = error
        .rate_limit
        .as_ref()
        .and_then(|rate| rate.retry_after_seconds);
    assert!(
        retry_after.is_some_and(|seconds| seconds > 5),
        "the peer's cooldown: {error:?}"
    );
    assert_eq!(sent_to(&server, "/api/v3/repos/a/b").await, 0);
}

/// A `retry-after` beyond GitHub's own rate-limit window (a bogus header, a
/// skewed clock) holds the process that received it, which states it whole,
/// but it is not mirrored: another process's request on the same key is sent.
#[tokio::test]
async fn a_cooldown_beyond_the_github_window_holds_only_the_receiving_process() {
    let declared = 10 * 24 * 60 * 60_u64;
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/limited/x"))
        .respond_with(secondary_limit(Some(&declared.to_string())))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ok_json())
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("state directory");
    let (receiving, endpoint) = process(&server, dir.path());
    let error = get_status(&receiving, &endpoint, &["repos", "limited", "x"])
        .await
        .expect_err("the declared cooldown outlasts the deadline");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert_eq!(
        error.rate_limit.and_then(|rate| rate.retry_after_seconds),
        Some(declared),
        "the receiving call states the declared wait whole"
    );
    let held = get_status(&receiving, &endpoint, &["repos", "a", "b"])
        .await
        .expect_err("the receiving process honors its cooldown");
    assert_eq!(held.kind, ProviderErrorKind::RateLimited);
    GitHubBudget::flush_detached_writes().await;
    assert_eq!(sent_to(&server, "/api/v3/repos/a/b").await, 0);

    let (other, _) = process(&server, dir.path());
    let page = get_status(&other, &endpoint, &["repos", "a", "b"])
        .await
        .expect("another process is not held by it");
    assert_eq!(page.status, 200);
    assert_eq!(sent_to(&server, "/api/v3/repos/a/b").await, 1);
}
