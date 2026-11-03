//! Concurrent reads retain their own request and cancellation.
use super::*;

/// Identical concurrent reads each fetch the current response.
#[tokio::test]
async fn concurrent_identical_reads_each_send_a_request() {
    let server = MockServer::builder().start().await;
    let sha = "1111111111111111111111111111111111111111";
    mount_file(
        &server,
        "/api/v3/repos/a/b/contents/flight.rs",
        Duration::from_millis(150),
    )
    .await;
    let provider = provider(&server).await;
    let context = with_token("secret", Duration::from_secs(5), 1024);
    let request = file_at("flight.rs", sha);
    let (first, second) = tokio::join!(
        provider.get_file_bytes(&request, &context),
        provider.get_file_bytes(&request, &context),
    );
    let (first, second) = (first.expect("first"), second.expect("second"));
    assert_eq!(first.bytes, second.bytes);
    assert_eq!(requests_to(&server, "/api/v3/repos/a/b/contents").await, 2);
}

/// A concurrent content or ref read stops on its own cancellation.
#[tokio::test]
async fn a_cancelled_read_stops_without_cancelling_a_concurrent_read() {
    // The holders' answers outlast any runner's stall: each waiter must
    // answer while its holder's request is still at the server, and the
    // test then cancels the holder instead of waiting for the answer.
    const HELD: Duration = Duration::from_secs(60);
    let server = MockServer::builder().start().await;
    let sha = "4444444444444444444444444444444444444444";
    let arrived = Arc::new(tokio::sync::Notify::new());
    let held = |template: ResponseTemplate| Delayed {
        arrived: Some(Arc::clone(&arrived)),
        ..Delayed::new(HELD, template)
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/held.rs"))
        .respond_with(held(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .set_body_json(serde_json::json!({
                    "type":"file","encoding":"base64","content":STANDARD.encode("body\n")})),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/held-ref"))
        .respond_with(held(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"sha":sha})),
        ))
        .mount(&server)
        .await;
    let provider = provider(&server).await;
    let request = file_at("held.rs", sha);
    let call = |call: &'static str, context: RequestContext| {
        let (provider, request) = (&provider, &request);
        async move {
            match call {
                "content" => provider.get_file_bytes(request, &context).await.map(drop),
                _ => provider
                    .resolve_reference("a", "b", Some("held-ref"), &context)
                    .await
                    .map(drop),
            }
        }
    };
    for name in ["content", "ref"] {
        let holder = with_token("secret", Duration::from_secs(10), 1024);
        let release_holder = holder.budget.cancellation.clone();
        let mut holding = std::pin::pin!(call(name, holder));
        // Cancel the second read only after both requests reached the server.
        let waiting = async {
            arrived.notified().await;
            let context = with_token("secret", Duration::from_secs(10), 1024);
            let cancellation = context.budget.cancellation.clone();
            let cancel = async {
                arrived.notified().await;
                cancellation.cancel();
            };
            let (result, ()) = tokio::join!(call(name, context), cancel);
            result.expect_err("cancelled").kind
        };
        let waited = tokio::select! {
            kind = waiting => kind,
            answered = &mut holding => panic!("the {name} holder answered first: {answered:?}"),
        };
        assert_eq!(waited, ProviderErrorKind::Cancelled, "{name}");
        release_holder.cancel();
        assert_eq!(
            holding.await.expect_err("the holder is cancelled").kind,
            ProviderErrorKind::Cancelled,
            "{name}"
        );
    }
    // Both reads reached the provider with independent cancellation.
    assert_eq!(requests_to(&server, "/api/v3/repos/a/b/contents").await, 2);
    assert_eq!(requests_to(&server, "/api/v3/repos/a/b/commits").await, 2);
}
