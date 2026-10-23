//! The code-search window: room left, processes sharing it, and batches refused before a send.
use super::*;

#[tokio::test]
async fn code_search_room_counts_the_searches_the_window_has_left() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"total_count":0,"incomplete_results":false,"items":[]}),
        ))
        .mount(&server)
        .await;
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig {
        code_search_per_minute: 3,
        ..ExecutorConfig::relaxed()
    }));
    let (transport, _) = executor_transport(&server, budget, short_retry());
    let context = with_token("t", Duration::from_secs(5), 1 << 20);
    assert_eq!(transport.code_search_room(&context), 3);
    for keyword in ["one", "two"] {
        transport
            .search_code(
                &CodeSearchRequest {
                    query: keyword.into(),
                    page: 1,
                    per_page: 5,
                    include_fragments: false,
                },
                &context,
            )
            .await
            .expect("search");
    }
    assert_eq!(transport.code_search_room(&context), 1);
    assert_eq!(
        transport.code_search_room(&with_token("fresh", Duration::from_secs(5), 1 << 20)),
        3,
        "another credential has its own window"
    );
}

/// Four transports with their own budgets share one state directory,
/// as four processes on one `OCTOCODE_HOME` do. Eight concurrent code
/// searches against a 2/min window send at most 2, and every refused search
/// returns well before its deadline instead of queueing toward it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn processes_sharing_a_state_dir_never_exceed_the_window_and_refuse_promptly() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    serde_json::json!({"total_count":0,"incomplete_results":false,"items":[]}),
                )
                .set_delay(Duration::from_millis(200)),
        )
        .mount(&server)
        .await;
    let state = tempfile::tempdir().expect("state directory");
    let transports = (0..4)
        .map(|_| {
            let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig {
                code_search_per_minute: 2,
                ..ExecutorConfig::relaxed()
            }));
            let (mut transport, _) = executor_transport(
                &server,
                budget,
                RetryPolicy {
                    max_rate_limit_wait: Duration::from_secs(1),
                    ..short_retry()
                },
            );
            transport.set_rate_limit_state_dir(Some(state.path().to_path_buf()));
            Arc::new(transport)
        })
        .collect::<Vec<_>>();
    let mut searches = tokio::task::JoinSet::new();
    for (index, transport) in transports.iter().cycle().take(8).enumerate() {
        let transport = Arc::clone(transport);
        searches.spawn(async move {
            let started = std::time::Instant::now();
            let result = transport
                .search_code(
                    &CodeSearchRequest {
                        query: format!("q{index}"),
                        page: 1,
                        per_page: 5,
                        include_fragments: false,
                    },
                    &with_token("t", Duration::from_secs(10), 1 << 20),
                )
                .await;
            (result.map(|_| ()), started.elapsed())
        });
    }
    let mut sent = 0;
    while let Some(joined) = searches.join_next().await {
        let (result, elapsed) = joined.expect("search task");
        match result {
            Ok(()) => sent += 1,
            Err(error) => {
                assert_eq!(error.kind, ProviderErrorKind::RateLimited, "{error:?}");
                assert!(
                    elapsed < Duration::from_secs(3),
                    "refused after {elapsed:?}: a refusal must not queue toward the deadline"
                );
            }
        }
    }
    let received = server.received_requests().await.expect("recorded").len();
    assert_eq!(received, sent);
    assert!(received <= 2, "{received} searches reached a 2/min window");
    assert!(sent >= 1);
}

/// A row of several code searches is refused before the first is
/// sent when they cannot all start within the cap; the refusal reserves no
/// slot and claims no provider counters. A search the cache answers needs no
/// slot, so callers can count only the searches they will send.
#[tokio::test]
async fn a_code_search_batch_that_cannot_fit_is_refused_before_any_send() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"total_count":0,"incomplete_results":false,"items":[]}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig {
        code_search_per_minute: 2,
        ..ExecutorConfig::relaxed()
    }));
    let (mut transport, _) = executor_transport(&server, budget, short_retry());
    transport.cache = Arc::new(PartitionedCache::default());
    let context = with_token("t", Duration::from_secs(5), 1 << 20);
    let request = |query: &str| CodeSearchRequest {
        query: query.into(),
        page: 1,
        per_page: 5,
        include_fragments: false,
    };
    transport
        .search_code(&request("one"), &context)
        .await
        .expect("search");
    assert_eq!(transport.code_search_room(&context), 1);

    assert!(transport.code_search_batch_refusal(1, &context).is_none());
    let refusal = transport
        .code_search_batch_refusal(2, &context)
        .expect("two searches cannot fit one slot within the cap");
    assert_eq!(refusal.kind, ProviderErrorKind::RateLimited);
    assert!(
        refusal.message.contains("2 searches"),
        "{}",
        refusal.message
    );
    let rate = refusal.rate_limit.expect("rate limit");
    assert_eq!(rate.remaining, None);
    assert_eq!(rate.resource.as_deref(), Some("code_search"));
    // The wait is what is left of the one-minute code-search window.
    assert!(
        matches!(rate.retry_after_seconds, Some(1..=60)),
        "{:?}",
        rate.retry_after_seconds
    );
    assert_eq!(
        transport.code_search_room(&context),
        1,
        "a refusal reserves nothing"
    );

    assert!(
        transport
            .cached_code_search(&request("one"), &context)
            .await
            .expect("lookup")
            .is_some(),
        "a cached search is visible without a send"
    );
    assert!(
        transport
            .cached_code_search(&request("two"), &context)
            .await
            .expect("lookup")
            .is_none()
    );
}
