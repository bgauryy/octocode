use super::*;

#[tokio::test]
async fn html_forbidden_preserves_request_without_claiming_the_cause() {
    let server = MockServer::builder().start().await;
    let request = json!({
        "state": {"content": "path is /etc/passwd and ../x"},
        "questions": {"q": {"instructions": "Does this read /etc/passwd?"}}
    });
    Mock::given(method("POST"))
        .and(wiremock::matchers::body_json(request.clone()))
        .respond_with(ResponseTemplate::new(403).set_body_raw(
            "<!DOCTYPE html><html>Access denied: invalid credentials</html>",
            "text/html",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let error = post(
        body(&request),
        reqwest::header::HeaderMap::new(),
        &SecretString::from("test-key".to_owned()),
        endpoint(&server.uri(), "v1/systemone").unwrap(),
        &test_budget(Duration::from_secs(10)),
        3,
        &fresh_gate(4),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::ClassificationProviderError);
    assert!(error.message.contains("403"));
    assert!(error.hints[0].contains("OCTOCODE_CLASSIFICATION_API"));
    assert!(!error.message.contains("firewall"));
    assert!(!error.hints.iter().any(|hint| hint.contains("key is fine")));
    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].body_json::<Value>().unwrap(), request);
}

#[tokio::test]
async fn transport_rejects_redirects_and_oversized_responses() {
    for (response, expected) in [
        (
            ResponseTemplate::new(302).insert_header("location", "https://example.com"),
            ErrorCode::ClassificationProviderError,
        ),
        (
            ResponseTemplate::new(200).set_body_string("x".repeat(65)),
            ErrorCode::InvalidClassificationResponse,
        ),
    ] {
        let server = MockServer::builder().start().await;
        Mock::given(method("POST"))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
        let mut budget = test_budget(Duration::from_secs(30));
        budget.max_body_bytes = 64;
        let error = post(
            body(&json!({})),
            reqwest::header::HeaderMap::new(),
            &SecretString::from("test-key".to_owned()),
            endpoint(&server.uri(), "v1/systemone").unwrap(),
            &budget,
            0,
            &fresh_gate(4),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, expected);
    }
}

#[tokio::test]
async fn expired_deadline_prevents_transport() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let error = send(&server.uri(), Duration::ZERO, 0, &fresh_gate(4))
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Timeout);
}

/// The retry budget bounds the attempts exactly: `retries` retries after
/// the first attempt, then the throttle is reported. Replaces the
/// classification-retry harness cases that varied MAX_RETRIES, now a
/// runtime constant (`RuntimeLimits::max_retries`).
#[tokio::test]
async fn the_retry_budget_bounds_attempts_exactly() {
    for retries in [0, 1, 3] {
        let server = MockServer::builder().start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after-ms", "1"))
            .expect(u64::from(retries) + 1)
            .mount(&server)
            .await;
        let error = send(
            &server.uri(),
            Duration::from_secs(10),
            retries,
            &fresh_gate(4),
        )
        .await
        .unwrap_err();
        assert_eq!(
            error.code,
            ErrorCode::ClassificationRateLimited,
            "{retries}: {error:?}"
        );
        assert_eq!(error.provider_calls, u64::from(retries) + 1, "{retries}");
        server.verify().await;
    }
}

#[test]
fn generic_request_errors_offer_discovery_without_inventing_a_capability_cause() {
    for code in [400, 422] {
        let error = status_error(
            reqwest::StatusCode::from_u16(code).unwrap(),
            Some(
                &json!({"detail":{"error_type":"api_usage_error","message":"private provider detail"}}),
            ),
        );
        assert_eq!(error.code, ErrorCode::ClassificationProviderError);
        assert!(error.message.contains(&code.to_string()));
        assert!(!error.message.contains("private provider detail"));
        assert!(error.hints[0].contains(r#"clasify {"operation":"models"}"#));
        assert!(error.hints[0].contains("state"));
        assert!(error.hints[0].contains("request size"));
        assert!(!error.hints[0].contains("jev-preview"));
        assert!(!error.message.contains("unsupported"));
    }
}

#[tokio::test]
async fn bad_request_is_not_retried_and_maps_the_provider_error_type() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(json!({"detail":{"error_type":"max_tokens_exceeded"}})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let error = send(&server.uri(), Duration::from_secs(10), 3, &fresh_gate(4))
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::ClassificationStateTooLarge);
    assert!(error.hints[0].contains("source ranges"), "{error:?}");
}

#[test]
fn endpoint_accepts_an_https_root_and_appends_the_vendor_path() {
    let url = endpoint("https://api.typesafe.ai", "v1/systemone").expect("valid root");
    assert_eq!(url.as_str(), "https://api.typesafe.ai/v1/systemone");
    // A trailing slash is also a bare root.
    assert!(endpoint("https://api.typesafe.ai/", "v1/systemone").is_ok());
    // HTTP is allowed only on loopback.
    assert!(endpoint("http://127.0.0.1", "v1/systemone").is_ok());
    assert!(endpoint("http://[::1]:8080", "v1/systemone").is_ok());
    assert!(endpoint("http://LOCALHOST", "v1/systemone").is_ok());
}

#[test]
fn endpoint_rejects_non_root_or_insecure_bases() {
    for bad in [
        "http://api.typesafe.ai",            // http, non-loopback
        "http://[2001:db8::1]",              // http, non-loopback IPv6
        "https://api.typesafe.ai/v1",        // has a path
        "https://api.typesafe.ai/?x=1",      // has a query
        "https://api.typesafe.ai/#frag",     // has a fragment
        "https://user:pass@api.typesafe.ai", // has credentials
        "not a url",
    ] {
        assert!(
            endpoint(bad, "v1/systemone").is_err(),
            "should reject {bad}"
        );
    }
}

#[test]
fn retry_delay_floors_zero_and_backoff_uses_full_jitter() {
    assert_eq!(BACKOFF.after(Some(Duration::ZERO), 0), MIN_RETRY_DELAY);
    assert_eq!(
        BACKOFF.after(Some(Duration::from_secs(2)), 5),
        Duration::from_secs(2)
    );
    let mut distinct = std::collections::HashSet::new();
    for attempt in 0..6 {
        let ceiling = BACKOFF.base.saturating_mul(1 << attempt).min(BACKOFF.cap);
        for _ in 0..20 {
            let delay = BACKOFF.after(None, attempt);
            assert!(delay >= MIN_RETRY_DELAY && delay <= ceiling.max(MIN_RETRY_DELAY));
            distinct.insert(delay);
        }
    }
    assert!(distinct.len() > 10, "backoff is not jittered");
    assert!(BACKOFF.after(None, 30) <= BACKOFF.cap);
}
