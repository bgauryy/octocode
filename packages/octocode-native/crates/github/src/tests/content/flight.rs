//! Single flight: identical concurrent reads and a cancelled waiter.
use super::*;

/// Identical concurrent reads take turns on one flight: the first fetches,
/// the others read what it cached.
#[tokio::test]
async fn concurrent_identical_reads_send_one_request() {
    let server = MockServer::builder().start().await;
    let sha = "1111111111111111111111111111111111111111";
    mount_file(
        &server,
        "/api/v3/repos/a/b/contents/flight.rs",
        Duration::from_millis(150),
    )
    .await;
    let provider = keyed_provider(&server).await;
    let context = with_token("secret", Duration::from_secs(5), 1024);
    let request = file_at("flight.rs", sha);
    let (first, second) = tokio::join!(
        provider.get_file_bytes(&request, &context),
        provider.get_file_bytes(&request, &context),
    );
    let (first, second) = (first.expect("first"), second.expect("second"));
    assert_eq!(first.bytes, second.bytes);
    assert!(
        first.from_cache != second.from_cache,
        "one read is the cached one"
    );
    assert_eq!(requests_to(&server, "/api/v3/repos/a/b/contents").await, 1);
}

/// A waiter on an identical read's or ref resolution's flight stops on its
/// own cancellation, while the first request is still in flight.
#[tokio::test]
async fn a_cancelled_flight_waiter_stops_before_the_holder_finishes() {
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
    let provider = keyed_provider(&server).await;
    let request = file_at("held.rs", sha);
    let call = |call: &'static str, context: RequestContext| {
        let (provider, request) = (&provider, &request);
        async move {
            match call {
                "content" => provider.get_file_bytes(request, &context).await.map(drop),
                _ => provider
                    .resolve_reference("a", "b", Some("held-ref"), false, &context)
                    .await
                    .map(drop),
            }
        }
    };
    for name in ["content", "ref"] {
        let holder = with_token("secret", Duration::from_secs(10), 1024);
        let release_holder = holder.budget.cancellation.clone();
        let mut holding = std::pin::pin!(call(name, holder));
        // The waiter joins once the holder's request reached the server,
        // so the holder has the turn; then it is cancelled while waiting.
        let waiting = async {
            arrived.notified().await;
            let context = with_token("secret", Duration::from_secs(10), 1024);
            let cancellation = context.budget.cancellation.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                cancellation.cancel();
            });
            call(name, context).await.expect_err("cancelled").kind
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
    // Neither waiter sent a request of its own.
    assert_eq!(requests_to(&server, "/api/v3/repos/a/b/contents").await, 1);
    assert_eq!(requests_to(&server, "/api/v3/repos/a/b/commits").await, 1);
}
