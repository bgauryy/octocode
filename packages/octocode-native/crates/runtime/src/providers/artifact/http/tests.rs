use super::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::super::test_http::{NoHttp, SequenceHttp};
use crate::tools::test_support::Fixture;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn classify(status: u16) -> Result<Option<Vec<u8>>, ArtifactError> {
    let budget = super::super::types::test_budget();
    let client = RegistryClient::uncached(&NoHttp, &budget);
    client.status(
        ArtifactType::Npm,
        ArtifactHttpResponse {
            status,
            body: vec![],
        },
        true,
    )
}

#[test]
fn deterministic_4xx_is_invalid_query_with_status() {
    // npm answers exact lookups for malformed names (e.g. non-ASCII
    // coordinates) with 405, not 404; it must not read as retryable.
    for status in [400u16, 405, 422] {
        let error = classify(status).expect_err("4xx is an error");
        assert_eq!(error.code, "invalidInput");
        assert_eq!(error.status, Some(status));
        assert!(
            error.message.contains(&format!("HTTP {status}")),
            "{}",
            error.message
        );
    }
}

/// A rejected request states the registry's own reason (a per-call
/// fact), not static advice.
#[test]
fn a_rejected_request_states_the_registrys_reason() {
    let budget = super::super::types::test_budget();
    let client = RegistryClient::uncached(&NoHttp, &budget);
    let detail = "Cannot page beyond the first 1000 results when sorting by relevance.";
    let error = client
        .status(
            ArtifactType::Crates,
            ArtifactHttpResponse {
                status: 400,
                body: serde_json::to_vec(&serde_json::json!({"errors": [{"detail": detail}]}))
                    .expect("body"),
            },
            true,
        )
        .expect_err("400");
    assert_eq!(error.code, "invalidInput");
    assert!(error.message.contains(detail), "{}", error.message);
    let bare = classify(400).expect_err("400");
    crate::tools::test_support::checks::assert_absent(
        &serde_json::json!(bare.message),
        "Check the",
    );
}

#[test]
fn server_errors_and_408_stay_retryable_provider_error() {
    for status in [408u16, 500, 502, 503] {
        let error = classify(status).expect_err("5xx is an error");
        assert_eq!(error.code, "providerError");
        assert_eq!(error.status, Some(status));
        assert!(
            error.message.contains(&format!("HTTP {status}"))
                && error.message.contains("Retry later"),
            "{}",
            error.message
        );
    }
}

#[test]
fn auth_rate_limit_and_not_found_keep_dedicated_mappings() {
    assert_eq!(classify(404).expect("404 maps to empty"), None);
    assert_eq!(classify(401).expect_err("401").code, "authentication");
    assert_eq!(classify(403).expect_err("403").code, "authentication");
    let limited = classify(429).expect_err("429");
    assert_eq!(limited.code, "rateLimited");
    assert_eq!(limited.status, Some(429));
}

/// No cache must bypass the in-process cache on both reads
/// and writes: consecutive anonymous calls for the same URL always reach
/// the HTTP layer.
#[tokio::test]
async fn cache_disabled_skips_cache_for_reads_and_writes() {
    let http = SequenceHttp::new(vec![b"{\"name\":\"test\"}".to_vec()]);
    let budget = super::super::types::test_budget();
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
    let budget = super::super::types::test_budget();
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
    let budget = super::super::types::test_budget();
    let client = RegistryClient {
        http: &http,
        budget: &budget,
        cache: Some(&cache),
        tags: None,
    };
    let url = Url::parse("https://malformed-cache-test.invalid/pkg").expect("test URL");
    let read = || client.json(ArtifactType::Npm, url.clone(), false, None, None, JSON);
    assert_eq!(read().await.expect_err("malformed").code, "providerError");
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
    let budget = super::super::types::test_budget();
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
    let budget = super::super::types::test_budget();
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
    let budget = super::super::types::test_budget();
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

/// A 5xx answer is retried `max_retries` times, then returned.
#[tokio::test]
async fn server_errors_retry_the_configured_number_of_times() {
    for (retries, requests) in [(0u8, 1usize), (2, 3)] {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let request = ArtifactHttpRequest {
            url: Url::parse(&format!("{}/pkg", server.uri())).expect("test URL"),
            accept: "application/json",
            authorization: None,
            dns_pin: None,
        };
        let budget = RequestBudget::with_timeout(Duration::from_secs(60), 1024);
        let response = SystemArtifactHttp::shared()
            .expect("HTTP client")
            .with_retries(retries)
            .get(request, &budget)
            .await
            .expect("the last answer is returned");
        assert_eq!(response.status, 503);
        assert_eq!(
            server.received_requests().await.unwrap_or_default().len(),
            requests,
            "retries {retries}"
        );
    }
}

/// A 429 retries after the registry's short `Retry-After`; a wait longer
/// than the retry cap (as delta-seconds or an HTTP-date) returns the rate
/// limit at once.
#[tokio::test]
async fn rate_limits_follow_retry_after() {
    for (retry_after, requests) in [
        ("0", 2usize),
        ("120", 1),
        ("Fri, 31 Dec 9999 23:59:59 GMT", 1),
    ] {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", retry_after))
            .mount(&server)
            .await;
        let request = ArtifactHttpRequest {
            url: Url::parse(&format!("{}/pkg", server.uri())).expect("test URL"),
            accept: "application/json",
            authorization: None,
            dns_pin: None,
        };
        let budget = RequestBudget::with_timeout(Duration::from_secs(60), 1024);
        // Build the client first: its one-time TLS root load is setup,
        // not the retry wait this bound measures.
        let client = SystemArtifactHttp::shared()
            .expect("HTTP client")
            .with_retries(1);
        let started = std::time::Instant::now();
        let response = client
            .get(request, &budget)
            .await
            .expect("the last answer is returned");
        assert_eq!(response.status, 429);
        assert!(started.elapsed() < Duration::from_secs(10), "{retry_after}");
        assert_eq!(
            server.received_requests().await.unwrap_or_default().len(),
            requests,
            "retry-after {retry_after}"
        );
    }
}

/// A keep-alive HTTP/1.1 server that answers the n-th request with
/// `responses[n]` (the last one repeats) and counts accepted connections.
fn counting_server(responses: Vec<&'static str>) -> (SocketAddr, Arc<AtomicUsize>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("address");
    let connections = Arc::new(AtomicUsize::new(0));
    let accepted = Arc::clone(&connections);
    let served = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                return;
            };
            accepted.fetch_add(1, Ordering::SeqCst);
            let responses = responses.clone();
            let served = Arc::clone(&served);
            std::thread::spawn(move || {
                let mut buffer = Vec::new();
                let mut chunk = [0_u8; 4096];
                loop {
                    while !buffer.windows(4).any(|window| window == b"\r\n\r\n") {
                        match stream.read(&mut chunk) {
                            Ok(0) | Err(_) => return,
                            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
                        }
                    }
                    let end = buffer
                        .windows(4)
                        .position(|window| window == b"\r\n\r\n")
                        .expect("request head")
                        + 4;
                    buffer.drain(..end);
                    let index = served.fetch_add(1, Ordering::SeqCst);
                    let response = responses[index.min(responses.len() - 1)];
                    if stream.write_all(response.as_bytes()).is_err() {
                        return;
                    }
                }
            });
        }
    });
    (address, connections)
}

const OK_JSON: &str =
    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\n\r\n{}";

/// A retried 5xx body is read, so the retry reuses the connection.
#[tokio::test]
async fn a_retry_reuses_the_connection_after_reading_the_error_body() {
    let body = "x".repeat(MAX_DRAIN_BYTES / 2);
    let failed: &'static str = Box::leak(
        format!(
            "HTTP/1.1 503 Service Unavailable\r\nretry-after: 0\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        )
        .into_boxed_str(),
    );
    let (address, connections) = counting_server(vec![failed, OK_JSON]);
    let request = ArtifactHttpRequest {
        url: Url::parse(&format!("http://{address}/pkg")).expect("test URL"),
        accept: "application/json",
        authorization: None,
        dns_pin: None,
    };
    let budget = RequestBudget::with_timeout(Duration::from_secs(60), 1024);
    let response = SystemArtifactHttp::shared()
        .expect("HTTP client")
        .with_retries(1)
        .get(request, &budget)
        .await
        .expect("retried answer");
    assert_eq!(response.status, 200);
    assert_eq!(connections.load(Ordering::SeqCst), 1);
}

/// A connection that fails before any answer is retried like a 5xx
/// (the GitHub and classification transports already did); without a
/// retry left the transport failure is returned.
#[tokio::test]
async fn a_transport_failure_is_retried() {
    for (retries, expected) in [(1_u8, Some(200_u16)), (0, None)] {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        std::thread::spawn(move || {
            for (index, stream) in listener.incoming().enumerate() {
                let Ok(mut stream) = stream else {
                    return;
                };
                let mut head = [0_u8; 4096];
                let _ = stream.read(&mut head);
                if index > 0 {
                    let _ = stream.write_all(OK_JSON.as_bytes());
                }
            }
        });
        let request = ArtifactHttpRequest {
            url: Url::parse(&format!("http://{address}/pkg")).expect("test URL"),
            accept: "application/json",
            authorization: None,
            dns_pin: None,
        };
        let budget = RequestBudget::with_timeout(Duration::from_secs(60), 1024);
        let response = SystemArtifactHttp::shared()
            .expect("HTTP client")
            .with_retries(retries)
            .get(request, &budget)
            .await;
        assert_eq!(
            response.ok().map(|response| response.status),
            expected,
            "retries {retries}"
        );
    }
}

/// Registry reads are idempotent GETs: a 2xx whose body breaks off is
/// retried like a failed connection; without a retry left it fails.
#[tokio::test]
async fn a_body_cut_short_is_retried() {
    for (retries, expected) in [(1_u8, Some(200_u16)), (0, None)] {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        std::thread::spawn(move || {
            for (index, stream) in listener.incoming().enumerate() {
                let Ok(mut stream) = stream else {
                    return;
                };
                let mut head = [0_u8; 4096];
                let _ = stream.read(&mut head);
                let answer: &[u8] = if index == 0 {
                    b"HTTP/1.1 200 OK\r\ncontent-length: 100\r\n\r\n{\"na"
                } else {
                    OK_JSON.as_bytes()
                };
                let _ = stream.write_all(answer);
            }
        });
        let request = ArtifactHttpRequest {
            url: Url::parse(&format!("http://{address}/pkg")).expect("test URL"),
            accept: "application/json",
            authorization: None,
            dns_pin: None,
        };
        let budget = RequestBudget::with_timeout(Duration::from_secs(60), 1024);
        let response = SystemArtifactHttp::shared()
            .expect("HTTP client")
            .with_retries(retries)
            .get(request, &budget)
            .await;
        assert_eq!(
            response.ok().map(|response| response.status),
            expected,
            "retries {retries}"
        );
    }
}

/// Two reads pinned to the same validated address share one client and
/// its pooled connection.
#[tokio::test]
async fn pinned_reads_reuse_one_client_and_connection() {
    let (address, connections) = counting_server(vec![OK_JSON]);
    let pin = DnsPin {
        host: "pinned-reuse.invalid".to_owned(),
        addresses: vec![address],
    };
    let budget = RequestBudget::with_timeout(Duration::from_secs(60), 1024);
    for _ in 0..3 {
        let request = ArtifactHttpRequest {
            url: Url::parse(&format!(
                "http://pinned-reuse.invalid:{}/pkg",
                address.port()
            ))
            .expect("test URL"),
            accept: "application/json",
            authorization: None,
            dns_pin: Some(pin.clone()),
        };
        let response = SystemArtifactHttp::shared()
            .expect("HTTP client")
            .get(request, &budget)
            .await
            .expect("pinned read");
        assert_eq!(response.status, 200);
    }
    assert_eq!(connections.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn dns_pin_connects_to_the_validated_address() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/package"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .mount(&server)
        .await;
    let port = server.address().port();
    let request = ArtifactHttpRequest {
        url: Url::parse(&format!("http://registry.invalid:{port}/package")).expect("test URL"),
        accept: "application/json",
        authorization: None,
        dns_pin: Some(DnsPin {
            host: "registry.invalid".to_owned(),
            addresses: vec![*server.address()],
        }),
    };
    // The full native suite runs several network/process fixtures in
    // parallel. This test proves address pinning, not latency, so keep its
    // fixture deadline wide enough to avoid scheduler-starvation flakes.
    let budget = RequestBudget::with_timeout(Duration::from_secs(60), 1024);
    let response = SystemArtifactHttp::shared()
        .expect("HTTP client")
        .get(request, &budget)
        .await
        .expect("pinned request reaches the validated address");
    assert_eq!(response.status, 200);
}
