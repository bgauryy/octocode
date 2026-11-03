//! Search pacing follows the quota GitHub reports: planned searches it covers
//! burst, and only the excess is spread until reset. No client-side constant
//! decides it.
use super::*;
use crate::budget::now_ms;

/// A search answer reporting the `remaining` quota and the window's `reset`
/// (epoch seconds).
fn quota_response(remaining: u64, reset: u64) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("x-ratelimit-remaining", remaining.to_string().as_str())
        .insert_header("x-ratelimit-reset", reset.to_string().as_str())
        .insert_header("x-ratelimit-resource", "search")
        .set_body_json(serde_json::json!({"total_count": 0, "items": []}))
}

async fn mount_search(server: &MockServer, responder: impl wiremock::Respond + 'static) {
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(responder)
        .mount(server)
        .await;
}

async fn search(transport: &GitHubTransport, endpoint: &GitHubEndpoint) {
    transport
        .execute(
            RequestSpec::get(endpoint.rest(&["search", "issues"]).expect("route")),
            &with_token("t", Duration::from_secs(30), 1024),
        )
        .await
        .expect("search");
}

/// Before any quota is reported, a search lane holds one request in flight:
/// the next waits for the first answer. An answer that reports no quota adds
/// no client-invented spacing on top.
#[tokio::test]
async fn an_unreported_quota_holds_the_lane_only_until_the_first_answer() {
    let server = MockServer::builder().start().await;
    let answer = Duration::from_millis(400);
    let answers = Delayed::new(
        answer,
        ResponseTemplate::new(200).set_body_json(serde_json::json!({"items": []})),
    );
    let arrivals = answers.arrivals.clone();
    mount_search(&server, answers).await;
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig::octokit()));
    let (transport, endpoint) = executor_transport(&server, budget, short_retry());
    futures_util::future::join(search(&transport, &endpoint), search(&transport, &endpoint)).await;
    let times = arrivals.times();
    assert_eq!(times.len(), 2);
    let gap = times[1].duration_since(times[0]);
    assert!(
        gap >= answer && gap < answer * 2,
        "second search must wait for the first answer and nothing more: {gap:?}"
    );
}

/// Ten planned searches covered by ten reported remaining go back-to-back;
/// none waits for the even-spreading interval the old pacing derived.
#[tokio::test]
async fn quota_covered_searches_go_back_to_back() {
    let server = MockServer::builder().start().await;
    let reset = epoch_secs() + 3;
    let answers = Sequence::new(vec![quota_response(10, reset)]);
    let arrivals = answers.arrivals.clone();
    mount_search(&server, answers).await;
    let config = ExecutorConfig::octokit();
    let budget = Arc::new(GitHubBudget::with_config(config.clone()));
    let (transport, endpoint) = executor_transport(&server, budget, short_retry());
    search(&transport, &endpoint).await;
    let started = now_ms();
    for _ in 0..10 {
        search(&transport, &endpoint).await;
    }
    let times = arrivals.times();
    assert_eq!(times.len(), 11);
    let even_spacing =
        Duration::from_millis(config.reset_until_ms(reset).saturating_sub(started) / 10);
    for gap in times[1..]
        .windows(2)
        .map(|pair| pair[1].duration_since(pair[0]))
    {
        assert!(
            gap * 2 < even_spacing,
            "a quota-covered search waited {gap:?}; even spreading would allow about {even_spacing:?}"
        );
    }
}

/// A secondary limit's `retry-after` outlasts the spacing the reported quota
/// derives: the retry waits for the cooldown, not the shorter spacing.
#[tokio::test]
async fn retry_after_wins_over_the_quota_spacing() {
    let server = MockServer::builder().start().await;
    // Plenty of quota remains: retry-after still holds the next attempt.
    let answers = Sequence::new(vec![
        secondary_limit_response(Some("3")),
        quota_response(3999, epoch_secs() + 3600),
    ]);
    let arrivals = answers.arrivals.clone();
    mount_search(&server, answers).await;
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig::octokit()));
    let (transport, endpoint) = executor_transport(&server, budget, short_retry());
    search(&transport, &endpoint).await;
    let times = arrivals.times();
    assert_eq!(times.len(), 2, "one secondary limit, one retry");
    let gap = times[1].duration_since(times[0]);
    assert!(
        gap >= Duration::from_secs(3),
        "retry before retry-after: {gap:?}"
    );
}
