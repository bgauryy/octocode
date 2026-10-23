//! Secondary limits and provider retry-after: cooldown waits, their reset, and queued requests.
use super::*;

#[tokio::test]
async fn secondary_limit_without_retry_after_waits_sixty_seconds_or_fails_fast() {
    let server = MockServer::builder().start().await;
    let limited = Sequence::new(vec![secondary_limit(None), ok_json()]);
    let hits = limited.arrivals.clone();
    Mock::given(method("GET"))
        .respond_with(limited)
        .expect(1)
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let context = RequestContext::with_timeout(Duration::from_secs(5), 1024);
    let error = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["search", "issues"]).expect("route")),
            &context,
        )
        .await
        .expect_err("60s fallback exceeds the 5s cap");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert!(error.retryable);
    assert_eq!(error.status, Some(403));
    // A 403 secondary limit keeps its rate-limit metadata.
    let rate = error.rate_limit.expect("secondary metadata");
    assert_eq!(rate.retry_after_seconds, Some(60));
    assert_eq!(rate.remaining, Some(21));
    // The cooldown is shared by the key: a core call is not sent either.
    let blocked = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).expect("route")),
            &context,
        )
        .await
        .expect_err("cooldown");
    assert_eq!(blocked.kind, ProviderErrorKind::RateLimited);
    assert_eq!(hits.count(), 1);
}

/// A row queued behind the search spacing must re-check the block
/// that landed while it slept, not send into a 120 s secondary cooldown.
#[tokio::test]
async fn a_queued_request_is_not_sent_after_a_cooldown_lands_while_it_waits() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(secondary_limit_response(Some("120")))
        .mount(&server)
        .await;
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig {
        search_spacing: Duration::from_millis(300),
        ..ExecutorConfig::relaxed()
    }));
    let (transport, endpoint) = executor_transport(&server, budget, short_retry());
    let rows = futures_util::future::join_all((0..2).map(|_| {
        let transport = transport.clone();
        let endpoint = endpoint.clone();
        async move { get_status(&transport, &endpoint, &["search", "issues"]).await }
    }))
    .await;
    for row in &rows {
        let error = row.as_ref().expect_err("cooldown");
        assert_eq!(error.kind, ProviderErrorKind::RateLimited);
        assert_eq!(
            error
                .rate_limit
                .as_ref()
                .and_then(|r| r.retry_after_seconds)
                .map(|s| s >= 100),
            Some(true)
        );
    }
    assert_eq!(
        server.received_requests().await.expect("requests").len(),
        1,
        "the queued row was sent into the cooldown"
    );
}

/// A secondary limit's `resetEpochSeconds` is the cooldown end, not
/// the unrelated primary window reset an hour away.
#[tokio::test]
async fn secondary_limit_reset_is_the_cooldown_end() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(secondary_limit_response(Some("120")))
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let before = epoch_secs();
    let error = get_status(&transport, &endpoint, &["repos", "a", "b"])
        .await
        .expect_err("secondary");
    let rate = error.rate_limit.expect("rate limit");
    assert_eq!(rate.retry_after_seconds, Some(120));
    let reset = rate.reset_epoch_seconds.expect("reset");
    assert!(
        (before + 119..=before + 123).contains(&reset),
        "reset {reset} is not the cooldown end near {}",
        before + 120
    );
}

/// A 5xx `Retry-After` beyond the retry cap is surfaced, not dropped.
#[tokio::test]
async fn a_long_server_retry_after_is_surfaced() {
    let server = MockServer::builder().start().await;
    let unavailable = Sequence::new(vec![
        ResponseTemplate::new(503).insert_header("retry-after", "30"),
    ]);
    let hits = unavailable.arrivals.clone();
    Mock::given(method("GET"))
        .respond_with(unavailable)
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let error = get_status(&transport, &endpoint, &["repos", "a", "b"])
        .await
        .expect_err("503");
    assert_eq!(error.kind, ProviderErrorKind::Server);
    assert_eq!(hits.count(), 1, "a long retry-after fails fast");
    assert_eq!(
        error.rate_limit.and_then(|rate| rate.retry_after_seconds),
        Some(30)
    );
}

/// The legacy abuse-detection 403 is a secondary limit.
#[tokio::test]
async fn an_abuse_detection_403_is_a_secondary_limit() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
            "message": "You have triggered an abuse detection mechanism. Please wait a few minutes before you try again."
        })))
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let error = get_status(&transport, &endpoint, &["repos", "a", "b"])
        .await
        .expect_err("limited");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert_eq!(
        error.message.as_ref(),
        "GitHub secondary rate limit exceeded"
    );
}

/// An HTTP-date `Retry-After` is honored instead of the 60 s default. The
/// date is whole seconds from now, so it is far enough ahead that a loaded
/// machine delaying the response by seconds still reads it, not 0 or 60;
/// `octocode_io::retry::tests` pins the exact arithmetic.
#[tokio::test]
async fn an_http_date_retry_after_is_honored() {
    let server = MockServer::builder().start().await;
    let date = httpdate_after(30);
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", date.as_str())
                .set_body_json(serde_json::json!({"message": "Too Many Requests"})),
        )
        .mount(&server)
        .await;
    let single = RetryPolicy {
        max_attempts: 1,
        ..short_retry()
    };
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), single);
    let error = get_status(&transport, &endpoint, &["repos", "a", "b"])
        .await
        .expect_err("limited");
    let seconds = error
        .rate_limit
        .and_then(|rate| rate.retry_after_seconds)
        .expect("retry after");
    assert!((20..=31).contains(&seconds), "{seconds} is not ~30 s");
}

/// An RFC 9110 IMF-fixdate `seconds` from now.
fn httpdate_after(seconds: u64) -> String {
    let at = epoch_secs() + seconds;
    let days = (at / 86_400) as i64;
    let (year, month, day) = civil_from_days(days);
    let rem = at % 86_400;
    let weekday = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][(days % 7) as usize];
    let month_name = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][(month - 1) as usize];
    format!(
        "{weekday}, {day:02} {month_name} {year} {:02}:{:02}:{:02} GMT",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}
