//! B91: a request discloses the client-side waits it spent before its sends,
//! summed by kind ([`RequestContext::client_waits`]), and only its own.
use super::*;
use octocode_contract_types::ClientWaitKind;
use std::time::Instant;

/// Milliseconds `context` waited of `kind`.
fn waited(context: &RequestContext, kind: ClientWaitKind) -> i64 {
    context
        .client_waits()
        .iter()
        .filter(|wait| wait.kind == kind)
        .map(|wait| wait.ms)
        .sum()
}

/// A 403 for a spent search quota; `reset` (epoch seconds) when GitHub sent
/// one. Search is the quota a request waits out: a spent core quota is
/// disclosed at once instead (`primary.rs`).
fn spent_quota(reset: Option<u64>) -> ResponseTemplate {
    let response = ResponseTemplate::new(403)
        .insert_header("x-ratelimit-remaining", "0")
        .insert_header("x-ratelimit-resource", "search")
        .set_body_json(serde_json::json!({"message": "API rate limit exceeded for user."}));
    match reset {
        Some(reset) => response.insert_header("x-ratelimit-reset", reset.to_string().as_str()),
        None => response,
    }
}

#[tokio::test]
async fn a_secondary_limit_cooldown_is_disclosed_as_secondary_limit() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(Sequence::new(vec![secondary_limit(Some("0.3")), ok_json()]))
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let context = with_token("cooled", Duration::from_secs(5), 1024);
    transport
        .execute(
            RequestSpec::get(endpoint.rest(&["search", "issues"]).expect("route")),
            &context,
        )
        .await
        .expect("retried after the cooldown");
    let waits = context.client_waits();
    assert_eq!(waits.len(), 1, "one kind: {waits:?}");
    assert_eq!(waits[0].kind, ClientWaitKind::SecondaryLimit);
    assert!(waits[0].ms >= 200, "the cooldown it sat out: {waits:?}");
}

#[tokio::test]
async fn a_spent_quota_wait_is_disclosed_as_quota_pacing() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(Sequence::new(vec![
            spent_quota(Some(epoch_secs() + 2)),
            ok_json(),
        ]))
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let context = with_token("spent", Duration::from_secs(10), 1024);
    transport
        .execute(
            RequestSpec::get(endpoint.rest(&["search", "issues"]).expect("route")),
            &context,
        )
        .await
        .expect("retried after the reset");
    assert!(
        waited(&context, ClientWaitKind::QuotaPacing) >= 500,
        "{:?}",
        context.client_waits()
    );
    assert_eq!(
        context.client_waits().len(),
        1,
        "{:?}",
        context.client_waits()
    );
}

/// A fixed start spacing (here GraphQL's) is disclosed on the request it held
/// back, never on the one that went first.
#[tokio::test]
async fn a_fixed_spacing_is_disclosed_on_the_spaced_request_alone() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ok_json())
        .mount(&server)
        .await;
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig {
        graphql_spacing: Duration::from_millis(400),
        ..ExecutorConfig::relaxed()
    }));
    let (transport, endpoint) = executor_transport(&server, budget, short_retry());
    let query = || {
        RequestSpec::graphql(
            endpoint.graphql(),
            serde_json::json!({"query": "{viewer{login}}"}),
        )
    };
    let first = with_token("spaced", Duration::from_secs(5), 1024);
    let second = with_token("spaced", Duration::from_secs(5), 1024);
    transport.execute(query(), &first).await.expect("first");
    transport.execute(query(), &second).await.expect("second");
    assert_eq!(
        first.client_waits(),
        Vec::new(),
        "the first start waits for nothing"
    );
    assert!(
        waited(&second, ClientWaitKind::Spacing) >= 250,
        "{:?}",
        second.client_waits()
    );
}

/// A quota-paced lane holds a search until the one ahead reports its quota:
/// the held request discloses quota pacing, the one ahead nothing.
#[tokio::test]
async fn a_search_held_for_its_lane_is_disclosed_as_quota_pacing() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(Delayed::new(
            Duration::from_millis(400),
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"items": []})),
        ))
        .mount(&server)
        .await;
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig::octokit()));
    let (transport, endpoint) = executor_transport(&server, budget, short_retry());
    let search = || RequestSpec::get(endpoint.rest(&["search", "issues"]).expect("route"));
    let (one, two) = (
        with_token("lane", Duration::from_secs(10), 1024),
        with_token("lane", Duration::from_secs(10), 1024),
    );
    let (a, b) = futures_util::future::join(
        transport.execute(search(), &one),
        transport.execute(search(), &two),
    )
    .await;
    a.expect("one");
    b.expect("two");
    let mut paced = [
        waited(&one, ClientWaitKind::QuotaPacing),
        waited(&two, ClientWaitKind::QuotaPacing),
    ];
    paced.sort_unstable();
    assert_eq!(
        paced[0], 0,
        "the search ahead waited for nothing: {paced:?}"
    );
    assert!(paced[1] >= 300, "the held search: {paced:?}");
    for context in [&one, &two] {
        assert!(
            context
                .client_waits()
                .iter()
                .all(|wait| wait.kind == ClientWaitKind::QuotaPacing),
            "{:?}",
            context.client_waits()
        );
    }
}

/// B75 condition 2: a spent quota GitHub gives no reset for still waits
/// under the same disclosure (the host hears the limit and the wait), and a
/// cancel ends that wait at once, though no caller deadline bounds it.
#[tokio::test]
async fn a_spent_quota_without_a_reset_is_disclosed_and_a_cancel_ends_its_wait() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(spent_quota(None))
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let mut context = with_token("no-reset", Duration::from_secs(5), 1024);
    // No caller budget: the wait may run as long as GitHub's limit holds.
    context.budget.wait_until = Instant::now() + Duration::from_secs(3600);
    let heard = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&heard);
    let scope = octocode_io::notice::Scope::with_progress(move |line| {
        sink.lock().expect("lines").push(line.to_owned());
    });
    let cancellation = context.budget.cancellation.clone();
    let cancelled_at = Arc::new(std::sync::Mutex::new(None::<Instant>));
    let at = Arc::clone(&cancelled_at);
    let lines = Arc::clone(&heard);
    tokio::spawn(async move {
        // Cancel once the wait is underway and the host was told about it.
        while lines.lock().expect("lines").is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        *at.lock().expect("at") = Some(Instant::now());
        cancellation.cancel();
    });
    let error = scope
        .run(transport.execute(
            RequestSpec::get(endpoint.rest(&["search", "issues"]).expect("route")),
            &context,
        ))
        .await
        .expect_err("cancelled mid-wait");
    let ended = Instant::now();
    let cancelled = cancelled_at
        .lock()
        .expect("at")
        .expect("the wait was cancelled");
    assert!(
        ended.duration_since(cancelled) < Duration::from_secs(1),
        "the cancel ended the wait at once: {:?}",
        ended.duration_since(cancelled)
    );
    assert_eq!(error.kind, ProviderErrorKind::Cancelled, "{error:?}");
    let lines = heard.lock().expect("lines").clone();
    assert!(
        lines[0].contains("GitHub search limit reached: waiting"),
        "the wait is disclosed like one with a reset: {lines:?}"
    );
    assert!(
        waited(&context, ClientWaitKind::QuotaPacing) >= 250,
        "{:?}",
        context.client_waits()
    );
}

/// B91 r2: a row that read the batch warm-up's answer waited for the warm-up's
/// admission: those waits are added to the row, once however often it reads,
/// beside the row's own.
#[tokio::test]
async fn a_row_reading_the_warm_up_carries_its_admission_waits_once() {
    let server = MockServer::builder().start().await;
    let (transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let provider = crate::GitHubProvider::new(transport);
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let memo = std::sync::Arc::new(crate::LastCommitMemo::new());
    let warm = with_token("warm", Duration::from_secs(5), 1024);
    warm.waits
        .record(ClientWaitKind::QuotaPacing, Duration::from_millis(300));
    memo.record_ref("acme", "repo", Some("main"), Ok(sha.to_owned()));
    memo.observed_by(&warm);
    memo.refs_known();
    let row = with_token("warm", Duration::from_secs(5), 1024).with_last_commits(memo);
    row.waits
        .record(ClientWaitKind::Spacing, Duration::from_millis(7));
    for _ in 0..2 {
        let resolved = provider
            .resolve_reference("acme", "repo", Some("main"), &row)
            .await
            .expect("the warm-up's answer");
        assert_eq!(resolved, sha);
    }
    assert_eq!(
        waited(&row, ClientWaitKind::QuotaPacing),
        300,
        "{:?}",
        row.client_waits()
    );
    assert_eq!(
        waited(&row, ClientWaitKind::Spacing),
        7,
        "{:?}",
        row.client_waits()
    );
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty(),
        "the memo answered"
    );
}

/// B91 r2: the backoff before retrying a transient GraphQL error (retried
/// like a 500) is a retry delay the row spent on the client.
#[tokio::test]
async fn a_transient_graphql_backoff_is_disclosed_as_retry_after() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(Sequence::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": null,
                "errors": [{"message": "Something went wrong while executing your query. This may be the result of a timeout."}]
            })),
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"data": {"viewer": {"login": "me"}}})),
        ]))
        .mount(&server)
        .await;
    let retry = RetryPolicy {
        base_delay: Duration::from_millis(200),
        ..short_retry()
    };
    let (transport, _) = executor_transport(&server, GitHubBudget::relaxed(), retry);
    let context = with_token("transient", Duration::from_secs(10), 1024);
    transport
        .execute_graphql(
            "query { viewer { login } }",
            serde_json::json!({}),
            &context,
        )
        .await
        .expect("retried");
    assert!(
        waited(&context, ClientWaitKind::RetryAfter) > 0,
        "{:?}",
        context.client_waits()
    );
}
