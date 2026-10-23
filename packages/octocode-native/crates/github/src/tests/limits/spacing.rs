//! Permits during backoff, per-key search spacing and token isolation.
use super::*;

#[tokio::test]
async fn permits_are_released_while_backing_off() {
    let server = MockServer::builder().start().await;
    let flaky = Sequence::new(vec![
        ResponseTemplate::new(503).insert_header("retry-after", "1"),
        ResponseTemplate::new(200).set_body_bytes(b"ok"),
    ]);
    let hits = flaky.arrivals.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/flaky"))
        .respond_with(flaky)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/fast"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"fast"))
        .mount(&server)
        .await;
    // One global slot per key: a request sleeping on retry-after must not
    // hold it.
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig {
        global_concurrency: 1,
        ..ExecutorConfig::relaxed()
    }));
    let (transport, endpoint) = executor_transport(&server, budget, short_retry());
    let slow = {
        let transport = transport.clone();
        let url = endpoint.rest(&["flaky"]).expect("route");
        tokio::spawn(async move {
            transport
                .execute(
                    RequestSpec::get(url),
                    &with_token("t", Duration::from_secs(5), 1024),
                )
                .await
        })
    };
    while hits.count() == 0 {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    let started = std::time::Instant::now();
    let fast = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["fast"]).expect("route")),
            &with_token("t", Duration::from_secs(5), 1024),
        )
        .await
        .expect("fast");
    assert_eq!(fast.body, b"fast".as_slice());
    assert!(
        started.elapsed() < Duration::from_millis(700),
        "fast request waited for the backing-off request's permit"
    );
    let slow = slow.await.expect("join").expect("slow retried");
    assert_eq!(slow.body, b"ok".as_slice());
    assert_eq!(hits.count(), 2);
}

#[tokio::test]
async fn search_requests_are_spaced_per_key() {
    let server = MockServer::builder().start().await;
    let search = Sequence::new(vec![
        ResponseTemplate::new(200).set_body_json(serde_json::json!({"items": []})),
    ]);
    let arrivals = search.arrivals.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(search)
        .mount(&server)
        .await;
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig {
        search_spacing: Duration::from_millis(150),
        ..ExecutorConfig::relaxed()
    }));
    let (transport, endpoint) = executor_transport(&server, budget, short_retry());
    let calls = (0..3).map(|_| {
        let transport = transport.clone();
        let url = endpoint.rest(&["search", "issues"]).expect("route");
        async move {
            transport
                .execute(
                    RequestSpec::get(url),
                    &with_token("t", Duration::from_secs(5), 1024),
                )
                .await
                .expect("search")
        }
    });
    futures_util::future::join_all(calls).await;
    let arrivals = arrivals.times();
    assert_eq!(arrivals.len(), 3);
    for pair in arrivals.windows(2) {
        assert!(
            pair[1].duration_since(pair[0]) >= Duration::from_millis(120),
            "search starts not spaced: {arrivals:?}"
        );
    }
}

#[tokio::test]
async fn limiter_keys_isolate_tokens() {
    let server = MockServer::builder().start().await;
    let reset = epoch_secs() + 3600;
    Mock::given(method("GET"))
        .and(header("authorization", "Bearer exhausted"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", reset.to_string().as_str())
                .set_body_json(serde_json::json!({"message": "API rate limit exceeded"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(header("authorization", "Bearer fresh"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"ok"))
        .expect(2)
        .mount(&server)
        .await;
    let budget = GitHubBudget::relaxed();
    let (transport, endpoint) = executor_transport(&server, budget, short_retry());
    let exhausted = || with_token("exhausted", Duration::from_secs(5), 1024);
    let fresh = || with_token("fresh", Duration::from_secs(5), 1024);
    let url = endpoint.rest(&["repos", "a", "b"]).expect("route");
    transport
        .execute(RequestSpec::get(url.clone()), &exhausted())
        .await
        .expect_err("exhausted");
    transport
        .execute(RequestSpec::get(url.clone()), &exhausted())
        .await
        .expect_err("still exhausted, not sent");
    for _ in 0..2 {
        transport
            .execute(RequestSpec::get(url.clone()), &fresh())
            .await
            .expect("other token unaffected");
    }
}
