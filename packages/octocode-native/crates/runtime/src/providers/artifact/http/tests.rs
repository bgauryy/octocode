//! Registry status classification and independent fresh reads.
use super::client::response_body;
use super::{ArtifactHttpResponse, JSON, RegistryClient};
use crate::providers::artifact::ArtifactError;
use crate::providers::artifact::test_http::SequenceHttp;
use crate::providers::artifact::test_http::test_budget;
use octocode_contract_types::{ArtifactType, ErrorCode};
use secrecy::SecretString;
use url::Url;

fn classify_with(status: u16, not_found_is_empty: bool) -> Result<Option<Vec<u8>>, ArtifactError> {
    response_body(
        ArtifactType::Npm,
        ArtifactHttpResponse {
            status,
            body: vec![],
            headers: Vec::new(),
        },
        not_found_is_empty,
    )
}

fn classify(status: u16) -> Result<Option<Vec<u8>>, ArtifactError> {
    classify_with(status, true)
}

#[test]
fn malformed_requests_are_invalid_input_with_status() {
    for status in [400u16, 422] {
        let error = classify(status).expect_err("4xx is an error");
        assert_eq!(error.code, ErrorCode::InvalidInput);
        assert_eq!(error.status, Some(status));
        assert!(
            error.message.contains(&format!("HTTP {status}")),
            "{}",
            error.message
        );
    }
}

/// A registry answer reports the errorCode a GitHub answer of the same
/// status does: one status table for every provider.
#[test]
fn registry_statuses_map_to_the_codes_github_reports() {
    for (status, code) in [
        (401u16, ErrorCode::Authentication),
        (403, ErrorCode::PermissionDenied),
        (410, ErrorCode::NotFound),
        (400, ErrorCode::InvalidInput),
        (422, ErrorCode::InvalidInput),
        (429, ErrorCode::RateLimited),
        (451, ErrorCode::Unavailable),
        (500, ErrorCode::Server),
        (502, ErrorCode::Server),
        (503, ErrorCode::Server),
        // npm answers exact lookups for malformed names (e.g. non-ASCII
        // coordinates) with 405: an unmapped status, never retryable.
        (405, ErrorCode::HttpStatus),
        (408, ErrorCode::HttpStatus),
    ] {
        let error = classify(status).expect_err("non-2xx is an error");
        assert_eq!(error.code, code, "HTTP {status}");
        assert_eq!(
            error.code,
            octocode_github::ProviderErrorKind::from_status(status).code(),
            "HTTP {status}"
        );
        assert_eq!(error.status, Some(status));
        assert!(
            error.message.contains(&format!("HTTP {status}")),
            "{}",
            error.message
        );
    }
}

/// A missing resource is an empty answer only where the caller asks for
/// that (`not_found_is_empty`); elsewhere it is `notFound`, as on GitHub.
#[test]
fn not_found_is_empty_only_where_asked() {
    assert_eq!(classify(404).expect("404 maps to empty"), None);
    let missing = classify_with(404, false).expect_err("404 is an error");
    assert_eq!(missing.code, ErrorCode::NotFound);
    assert_eq!(missing.status, Some(404));
}

/// A rejected request states the registry's own reason (a per-call
/// fact), not static advice.
#[test]
fn a_rejected_request_states_the_registrys_reason() {
    let detail = "Cannot page beyond the first 1000 results when sorting by relevance.";
    let error = response_body(
        ArtifactType::Crates,
        ArtifactHttpResponse {
            status: 400,
            body: serde_json::to_vec(&serde_json::json!({"errors": [{"detail": detail}]}))
                .expect("body"),
            headers: Vec::new(),
        },
        true,
    )
    .expect_err("400");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    assert!(error.message.contains(detail), "{}", error.message);
    let bare = classify(400).expect_err("400");
    crate::tools::test_support::checks::assert_absent(
        &serde_json::json!(bare.message),
        "Check the",
    );
}

#[test]
fn server_errors_say_retry_later() {
    for status in [500u16, 502, 503] {
        let error = classify(status).expect_err("5xx is an error");
        assert!(error.message.contains("Retry later"), "{}", error.message);
    }
    let limited = classify(429).expect_err("429");
    assert!(
        limited.message.contains("rate limit"),
        "{}",
        limited.message
    );
}

/// Every independent read reaches the registry.
/// and writes: consecutive anonymous calls for the same URL always reach
/// the HTTP layer.
#[tokio::test]
async fn independent_reads_fetch_current_answers() {
    let http = SequenceHttp::new(vec![b"{\"name\":\"test\"}".to_vec()]);
    let budget = test_budget();
    let client = RegistryClient {
        tags: None,
        ..RegistryClient::new(&http, &budget)
    };
    let url = Url::parse("https://cache-disabled-test.invalid/pkg").expect("test URL");
    // First call — must hit HTTP.
    let _ = client
        .json(ArtifactType::Npm, url.clone(), false, None, None, JSON)
        .await;
    assert_eq!(http.calls(), 1, "first call must hit HTTP");
    // The second call with the same URL also reaches HTTP.
    let _ = client
        .json(ArtifactType::Npm, url, false, None, None, JSON)
        .await;
    assert_eq!(http.calls(), 2, "second call must hit HTTP without a cache");
}

/// Every authorized read reaches the provider with its own credential.
#[tokio::test]
async fn every_authorized_read_uses_its_current_credential() {
    let http = SequenceHttp::new(vec![b"{\"name\":\"private\"}".to_vec()]);
    let budget = test_budget();
    let client = RegistryClient {
        tags: None,
        ..RegistryClient::new(&http, &budget)
    };
    let url = Url::parse("https://authorized-cache-test.invalid/pkg").expect("test URL");
    let token = |value: &str| Some(SecretString::from(format!("Bearer {value}")));
    for authorization in [token("a"), token("a"), token("b"), None, token("b")] {
        client
            .json(
                ArtifactType::Npm,
                url.clone(),
                false,
                authorization,
                None,
                JSON,
            )
            .await
            .expect("read");
    }
    assert_eq!(http.calls(), 5, "each independent call reaches HTTP");
}

/// A malformed body fails, and each retry reads the provider again.
#[tokio::test]
async fn a_malformed_body_is_not_cached_and_the_retry_reads_again() {
    let http = SequenceHttp::new(vec![b"{\"name\":".to_vec(), b"{\"name\":\"ok\"}".to_vec()]);
    let budget = test_budget();
    let client = RegistryClient {
        tags: None,
        ..RegistryClient::new(&http, &budget)
    };
    let url = Url::parse("https://malformed-cache-test.invalid/pkg").expect("test URL");
    let read = || client.json(ArtifactType::Npm, url.clone(), false, None, None, JSON);
    assert_eq!(
        read().await.expect_err("malformed").code,
        ErrorCode::ProviderError
    );
    assert_eq!(read().await.expect("retry").expect("body")["name"], "ok");
    assert_eq!(read().await.expect("fresh").expect("body")["name"], "ok");
    assert_eq!(http.calls(), 3, "malformed, retry, independent fresh read");
}

mod kept_answers {
    use super::super::*;
    use crate::providers::saved_answers::{KeptAnswers, ReusedSince};
    use std::sync::Mutex;

    /// Answers each read with the next scripted answer and records what asked.
    struct Scripted {
        answers: Mutex<std::collections::VecDeque<ArtifactHttpResponse>>,
        validators: Mutex<Vec<Option<String>>>,
    }

    /// One scripted reply: status, body, headers.
    type ScriptedAnswer<'a> = (u16, &'a str, &'a [(&'a str, &'a str)]);

    impl Scripted {
        fn new(answers: Vec<ScriptedAnswer<'_>>) -> Self {
            Self {
                answers: Mutex::new(
                    answers
                        .into_iter()
                        .map(|(status, body, headers)| ArtifactHttpResponse {
                            status,
                            body: body.as_bytes().to_vec(),
                            headers: headers
                                .iter()
                                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                                .collect(),
                        })
                        .collect(),
                ),
                validators: Mutex::default(),
            }
        }

        fn asked(&self) -> Vec<Option<String>> {
            self.validators.lock().unwrap().clone()
        }
    }

    impl ArtifactHttp for Scripted {
        fn get<'a>(
            &'a self,
            request: ArtifactHttpRequest,
            _budget: &'a RequestBudget,
        ) -> ArtifactHttpFuture<'a> {
            self.validators
                .lock()
                .unwrap()
                .push(request.validator.map(|(_, value)| value));
            let answer = self.answers.lock().unwrap().pop_front().expect("scripted");
            Box::pin(async move { Ok(answer) })
        }
    }

    fn budget() -> RequestBudget {
        RequestBudget::with_timeout(std::time::Duration::from_secs(5), 1024 * 1024)
    }

    async fn read(
        http: &Scripted,
        answers: &KeptAnswers,
        reused: &ReusedSince,
        token: Option<&str>,
    ) -> Result<Option<serde_json::Value>, ArtifactError> {
        let budget = budget();
        let client = RegistryClient {
            answers: Some(answers),
            reused,
            ..RegistryClient::new(http, &budget)
        };
        client
            .json(
                octocode_contract_types::ArtifactType::Npm,
                url::Url::parse("https://registry.invalid/zod").unwrap(),
                true,
                token.map(|token| secrecy::SecretString::from(token.to_owned())),
                None,
                JSON,
            )
            .await
    }

    #[tokio::test]
    async fn a_repeat_asks_with_its_validator_and_a_304_answers_the_same_value() {
        let http = Scripted::new(vec![
            (200, "{\"v\":1}", &[("etag", "\"e1\"")]),
            (304, "", &[("etag", "\"e1\"")]),
            (200, "{\"v\":2}", &[("etag", "\"e2\"")]),
            (304, "", &[]),
        ]);
        let answers = KeptAnswers::new(None);
        let reused = ReusedSince::default();
        for version in [1, 1, 2, 2] {
            let value = read(&http, &answers, &reused, None).await.unwrap().unwrap();
            assert_eq!(value["v"], version);
        }
        assert_eq!(
            http.asked(),
            [
                None,
                Some("\"e1\"".to_owned()),
                Some("\"e1\"".to_owned()),
                Some("\"e2\"".to_owned())
            ]
        );
        assert_eq!(
            reused.get(),
            None,
            "confirmed each time: nothing to disclose"
        );
    }

    /// The registry's own `max-age` answers unasked, and says so.
    #[tokio::test]
    async fn a_stated_lifetime_answers_with_nothing_sent_and_is_disclosed() {
        let http = Scripted::new(vec![(
            200,
            "{\"v\":1}",
            &[("cache-control", "max-age=300")],
        )]);
        let answers = KeptAnswers::new(None);
        let reused = ReusedSince::default();
        read(&http, &answers, &reused, None).await.unwrap();
        assert_eq!(reused.get(), None);
        let value = read(&http, &answers, &reused, None).await.unwrap().unwrap();
        assert_eq!(value["v"], 1);
        assert_eq!(http.asked().len(), 1);
        assert!(reused.get().is_some());
    }

    #[tokio::test]
    async fn one_tokens_answer_never_reaches_another() {
        let http = Scripted::new(vec![
            (
                200,
                "{\"v\":\"private\"}",
                &[("cache-control", "max-age=300"), ("etag", "\"p\"")],
            ),
            (200, "{\"v\":\"public\"}", &[("etag", "\"q\"")]),
        ]);
        let answers = KeptAnswers::new(None);
        let reused = ReusedSince::default();
        read(&http, &answers, &reused, Some("Bearer one"))
            .await
            .unwrap();
        let other = read(&http, &answers, &reused, None).await.unwrap().unwrap();
        assert_eq!(other["v"], "public");
        assert_eq!(
            http.asked(),
            [None, None],
            "no validator crossed credentials"
        );
        assert_eq!(reused.get(), None);
    }

    /// An answer that does not decode is not kept, a 404 drops what was kept,
    /// and a rate limit answers with what was kept, disclosed.
    #[tokio::test]
    async fn only_a_decoded_answer_is_kept_and_a_404_drops_it() {
        let http = Scripted::new(vec![
            (200, "not json", &[("etag", "\"bad\"")]),
            (200, "{\"v\":1}", &[("etag", "\"e\"")]),
            (429, "", &[]),
            (404, "{}", &[]),
            (200, "{\"v\":2}", &[]),
        ]);
        let answers = KeptAnswers::new(None);
        let reused = ReusedSince::default();
        assert!(read(&http, &answers, &reused, None).await.is_err());
        read(&http, &answers, &reused, None).await.unwrap();
        let limited = read(&http, &answers, &reused, None).await.unwrap().unwrap();
        assert_eq!(limited["v"], 1);
        assert!(
            reused.get().is_some(),
            "served because of a rate limit: said so"
        );
        assert_eq!(read(&http, &answers, &reused, None).await.unwrap(), None);
        read(&http, &answers, &reused, None).await.unwrap();
        assert_eq!(
            http.asked(),
            [
                None,
                None,
                Some("\"e\"".to_owned()),
                Some("\"e\"".to_owned()),
                None
            ],
            "the undecoded answer gave no validator, and the 404 dropped the kept one"
        );
    }
}
