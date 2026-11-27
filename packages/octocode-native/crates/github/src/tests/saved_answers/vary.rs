//! Wildcard Vary never identifies a representation reusable for another request.
use super::*;

fn saved_with(headers: &[(&str, &str)]) -> SavedAnswer {
    SavedAnswer {
        answer: ProviderAnswer {
            request: "GET x".into(),
            status: 200,
            headers: headers
                .iter()
                .map(|(k, v)| ((*k).into(), (*v).into()))
                .collect(),
            body: "old".into(),
            next: None,
            not_found: None,
        },
        observed_ms: 1_000_000,
        redirected: false,
    }
}

#[test]
fn vary_star_persisted_answers_never_reuse_or_serve_stale() {
    for vary in ["*", "Accept, *", " Accept ,\t*\t, X-Variant "] {
        let saved = saved_with(&[
            ("vary", "Accept-Encoding"),
            ("VaRy", vary),
            ("cache-control", "private, max-age=60"),
            ("etag", "\"old\""),
        ]);
        for pinned in [false, true] {
            for now in [saved.observed_ms, saved.observed_ms + 120_000] {
                assert_eq!(
                    saved.reuse(pinned, Some(Duration::from_secs(600)), now),
                    Reuse::Spent,
                    "{vary}"
                );
            }
            assert!(
                !worth_keeping(
                    &saved.answer.headers,
                    pinned,
                    Some(Duration::from_secs(600))
                ),
                "{vary}"
            );
        }
        assert!(!saved.allows_stale(), "{vary}");
    }
}

#[test]
fn vary_star_confirmation_replaces_all_previous_vary_fields() {
    let mut saved = saved_with(&[
        ("vary", "Accept"),
        ("VARY", "X-Variant"),
        ("cache-control", "max-age=60"),
        ("etag", "\"old\""),
    ]);
    saved.confirm(&[("VaRy".into(), "*".into())], saved.observed_ms + 1);
    assert_eq!(
        saved
            .answer
            .headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("vary"))
            .collect::<Vec<_>>(),
        vec![&("VaRy".to_owned(), "*".to_owned())]
    );
    assert_eq!(saved.reuse(true, None, saved.observed_ms), Reuse::Spent);
    saved.confirm(&[("vary".into(), "Accept".into())], saved.observed_ms + 1);
    assert_eq!(saved.reuse(false, None, saved.observed_ms), Reuse::Fresh);
}

#[tokio::test]
async fn vary_star_fresh_responses_always_fetch_the_next_representation() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let server = MockServer::start().await;
    let sequence = AtomicUsize::new(0);
    Mock::given(path("/api/v3/search/repositories"))
        .respond_with(move |_: &Request| {
            ResponseTemplate::new(200)
                .insert_header("cache-control", "private, max-age=60")
                .insert_header("vary", "*")
                .insert_header("etag", "\"old\"")
                .set_body_string(format!(
                    "representation-{}",
                    sequence.fetch_add(1, Ordering::SeqCst)
                ))
        })
        .mount(&server)
        .await;
    let (transport, endpoint, memory) = keeping(&server);
    for index in 0..2 {
        let page = transport
            .execute(
                RequestSpec::get(endpoint.rest(&["search", "repositories"]).unwrap()),
                &request("t"),
            )
            .await
            .unwrap();
        assert_eq!(
            page.body.as_ref(),
            format!("representation-{index}").as_bytes()
        );
    }
    let seen = requests(&server).await;
    assert_eq!(seen.len(), 2);
    assert!(
        seen.iter()
            .all(|request| !request.headers.contains_key("if-none-match"))
    );
    assert_eq!(memory.len(), 0);
}

#[tokio::test]
async fn vary_star_replacements_and_confirmations_revoke_saved_answers() {
    for (status, partial) in [(200, false), (304, false), (200, true)] {
        let server = MockServer::start().await;
        Mock::given(path("/api/v3/repos/a/b"))
            .respond_with(validated("old", "\"v1\""))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        let mut replacement = ResponseTemplate::new(status)
            .insert_header("vary", "*")
            .insert_header("cache-control", "private, max-age=60")
            .set_body_string(if status == 200 { "new" } else { "" });
        if partial {
            replacement =
                replacement.insert_header("x-github-sso", "partial-results; organizations=1");
        }
        Mock::given(path("/api/v3/repos/a/b"))
            .respond_with(replacement)
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(path("/api/v3/repos/a/b"))
            .respond_with(ResponseTemplate::new(200).set_body_string("latest"))
            .mount(&server)
            .await;
        let (transport, endpoint, memory) = keeping(&server);
        let spec = || RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap());
        transport.execute(spec(), &request("t")).await.unwrap();
        assert_eq!(memory.len(), 1);
        let changed = transport.execute(spec(), &request("t")).await.unwrap();
        if status == 304 {
            assert_eq!(changed.body.as_ref(), b"old");
        }
        assert_eq!(memory.len(), 0, "status {status}, partial {partial}");
        let latest = transport.execute(spec(), &request("t")).await.unwrap();
        assert_eq!(latest.body.as_ref(), b"latest");
        assert!(
            !requests(&server).await[2]
                .headers
                .contains_key("if-none-match")
        );
    }
}

#[tokio::test]
async fn vary_star_legacy_saved_answer_cannot_fall_back_after_rate_limit() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v3/repos/a/b"))
        .respond_with(validated("old", "\"v1\""))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "60")
                .set_body_string("limited"),
        )
        .mount(&server)
        .await;
    let (transport, endpoint, memory) = keeping(&server);
    let spec = || RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap());
    transport.execute(spec(), &request("t")).await.unwrap();
    for saved in memory.0.lock().unwrap().values_mut() {
        saved
            .answer
            .headers
            .push(("VaRy".into(), "Accept, *".into()));
    }
    let error = transport.execute(spec(), &request("t")).await.unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert!(
        !requests(&server).await[1]
            .headers
            .contains_key("if-none-match")
    );
}
