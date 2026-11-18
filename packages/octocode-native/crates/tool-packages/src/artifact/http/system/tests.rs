//! The system client's retries, connection reuse and DNS pinning against
//! live local servers (loopback: reached directly, with no system proxy
//! lookup).
use super::{MAX_DRAIN_BYTES, SystemArtifactHttp, retry_delay};
use crate::artifact::http::{ArtifactHttp, ArtifactHttpRequest, DnsPin};
use octocode_io::budget::RequestBudget;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
            validator: None,
        };
        let budget = RequestBudget::with_timeout(Duration::from_secs(60), 1024);
        let response = SystemArtifactHttp::shared()
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

/// A 429 retries after the registry's short `Retry-After`; a wait past the
/// request's deadline (as delta-seconds or an HTTP-date) returns the rate
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
            validator: None,
        };
        let budget = RequestBudget::with_timeout(Duration::from_secs(60), 1024);
        // Build the client first: building is setup, not the retry wait
        // this bound measures.
        let client = SystemArtifactHttp::shared().with_retries(1);
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
        validator: None,
    };
    let budget = RequestBudget::with_timeout(Duration::from_secs(60), 1024);
    let response = SystemArtifactHttp::shared()
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
            validator: None,
        };
        let budget = RequestBudget::with_timeout(Duration::from_secs(60), 1024);
        let response = SystemArtifactHttp::shared()
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
            validator: None,
        };
        let budget = RequestBudget::with_timeout(Duration::from_secs(60), 1024);
        let response = SystemArtifactHttp::shared()
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
            validator: None,
        };
        let response = SystemArtifactHttp::shared()
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
        validator: None,
    };
    let budget = RequestBudget::with_timeout(Duration::from_secs(60), 1024);
    let response = SystemArtifactHttp::shared()
        .get(request, &budget)
        .await
        .expect("pinned request reaches the validated address");
    assert_eq!(response.status, 200);
}

/// A 304 answers the validator a request asked with; asked without one it stays
/// the redirect it is, which the registry client refuses.
#[tokio::test]
async fn a_304_is_an_answer_only_to_a_conditional_request() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(304).insert_header("etag", "\"e\""))
        .mount(&server)
        .await;
    let http = SystemArtifactHttp::shared().with_retries(0);
    let budget = RequestBudget::with_timeout(Duration::from_secs(5), 1024);
    let request = |validator| ArtifactHttpRequest {
        url: Url::parse(&format!("{}/pkg", server.uri())).expect("test URL"),
        accept: "application/json",
        authorization: None,
        dns_pin: None,
        validator,
    };
    let confirmed = http
        .get(
            request(Some((reqwest::header::IF_NONE_MATCH, "\"e\"".to_owned()))),
            &budget,
        )
        .await
        .expect("a confirmation");
    assert_eq!(confirmed.status, 304);
    assert!(
        confirmed
            .headers
            .contains(&("etag".to_owned(), "\"e\"".to_owned()))
    );
    assert!(confirmed.headers.iter().any(|(name, _)| name == "age"));
    let received = server.received_requests().await.expect("recorded");
    assert_eq!(received[0].headers["if-none-match"], "\"e\"");
    assert!(http.get(request(None), &budget).await.is_err());
}

#[tokio::test]
async fn registry_freshness_keeps_dates_repeated_policy_and_response_delay() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("date", "Thu, 01 Jan 1970 00:15:00 GMT")
                .append_header("cache-control", "max-age=60")
                .append_header("cache-control", "no-store")
                .insert_header("age", "45")
                .set_body_string("{}"),
        )
        .mount(&server)
        .await;
    let request = ArtifactHttpRequest {
        url: Url::parse(&format!("{}/pkg", server.uri())).expect("test URL"),
        accept: "application/json",
        authorization: None,
        dns_pin: None,
        validator: None,
    };
    let budget = RequestBudget::with_timeout(Duration::from_secs(30), 1024);
    let response = SystemArtifactHttp::shared()
        .get(request, &budget)
        .await
        .expect("registry answer");
    assert!(
        response
            .headers
            .contains(&("date".into(), "Thu, 01 Jan 1970 00:15:00 GMT".into()))
    );
    assert!(
        response
            .headers
            .contains(&("cache-control".into(), "max-age=60".into()))
    );
    assert!(
        response
            .headers
            .contains(&("cache-control".into(), "no-store".into()))
    );
    let age = response
        .headers
        .iter()
        .find(|(name, _)| name == "age")
        .expect("corrected age")
        .1
        .parse::<u64>()
        .expect("seconds");
    assert!(age > 45, "even a subsecond response contributes to Age");
}

/// Cache policy must cross the actual HTTP adapter without losing repeated
/// Vary fields; the shared saved-answer policy can only enforce what it sees.
#[tokio::test]
async fn response_cache_controls_preserve_registry_origin_headers() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .append_header("vary", "Accept")
                .append_header("vary", "*")
                .insert_header("cache-control", "no-store")
                .set_body_string("{}"),
        )
        .mount(&server)
        .await;
    let request = ArtifactHttpRequest {
        url: Url::parse(&format!("{}/pkg", server.uri())).expect("URL"),
        accept: "application/json",
        authorization: None,
        dns_pin: None,
        validator: None,
    };
    let budget = RequestBudget::with_timeout(Duration::from_secs(30), 1024);
    let response = SystemArtifactHttp::shared()
        .get(request, &budget)
        .await
        .expect("origin answer");
    assert_eq!(response.body, b"{}");
    for (name, value) in [
        ("vary", "Accept"),
        ("vary", "*"),
        ("cache-control", "no-store"),
    ] {
        assert!(
            response
                .headers
                .contains(&(name.to_owned(), value.to_owned())),
            "missing {name}: {value}: {:?}",
            response.headers
        );
    }
}

/// A GET of `url` that gets no answer, with no retry.
async fn unanswered(url: String) -> crate::artifact::ArtifactError {
    let request = ArtifactHttpRequest {
        url: Url::parse(&url).expect("URL"),
        accept: "application/json",
        authorization: None,
        dns_pin: None,
        validator: None,
    };
    let budget = RequestBudget::with_timeout(Duration::from_secs(30), 1024);
    SystemArtifactHttp::shared()
        .with_retries(0)
        .get(request, &budget)
        .await
        .expect_err("no answer")
}

/// A registry request that got no answer says why, from the transport
/// error, and how to fix it, instead of a bare "Retry later".
#[tokio::test]
async fn a_refused_connection_names_its_cause_and_fix() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("address");
    drop(listener);
    let error = unanswered(format!("http://{address}/pkg")).await;
    assert!(
        error.message.contains("refused the connection"),
        "{error:?}"
    );
    assert!(
        error.hints.iter().any(|hint| hint.contains("reachable")),
        "{error:?}"
    );
}

/// A peer that does not speak TLS fails the handshake, and the fix names the
/// trust store settings a TLS-intercepting proxy needs.
#[tokio::test]
async fn a_failed_tls_handshake_names_its_cause_and_fix() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let _ = tokio::io::AsyncWriteExt::write_all(
                &mut socket,
                b"HTTP/1.1 400 Bad Request\r\ncontent-length: 0\r\n\r\n",
            )
            .await;
        }
    });
    let error = unanswered(format!("https://{address}/pkg")).await;
    server.abort();
    assert!(error.message.contains("TLS"), "{error:?}");
    assert!(
        error
            .hints
            .iter()
            .any(|hint| hint.contains("SSL_CERT_FILE")),
        "{error:?}"
    );
}

/// A registry's `Retry-After` is waited out whole whenever the request's
/// deadline leaves time for it, longer than the client's own backoff cap
/// included (a 429 asking 20 s inside a 60 s request is retried, not
/// returned); one the deadline cannot hold is returned at once. The deadline
/// is the injected clock: no test waits in real time.
#[test]
fn a_retry_after_the_deadline_holds_is_waited_out_whole() {
    let headers = |value: &str| {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::RETRY_AFTER,
            reqwest::header::HeaderValue::from_str(value).expect("header"),
        );
        headers
    };
    let within = RequestBudget::with_timeout(Duration::from_secs(60), 1024);
    assert_eq!(
        retry_delay(Some(&headers("20")), 0, &within),
        Some(Duration::from_secs(20))
    );
    let short = RequestBudget::with_timeout(Duration::from_secs(15), 1024);
    assert_eq!(retry_delay(Some(&headers("20")), 0, &short), None);
    // The client's own backoff stays within its cap.
    let backoff = retry_delay(None, 30, &within).expect("a backoff");
    assert!(backoff <= Duration::from_secs(10), "{backoff:?}");
}
