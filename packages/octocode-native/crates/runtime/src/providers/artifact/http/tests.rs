//! Registry status classification and the answer cache.
use super::client::response_body;
use super::{
    ArtifactCache, ArtifactHttpResponse, JSON, REGISTRY_ANSWER, RegistryClient, cache_key,
};
use crate::providers::artifact::ArtifactError;
use crate::providers::artifact::test_http::test_budget;
use crate::providers::artifact::test_http::{NoHttp, SequenceHttp};
use crate::tools::test_support::Fixture;
use octocode_contract_types::{ArtifactType, ErrorCode};
use octocode_io::budget::RequestBudget;
use secrecy::SecretString;
use url::Url;

fn classify_with(status: u16, not_found_is_empty: bool) -> Result<Option<Vec<u8>>, ArtifactError> {
    response_body(
        ArtifactType::Npm,
        ArtifactHttpResponse {
            status,
            body: vec![],
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
        (403, ErrorCode::Permission),
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

/// No cache must bypass the in-process cache on both reads
/// and writes: consecutive anonymous calls for the same URL always reach
/// the HTTP layer.
#[tokio::test]
async fn cache_disabled_skips_cache_for_reads_and_writes() {
    let http = SequenceHttp::new(vec![b"{\"name\":\"test\"}".to_vec()]);
    let budget = test_budget();
    let client = RegistryClient {
        http: &http,
        budget: &budget,
        cache: None,
        tags: None,
    };
    let url = Url::parse("https://cache-disabled-test.invalid/pkg").expect("test URL");
    // First call — must hit HTTP.
    let _ = client
        .json(ArtifactType::Npm, url.clone(), false, None, None, JSON)
        .await;
    assert_eq!(http.calls(), 1, "first call must hit HTTP");
    // Second call with the same URL — must hit HTTP again (nothing written to cache).
    let _ = client
        .json(ArtifactType::Npm, url, false, None, None, JSON)
        .await;
    assert_eq!(http.calls(), 2, "second call must hit HTTP without a cache");
}

/// A token-authorized read is cached under that credential only: the
/// same token reads it once, another token or no token reads again.
#[tokio::test]
async fn authorized_reads_are_cached_per_credential() {
    let http = SequenceHttp::new(vec![b"{\"name\":\"private\"}".to_vec()]);
    let cache = ArtifactCache::new(None);
    let budget = test_budget();
    let client = RegistryClient {
        http: &http,
        budget: &budget,
        cache: Some(&cache),
        tags: None,
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
    assert_eq!(http.calls(), 3, "a, b and anonymous read once each");
}

/// A malformed registry body is never cached: the retry asks the
/// registry again, succeeds, and the valid answer is what is cached.
#[tokio::test]
async fn a_malformed_body_is_not_cached_and_the_retry_reads_again() {
    let http = SequenceHttp::new(vec![b"{\"name\":".to_vec(), b"{\"name\":\"ok\"}".to_vec()]);
    let cache = ArtifactCache::new(None);
    let budget = test_budget();
    let client = RegistryClient {
        http: &http,
        budget: &budget,
        cache: Some(&cache),
        tags: None,
    };
    let url = Url::parse("https://malformed-cache-test.invalid/pkg").expect("test URL");
    let read = || client.json(ArtifactType::Npm, url.clone(), false, None, None, JSON);
    assert_eq!(
        read().await.expect_err("malformed").code,
        ErrorCode::ProviderError
    );
    assert_eq!(read().await.expect("retry").expect("body")["name"], "ok");
    assert_eq!(read().await.expect("cached").expect("body")["name"], "ok");
    assert_eq!(http.calls(), 2, "malformed, retry, then cache");
}

/// An invalid entry already in the cache reads as a miss and is replaced.
#[tokio::test]
async fn an_invalid_cached_entry_is_a_miss_and_is_replaced() {
    let http = SequenceHttp::new(vec![b"{\"name\":\"fresh\"}".to_vec()]);
    let dir = Fixture::new();
    ArtifactCache::new(Some(dir.0.clone())).0.put(
        cache_key(
            &Url::parse("https://poisoned-cache-test.invalid/pkg").expect("URL"),
            JSON,
            None,
        ),
        serde_json::json!({"name": "stale"}),
        16,
        REGISTRY_ANSWER,
    );
    for file in crate::cache::tests::entry_files(&dir.0) {
        std::fs::write(file, b"not json").expect("corrupt the entry");
    }
    let cache = ArtifactCache::new(Some(dir.0.clone()));
    let budget = test_budget();
    let client = RegistryClient {
        http: &http,
        budget: &budget,
        cache: Some(&cache),
        tags: None,
    };
    let url = Url::parse("https://poisoned-cache-test.invalid/pkg").expect("test URL");
    for _ in 0..2 {
        let value = client
            .json(ArtifactType::Npm, url.clone(), false, None, None, JSON)
            .await
            .expect("read")
            .expect("body");
        assert_eq!(value["name"], "fresh");
    }
    assert_eq!(http.calls(), 1);
}

/// Immutable release facts are remembered by key while the cache is on.
#[test]
fn release_facts_are_remembered_only_with_the_cache_on() {
    let budget = test_budget();
    let key = "crates:fact-test@1.0.0";
    let fact = serde_json::json!({"sha": "abc"});
    let off = RegistryClient::uncached(&NoHttp, &budget);
    off.remember_fact(key, &fact);
    assert_eq!(off.fact(key), None);
    let cache = ArtifactCache::new(None);
    let on = RegistryClient {
        cache: Some(&cache),
        tags: None,
        ..off
    };
    assert_eq!(on.fact(key), None);
    on.remember_fact(key, &fact);
    assert_eq!(on.fact(key), Some(fact));
}

/// Each runtime owns its cache: a fact remembered under one home is not
/// seen under another, while a later runtime on the same home reads it
/// from disk.
#[test]
fn runtimes_with_different_homes_do_not_share_registry_answers() {
    let budget = test_budget();
    let (first, second) = (Fixture::new(), Fixture::new());
    let fact = serde_json::json!({"sha": "abc"});
    fn client<'a>(cache: &'a ArtifactCache, budget: &'a RequestBudget) -> RegistryClient<'a> {
        RegistryClient {
            cache: Some(cache),
            ..RegistryClient::uncached(&NoHttp, budget)
        }
    }
    let one = ArtifactCache::new(Some(first.0.clone()));
    client(&one, &budget).remember_fact("crates:isolated@1.0.0", &fact);
    let other = ArtifactCache::new(Some(second.0.clone()));
    assert_eq!(client(&other, &budget).fact("crates:isolated@1.0.0"), None);
    let reopened = ArtifactCache::new(Some(first.0.clone()));
    assert_eq!(
        client(&reopened, &budget).fact("crates:isolated@1.0.0"),
        Some(fact)
    );
}
