//! Origin cache directives remain authoritative across replacement and revalidation.
use super::*;

#[tokio::test]
async fn validation_required_answers_are_not_served_on_rate_limits() {
    for directive in ["no-cache", "max-age=0, must-revalidate"] {
        let server = MockServer::start().await;
        Mock::given(path("/api/v3/repos/a/b"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("cache-control", directive)
                    .insert_header("etag", "\"v1\"")
                    .set_body_string("old"),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(path("/api/v3/repos/a/b"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("retry-after", "60")
                    .set_body_string("rate limited"),
            )
            .mount(&server)
            .await;
        let (transport, endpoint, _) = keeping(&server);
        let spec = || RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap());
        transport.execute(spec(), &request("t")).await.unwrap();
        let error = transport.execute(spec(), &request("t")).await.unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::RateLimited, "{directive}");
    }
}

#[tokio::test]
async fn redirects_requiring_origin_rechecks_do_not_save_a_rename_route() {
    for (status, cache_control) in [
        (302, None),
        (307, None),
        (301, Some("no-store")),
        (308, Some("no-cache")),
        (301, Some("max-age=0")),
    ] {
        let server = MockServer::start().await;
        let mut redirect = ResponseTemplate::new(status).insert_header(
            "location",
            format!("{}/api/v3/repositories/42", server.uri()),
        );
        if let Some(value) = cache_control {
            redirect = redirect.insert_header("cache-control", value);
        }
        Mock::given(path("/api/v3/repos/a/b"))
            .respond_with(redirect)
            .mount(&server)
            .await;
        Mock::given(path("/api/v3/repositories/42"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;
        let (transport, endpoint, _) = keeping(&server);
        let spec = || RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap());
        for _ in 0..2 {
            transport.execute(spec(), &request("t")).await.unwrap();
        }
        assert_eq!(
            requests(&server)
                .await
                .iter()
                .filter(|request| request.url.path() == "/api/v3/repos/a/b")
                .count(),
            2,
            "redirect {status} with {cache_control:?} must ask the original URL on each fresh request"
        );
    }
}

#[tokio::test]
async fn no_cache_searches_without_validators_always_reach_the_origin() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v3/search/repositories"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "no-cache")
                .set_body_string("{}"),
        )
        .mount(&server)
        .await;
    let (transport, endpoint, _) = keeping(&server);
    let spec = || RequestSpec::get(endpoint.rest(&["search", "repositories"]).unwrap());
    for _ in 0..2 {
        transport.execute(spec(), &request("t")).await.unwrap();
    }
    assert_eq!(requests(&server).await.len(), 2);
}

#[tokio::test]
async fn no_store_replacements_and_confirmations_forget_the_saved_answer() {
    for (status, partial) in [(200, false), (304, false), (200, true)] {
        let server = MockServer::start().await;
        Mock::given(path("/api/v3/repos/a/b"))
            .respond_with(validated("old", "\"v1\""))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(path("/api/v3/repos/a/b"))
            .respond_with(
                ResponseTemplate::new(status)
                    .insert_header("cache-control", "no-store")
                    .set_body_string(if partial {
                        "{\"incomplete_results\":true}"
                    } else if status == 200 {
                        "new"
                    } else {
                        ""
                    }),
            )
            .mount(&server)
            .await;
        let (transport, endpoint, memory) = keeping(&server);
        let spec = || RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap());
        transport.execute(spec(), &request("t")).await.unwrap();
        assert_eq!(memory.len(), 1);
        let next = transport.execute(spec(), &request("t")).await.unwrap();
        if !partial {
            assert_eq!(
                next.body.as_ref(),
                if status == 200 { b"new" } else { b"old" }
            );
        }
        assert_eq!(memory.len(), 0, "status {status} revoked cache retention");
    }
}
