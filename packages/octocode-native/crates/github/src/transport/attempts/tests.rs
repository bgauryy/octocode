use super::*;

#[tokio::test]
async fn an_extreme_provider_wait_is_refused_without_overflow_or_network() {
    let endpoint =
        crate::GitHubEndpoint::new(url::Url::parse("https://api.github.com").expect("API URL"))
            .expect("endpoint");
    let transport = GitHubTransport::new(
        endpoint.clone(),
        super::super::RetryPolicy::default(),
        crate::GitHubBudget::relaxed(),
    )
    .expect("transport");
    let context = RequestContext::with_timeout(Duration::from_secs(5), 1024);
    let mut failed = None;
    let mut attempt = 0;
    let mut followed = Vec::new();
    let mut attempts = Attempts::new(
        &transport,
        RequestSpec::get(endpoint.rest(&[]).expect("route")),
        &context,
        &mut failed,
        &mut attempt,
        &mut followed,
    );
    let refused = attempts
        .retry(
            ProviderError::new(ProviderErrorKind::RateLimited, "provider delay"),
            RetryWait {
                wait: Duration::MAX,
                sleep: Duration::ZERO,
            },
        )
        .await
        .expect_err("provider delay exceeds the call deadline");
    assert_eq!(refused.kind, ProviderErrorKind::RateLimited);

    let state = transport.key_state(None);
    state.cool_down(u64::MAX);
    assert!(!transport.graphql_available(&context));
    let blocked = state
        .wait_unblocked(
            "core",
            transport.budget.config(),
            context.budget.deadline,
            &context.budget.cancellation,
            &context.waits,
        )
        .await
        .expect_err("the shared cooldown exceeds the call deadline");
    assert_eq!(blocked.kind, ProviderErrorKind::RateLimited);
}
