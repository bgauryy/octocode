//! Provider retry delays remain whole even when they exceed this call's deadline.
use super::*;

#[tokio::test]
async fn rest_preserves_provider_retry_after_beyond_a_day_without_retrying() {
    for delay in [2 * 24 * 60 * 60, u64::MAX] {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .respond_with(secondary_limit_response(Some(&delay.to_string())))
            .expect(1)
            .mount(&server)
            .await;
        let (transport, endpoint) =
            executor_transport(&server, GitHubBudget::relaxed(), short_retry());
        let context = RequestContext::with_timeout(Duration::from_secs(5), 1024);
        let error = transport
            .execute(
                RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).expect("route")),
                &context,
            )
            .await
            .expect_err("provider delay exceeds the call deadline");
        assert_eq!(error.kind, ProviderErrorKind::RateLimited);
        assert_eq!(
            error.rate_limit.expect("rate limit").retry_after_seconds,
            Some(delay),
            "the provider's delay must not be shortened"
        );
        assert!(!transport.graphql_available(&context));
        let blocked = transport
            .execute(
                RequestSpec::get(endpoint.rest(&["search", "issues"]).expect("route")),
                &context,
            )
            .await
            .expect_err("the shared cooldown must refuse another send");
        assert_eq!(blocked.kind, ProviderErrorKind::RateLimited);
        assert_eq!(server.received_requests().await.expect("requests").len(), 1);
    }
}

#[tokio::test]
async fn graphql_preserves_provider_retry_after_beyond_a_day_without_retrying() {
    for delay in [2 * 24 * 60 * 60, u64::MAX] {
        let server = MockServer::builder().start().await;
        Mock::given(method("POST"))
            .and(path("/api/graphql"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("retry-after", delay.to_string())
                    .set_body_json(serde_json::json!({
                        "data": null,
                        "errors": [{
                            "type": "RATE_LIMITED",
                            "message": "You have exceeded a secondary rate limit."
                        }]
                    })),
            )
            .expect(1)
            .mount(&server)
            .await;
        let (transport, endpoint) =
            executor_transport(&server, GitHubBudget::relaxed(), short_retry());
        let context = RequestContext::with_timeout(Duration::from_secs(5), 1024);
        let error = transport
            .execute_graphql(
                "query { viewer { login } }",
                serde_json::json!({}),
                &context,
            )
            .await
            .expect_err("provider delay exceeds the call deadline");
        assert_eq!(error.kind, ProviderErrorKind::RateLimited);
        assert_eq!(
            error.rate_limit.expect("rate limit").retry_after_seconds,
            Some(delay),
            "the provider's delay must not be shortened"
        );
        let blocked = transport
            .execute(
                RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).expect("route")),
                &context,
            )
            .await
            .expect_err("the shared cooldown must refuse another send");
        assert_eq!(blocked.kind, ProviderErrorKind::RateLimited);
        assert_eq!(server.received_requests().await.expect("requests").len(), 1);
    }
}
