//! A cold concurrent wave waits for one cache fill, preserving every caller's evidence.
use super::*;

#[tokio::test]
async fn cacheable_cold_wave_sends_once_and_records_every_answer() {
    let server = MockServer::start().await;
    let body = "complete α evidence\nsecond line\n";
    Mock::given(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "private, max-age=60")
                .set_body_string(body),
        )
        .mount(&server)
        .await;
    let (transport, endpoint, _) = keeping(&server);
    let spec = RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap());
    let contexts = (0..5).map(|_| request("same-token")).collect::<Vec<_>>();
    let pages = futures_util::future::join_all(
        contexts
            .iter()
            .map(|context| transport.execute(spec.clone(), context)),
    )
    .await;
    for (page, context) in pages.into_iter().zip(&contexts) {
        let page = page.expect("each caller gets its complete answer");
        assert_eq!(page.body.as_ref(), body.as_bytes());
        assert_eq!(context.evidence().answers.len(), 1);
        assert_eq!(
            context.evidence().answers[0].body.as_bytes(),
            body.as_bytes()
        );
    }
    assert_eq!(requests(&server).await.len(), 1, "one physical cache fill");
}

/// Followers of an uncacheable or failed fill must reach the origin together,
/// before any follower response arrives. The held response is only a test probe.
#[tokio::test]
async fn unusable_fill_releases_followers_without_a_network_convoy() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    for (status, policy) in [(200, "no-store"), (200, "max-age=0"), (503, "no-store")] {
        let server = MockServer::start().await;
        let arrivals = Arc::new(AtomicUsize::new(0));
        let arrived = Arc::new(tokio::sync::Notify::new());
        let (count, notify) = (arrivals.clone(), arrived.clone());
        Mock::given(path("/api/v3/repos/a/b"))
            .respond_with(move |_: &Request| {
                let index = count.fetch_add(1, Ordering::SeqCst);
                notify.notify_one();
                ResponseTemplate::new(status)
                    .insert_header("cache-control", policy)
                    .set_body_string("complete response")
                    .set_delay(if index == 0 {
                        Duration::ZERO
                    } else {
                        Duration::from_secs(60)
                    })
            })
            .mount(&server)
            .await;
        let endpoint = mock_endpoint(&server);
        let mut transport = GitHubTransport::new(
            endpoint.clone(),
            RetryPolicy {
                max_attempts: 1,
                base_delay: Duration::ZERO,
            },
            GitHubBudget::relaxed(),
        )
        .unwrap();
        transport.set_saved_answers(Some(Arc::new(Memory::default())));
        let spec = RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap());
        let contexts = (0..5).map(|_| request("same-token")).collect::<Vec<_>>();
        let calls = futures_util::future::join_all(
            contexts
                .iter()
                .map(|context| transport.execute(spec.clone(), context)),
        );
        tokio::pin!(calls);
        let all_arrived = async {
            while arrivals.load(Ordering::SeqCst) < contexts.len() {
                arrived.notified().await;
            }
        };
        tokio::select! {
            () = all_arrived => {},
            results = &mut calls => panic!("followers did not overlap for {status}/{policy}: {results:?}"),
        }
        for context in &contexts {
            context.budget.cancellation.cancel();
        }
        let _ = calls.await;
        assert_eq!(arrivals.load(Ordering::SeqCst), contexts.len());
    }
}

#[tokio::test]
async fn distinct_credentials_and_media_types_fill_independently() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v3/repos/a/b"))
        .respond_with(|request: &Request| {
            let auth = request.headers["authorization"].to_str().unwrap();
            let media = request.headers["accept"].to_str().unwrap();
            ResponseTemplate::new(200)
                .insert_header("cache-control", "private, max-age=60")
                .set_body_string(format!("{auth}/{media}"))
        })
        .mount(&server)
        .await;
    let (transport, endpoint, _) = keeping(&server);
    let mut calls = Vec::new();
    for token in ["first", "second"] {
        for media in ["application/json", "text/plain"] {
            let mut spec = RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap());
            spec.headers.insert("accept", media.parse().unwrap());
            calls.push((spec, request(token), format!("Bearer {token}/{media}")));
        }
    }
    let results =
        futures_util::future::join_all(calls.iter().map(|(spec, context, expected)| async {
            let page = transport.execute(spec.clone(), context).await.unwrap();
            assert_eq!(page.body.as_ref(), expected.as_bytes());
        }))
        .await;
    assert_eq!(results.len(), calls.len());
    assert_eq!(requests(&server).await.len(), calls.len());
}

#[tokio::test]
async fn cancelled_follower_does_not_cancel_leader_and_cancelled_leader_releases_follower() {
    for cancel_leader in [false, true] {
        let server = MockServer::start().await;
        let arrived = Arc::new(tokio::sync::Notify::new());
        let notify = arrived.clone();
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = count.clone();
        Mock::given(path("/api/v3/repos/a/b"))
            .respond_with(move |_: &Request| {
                let index = seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                notify.notify_one();
                ResponseTemplate::new(200)
                    .insert_header("cache-control", "private, max-age=60")
                    .set_body_string("complete")
                    .set_delay(if index == 0 {
                        Duration::from_secs(60)
                    } else {
                        Duration::ZERO
                    })
            })
            .mount(&server)
            .await;
        let (transport, endpoint, _) = keeping(&server);
        let spec = RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap());
        let leader = request("same-token");
        let follower = request("same-token");
        let leading = transport.execute(spec.clone(), &leader);
        tokio::pin!(leading);
        tokio::select! {
            () = arrived.notified() => {},
            result = &mut leading => panic!("leader answered before arrival: {result:?}"),
        }
        let following = transport.execute(spec.clone(), &follower);
        tokio::pin!(following);
        assert!(futures_util::poll!(&mut following).is_pending());
        if cancel_leader {
            leader.budget.cancellation.cancel();
            assert_eq!(
                leading.await.unwrap_err().kind,
                ProviderErrorKind::Cancelled
            );
            assert_eq!(following.await.unwrap().body.as_ref(), b"complete");
            assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
        } else {
            follower.budget.cancellation.cancel();
            assert_eq!(
                following.await.unwrap_err().kind,
                ProviderErrorKind::Cancelled
            );
            assert!(!leader.budget.cancellation.is_cancelled());
            assert!(futures_util::poll!(&mut leading).is_pending());
            leader.budget.cancellation.cancel();
            assert_eq!(
                leading.await.unwrap_err().kind,
                ProviderErrorKind::Cancelled
            );
        }
        // A later wave is not trapped behind a dropped leader or follower.
        assert_eq!(
            transport
                .execute(spec, &request("same-token"))
                .await
                .unwrap()
                .body
                .as_ref(),
            b"complete"
        );
    }
}

#[tokio::test]
async fn conditional_fill_confirms_or_replaces_the_complete_answer_once() {
    for status in [200, 304] {
        let server = MockServer::start().await;
        Mock::given(path("/api/v3/repos/a/b"))
            .respond_with(move |request: &Request| {
                if request.headers.contains_key("if-none-match") {
                    assert_eq!(request.headers["if-none-match"], "\"v1\"");
                    ResponseTemplate::new(status)
                        .insert_header("cache-control", "private, max-age=60")
                        .insert_header("etag", "\"v2\"")
                        .set_body_string(if status == 200 { "new α evidence" } else { "" })
                } else {
                    ResponseTemplate::new(200)
                        .insert_header("cache-control", "max-age=0")
                        .insert_header("etag", "\"v1\"")
                        .set_body_string("old α evidence")
                }
            })
            .mount(&server)
            .await;
        let (transport, endpoint, _) = keeping(&server);
        let spec = RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap());
        transport
            .execute(spec.clone(), &request("token"))
            .await
            .unwrap();
        let contexts = (0..5).map(|_| request("token")).collect::<Vec<_>>();
        let pages = futures_util::future::join_all(
            contexts
                .iter()
                .map(|context| transport.execute(spec.clone(), context)),
        )
        .await;
        let expected = if status == 200 {
            "new α evidence"
        } else {
            "old α evidence"
        };
        for (page, context) in pages.into_iter().zip(&contexts) {
            assert_eq!(page.unwrap().body.as_ref(), expected.as_bytes());
            assert_eq!(
                context.evidence().answers[0].body.as_bytes(),
                expected.as_bytes()
            );
        }
        assert_eq!(
            requests(&server).await.len(),
            2,
            "one seed and one validation"
        );
    }
}

#[tokio::test]
async fn each_caller_keeps_its_body_limit_after_a_successful_or_failed_fill() {
    for limits in [[1, 1024], [1024, 1]] {
        let server = MockServer::start().await;
        Mock::given(path("/api/v3/repos/a/b"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("cache-control", "private, max-age=60")
                    .set_body_string("complete α body"),
            )
            .mount(&server)
            .await;
        let (transport, endpoint, _) = keeping(&server);
        let spec = RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap());
        let contexts = limits.map(|limit| with_token("token", Duration::from_secs(5), limit));
        let pages = futures_util::future::join_all(
            contexts
                .iter()
                .map(|context| transport.execute(spec.clone(), context)),
        )
        .await;
        for (page, limit) in pages.into_iter().zip(limits) {
            if limit == 1 {
                assert_eq!(page.unwrap_err().kind, ProviderErrorKind::ResponseTooLarge);
            } else {
                assert_eq!(page.unwrap().body.as_ref(), "complete α body".as_bytes());
            }
        }
        assert_eq!(requests(&server).await.len(), 2);
    }
}

/// Identical searches asked together share the answer GitHub gave the first,
/// though `no-cache` forbids keeping it for later: they were asked at once,
/// so it is their answer too (a batch with a duplicate search row sends one).
/// A later search asks again.
#[tokio::test]
async fn identical_no_cache_searches_asked_together_share_one_answer() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v3/search/repositories"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "no-cache")
                .set_body_string("{\"items\":[]}")
                .set_delay(Duration::from_millis(200)),
        )
        .mount(&server)
        .await;
    let (transport, endpoint, _) = keeping(&server);
    let spec = RequestSpec::get(endpoint.rest(&["search", "repositories"]).unwrap());
    let contexts = (0..3).map(|_| request("same-token")).collect::<Vec<_>>();
    let pages = futures_util::future::join_all(
        contexts
            .iter()
            .map(|context| transport.execute(spec.clone(), context)),
    )
    .await;
    for (page, context) in pages.into_iter().zip(&contexts) {
        assert_eq!(page.expect("answer").body.as_ref(), b"{\"items\":[]}");
        assert_eq!(
            context.evidence().answers.len(),
            1,
            "each caller's evidence"
        );
    }
    assert_eq!(
        requests(&server).await.len(),
        1,
        "asked together, sent once"
    );
    transport
        .execute(spec, &request("same-token"))
        .await
        .expect("later search");
    assert_eq!(
        requests(&server).await.len(),
        2,
        "a later search asks again"
    );
}

/// Identical GraphQL reads of one pull request asked together send once.
#[tokio::test]
async fn identical_pull_request_graphql_reads_asked_together_send_once() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("{\"data\":{\"repository\":{\"pullRequest\":{\"number\":7}}}}")
                .set_delay(Duration::from_millis(200)),
        )
        .mount(&server)
        .await;
    let (transport, _, _) = keeping(&server);
    let query =
        "query($n:Int!){ repository(owner:\"a\",name:\"b\"){ pullRequest(number:$n){ number } } }";
    let contexts = (0..2).map(|_| request("same-token")).collect::<Vec<_>>();
    let pages = futures_util::future::join_all(contexts.iter().map(|context| {
        transport.execute_pull_request_graphql(query, serde_json::json!({"n": 7}), context)
    }))
    .await;
    for page in pages {
        let page = page.expect("answer");
        assert_eq!(
            page.data.expect("data")["repository"]["pullRequest"]["number"],
            7
        );
    }
    assert_eq!(requests(&server).await.len(), 1);
}
