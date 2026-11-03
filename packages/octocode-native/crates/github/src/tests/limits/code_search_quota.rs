//! Code search follows the quota GitHub reports for the `code_search`
//! resource: the room it leaves, the wait once it is spent, and a refusal only
//! when the reset falls past the request's own deadline.
use super::*;

/// A code-search answer reporting `remaining` left until `reset` (epoch seconds).
fn answer(remaining: u64, reset: u64) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("x-ratelimit-limit", "10")
        .insert_header("x-ratelimit-remaining", remaining.to_string().as_str())
        .insert_header("x-ratelimit-reset", reset.to_string().as_str())
        .insert_header("x-ratelimit-resource", "code_search")
        .set_body_json(
            serde_json::json!({"total_count": 0, "incomplete_results": false, "items": []}),
        )
}

async fn search(
    transport: &GitHubTransport,
    context: &RequestContext,
) -> Result<(), ProviderError> {
    transport
        .search_code(
            &CodeSearchRequest {
                query: "q".into(),
                page: 1,
                per_page: 5,
                include_fragments: false,
            },
            context,
        )
        .await
        .map(|_| ())
}

/// The room follows what GitHub reported (10 a minute assumed before any
/// answer), per credential.
#[tokio::test]
async fn code_search_room_follows_the_reported_quota() {
    let server = MockServer::builder().start().await;
    let reset = epoch_secs() + 60;
    let answers = Sequence::new(vec![answer(9, reset), answer(8, reset)]);
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(answers)
        .mount(&server)
        .await;
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig::octokit()));
    let (transport, _) = executor_transport(&server, budget, short_retry());
    let context = with_token("t", Duration::from_secs(5), 1 << 20);
    assert_eq!(transport.code_search_room(&context), 10);
    search(&transport, &context).await.expect("first");
    assert_eq!(transport.code_search_room(&context), 9);
    search(&transport, &context).await.expect("second");
    assert_eq!(transport.code_search_room(&context), 8);
    let fresh = with_token("fresh", Duration::from_secs(5), 1 << 20);
    assert_eq!(transport.code_search_room(&fresh), 10);
}

/// Searches the reported quota covers go back-to-back; the one past it waits
/// for the reset, then goes.
#[tokio::test]
async fn a_spent_quota_waits_for_the_reset_and_a_covered_one_does_not() {
    let server = MockServer::builder().start().await;
    let reset = epoch_secs() + 2;
    let answers = Sequence::new(vec![
        answer(1, reset),
        answer(0, reset),
        answer(9, reset + 60),
    ]);
    let arrivals = answers.arrivals.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(answers)
        .mount(&server)
        .await;
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig::octokit()));
    let (transport, _) = executor_transport(&server, budget, short_retry());
    let context = with_token("t", Duration::from_secs(30), 1 << 20);
    for _ in 0..3 {
        search(&transport, &context).await.expect("search");
    }
    let times = arrivals.times();
    assert_eq!(times.len(), 3);
    assert!(
        times[1].duration_since(times[0]) < Duration::from_millis(500),
        "a covered search waited"
    );
    assert!(
        times[2].duration_since(times[1]) >= Duration::from_secs(1),
        "the search past the quota did not wait for the reset"
    );
}

/// A reset past the request's own deadline is refused at once, with the wait
/// as `retryAfter`, and nothing is sent; a longer deadline waits instead.
#[tokio::test]
async fn a_reset_past_the_deadline_is_refused_without_sending() {
    let server = MockServer::builder().start().await;
    let reset = epoch_secs() + 3600;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(answer(0, reset))
        .expect(1)
        .mount(&server)
        .await;
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig::octokit()));
    let (transport, _) = executor_transport(&server, budget, short_retry());
    let context = with_token("t", Duration::from_secs(5), 1 << 20);
    search(&transport, &context).await.expect("first");
    let started = std::time::Instant::now();
    let error = search(&transport, &context).await.expect_err("spent quota");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert!(started.elapsed() < Duration::from_secs(2));
    let rate = error.rate_limit.expect("metadata");
    assert!(rate.retry_after_seconds.unwrap_or_default() > 3000);
    assert_eq!(rate.resource.as_deref(), Some("code_search"));
}

/// A row of searches that cannot all start before the deadline is refused
/// before the first is sent, once the quota reported says so.
#[tokio::test]
async fn a_batch_the_reported_quota_cannot_cover_in_time_is_refused() {
    let server = MockServer::builder().start().await;
    let reset = epoch_secs() + 3600;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(answer(1, reset))
        .expect(1)
        .mount(&server)
        .await;
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig::octokit()));
    let (transport, _) = executor_transport(&server, budget, short_retry());
    let context = with_token("t", Duration::from_secs(5), 1 << 20);
    assert!(transport.code_search_batch_refusal(2, &context).is_none());
    search(&transport, &context).await.expect("first");
    let refusal = transport
        .code_search_batch_refusal(2, &context)
        .expect("one left, the second waits an hour");
    assert_eq!(refusal.kind, ProviderErrorKind::RateLimited);
}
