use super::*;
use crate::providers::classification::CIRCUIT_OPEN;
use std::time::SystemTime;

#[tokio::test]
async fn transient_statuses_are_retried_then_succeed() {
    for status in [408, 500, 502, 504, 503, 529, 429] {
        let server = MockServer::builder().start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status).insert_header("retry-after-ms", "1"))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        mount_ok_once(&server).await;
        let started = Instant::now();
        let value = send(&server.uri(), Duration::from_secs(10), 1, &fresh_gate(4))
            .await
            .unwrap_or_else(|error| panic!("HTTP {status} was not retried: {error:?}"));
        assert_eq!(value["model"], "m");
        assert!(
            started.elapsed() >= MIN_RETRY_DELAY,
            "retry-after-ms: 1 must be floored"
        );
    }
}

#[tokio::test]
async fn payment_required_is_remembered_for_every_call_until_the_memo_expires() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(402).set_body_json(json!({"detail":"no credit"})))
        .expect(2)
        .mount(&server)
        .await;
    let call = fresh_gate(4);
    let error = send(&server.uri(), Duration::from_secs(10), 3, &call)
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::ClassificationQuotaExhausted);
    assert!(error.message.contains("quota"), "{}", error.message);
    assert_eq!(error.provider_calls, 1);
    // Later requests of the same call fail fast with the same error.
    let again = send(&server.uri(), Duration::from_secs(10), 3, &call)
        .await
        .unwrap_err();
    assert_eq!(
        (again.code, again.message.as_str(), again.provider_calls),
        (error.code, error.message.as_str(), 0)
    );
    // Another call within the memo fails fast too: no request is sent.
    let other = send(&server.uri(), Duration::from_secs(10), 3, &call.fork())
        .await
        .unwrap_err();
    assert_eq!((other.code, other.provider_calls), (error.code, 0));
    assert!(call.quota_exhausted());
    // After the memo, one call probes again: credit may have been added.
    call.gate().expire_quota_memo();
    assert!(!call.quota_exhausted());
    let probe = send(&server.uri(), Duration::from_secs(10), 3, &call.fork())
        .await
        .unwrap_err();
    assert_eq!((probe.code, probe.provider_calls), (error.code, 1));
}

#[tokio::test]
async fn system_overloaded_body_is_retried_and_shrinks_the_gate() {
    for status in [500, 400] {
        let server = MockServer::builder().start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(status)
                    .insert_header("retry-after-ms", "1")
                    .set_body_json(json!({"detail":{"error_type":"system_overloaded"}})),
            )
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        mount_ok_once(&server).await;
        let lease = fresh_gate(8);
        let value = send(&server.uri(), Duration::from_secs(10), 1, &lease)
            .await
            .unwrap_or_else(|error| panic!("HTTP {status} overload not retried: {error:?}"));
        assert_eq!(value["model"], "m");
        assert_eq!(
            lease.gate().effective_limit(),
            4,
            "overload is an AIMD signal"
        );
    }
    // Retries exhausted: an overload that names its delay is a throttle.
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(503)
                .insert_header("retry-after-ms", "1")
                .set_body_json(json!({"detail":{"error_type":"system_overloaded"}})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let error = send(&server.uri(), Duration::from_secs(10), 0, &fresh_gate(8))
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::ClassificationRateLimited);
    assert!(
        error
            .message
            .contains("is overloaded (the provider returned HTTP 503)"),
        "{}",
        error.message
    );
}

/// A 503/529 without a retry delay is the provider being down, not a
/// throttle: it is never called rate limiting, and it counts toward the
/// circuit, so after the threshold every further chunk fails fast
/// without reaching the provider (11 chunks sent 44 requests before).
#[tokio::test]
async fn an_overload_without_a_delay_opens_the_circuit_instead_of_throttling() {
    for status in [503, 529] {
        let server = MockServer::builder().start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        let lease = fresh_gate(8);
        let mut codes = Vec::new();
        for _ in 0..5 {
            let error = send(&server.uri(), Duration::from_secs(30), 1, &lease.fork())
                .await
                .unwrap_err();
            assert!(
                !error.message.contains("rate limiting"),
                "{status}: {}",
                error.message
            );
            codes.push((error.code, error.message));
        }
        // Every call reports the provider unavailable: the ones that
        // reached it with its failure, the refused ones as paused.
        assert!(
            codes.iter().all(|(code, _)| *code == CIRCUIT_OPEN),
            "{codes:?}"
        );
        assert!(
            codes[..3]
                .iter()
                .all(|(_, message)| message.contains(&format!("HTTP {status}"))),
            "{codes:?}"
        );
        assert!(
            codes[3..]
                .iter()
                .all(|(_, message)| message.contains("paused")),
            "{codes:?}"
        );
        let received = server.received_requests().await.unwrap_or_default().len();
        assert_eq!(
            received,
            gate::CIRCUIT_THRESHOLD as usize,
            "{status}: the open circuit stops the requests"
        );
        assert_eq!(lease.gate().effective_limit(), 8, "no throttle signal");
    }
}

#[tokio::test]
async fn retry_after_beyond_the_deadline_is_rate_limited_not_timeout() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "120"))
        .expect(1)
        .mount(&server)
        .await;
    let lease = fresh_gate(4);
    let error = send(&server.uri(), Duration::from_secs(5), 3, &lease)
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::ClassificationRateLimited);
    assert!(
        error
            .message
            .contains("retry delay of about 120s exceeds the remaining deadline"),
        "{}",
        error.message
    );
    // The cooldown is shared: another call on the same endpoint fails fast
    // without reaching the provider (the mock expects exactly one request).
    let other = lease.fork();
    let error = send(&server.uri(), Duration::from_secs(5), 3, &other)
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::ClassificationRateLimited);
    let seconds: u64 = error
        .message
        .split("about ")
        .nth(1)
        .and_then(|rest| rest.split('s').next())
        .and_then(|n| n.parse().ok())
        .expect("the shared cooldown names its delay");
    assert!(seconds > 100, "{}", error.message);
}

#[test]
fn huge_retry_after_headers_are_bounded_before_duration_conversion() {
    for name in ["retry-after-ms", "retry-after"] {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            reqwest::header::HeaderValue::from_static("1e20"),
        );
        assert_eq!(
            octocode_io::retry::retry_after_delay(&headers, MAX_RETRY_AFTER, SystemTime::now()),
            Some(MAX_RETRY_AFTER)
        );
    }
    // A far-future HTTP-date is bounded by the same cap.
    let mut date = reqwest::header::HeaderMap::new();
    date.insert(
        reqwest::header::RETRY_AFTER,
        reqwest::header::HeaderValue::from_static("Fri, 31 Dec 9999 23:59:59 GMT"),
    );
    assert_eq!(
        octocode_io::retry::retry_after_delay(&date, MAX_RETRY_AFTER, SystemTime::UNIX_EPOCH),
        Some(MAX_RETRY_AFTER)
    );
}

/// Minimal HTTP/1.1 server: one request per connection. `script` decides,
/// per accepted connection index, to reset (`None`) or answer after a delay.
async fn raw_server(
    script: impl Fn(usize) -> Option<Duration> + Send + Sync + 'static,
    in_flight: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let script = Arc::new(script);
    tokio::spawn(async move {
        let mut index = 0;
        while let Ok((mut socket, _)) = listener.accept().await {
            let Some(delay) = script(index) else {
                index += 1;
                drop(socket);
                continue;
            };
            index += 1;
            let (in_flight, peak) = (in_flight.clone(), peak.clone());
            tokio::spawn(async move {
                let mut request = Vec::new();
                let mut buffer = [0_u8; 4096];
                loop {
                    let Ok(read) = socket.read(&mut buffer).await else {
                        return;
                    };
                    if read == 0 {
                        return;
                    }
                    request.extend_from_slice(&buffer[..read]);
                    let text = String::from_utf8_lossy(&request);
                    if let Some(head_end) = text.find("\r\n\r\n") {
                        let length = text[..head_end]
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .and_then(|value| value.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if request.len() >= head_end + 4 + length {
                            break;
                        }
                    }
                }
                let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(delay).await;
                in_flight.fetch_sub(1, Ordering::SeqCst);
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{OK_BODY}",
                    OK_BODY.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    format!("http://{address}")
}

#[tokio::test]
async fn connection_reset_is_retried() {
    let counters = (Arc::default(), Arc::default());
    let server = raw_server(
        |index| (index > 0).then_some(Duration::ZERO),
        counters.0,
        counters.1,
    )
    .await;
    let value = send(&server, Duration::from_secs(10), 2, &fresh_gate(4))
        .await
        .unwrap();
    assert_eq!(value["model"], "m");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gate_bounds_in_flight_requests_across_concurrent_calls() {
    let in_flight = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let server = raw_server(
        |_| Some(Duration::from_millis(40)),
        in_flight.clone(),
        peak.clone(),
    )
    .await;
    let first = fresh_gate(3);
    let calls = [first.clone(), first.fork(), first.fork()];
    let mut tasks = Vec::new();
    for call in calls {
        for _ in 0..6 {
            let (server, call) = (server.clone(), call.clone());
            tasks.push(tokio::spawn(async move {
                send(&server, Duration::from_secs(20), 0, &call).await
            }));
        }
    }
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    assert_eq!(peak.load(Ordering::SeqCst), 3);
    assert_eq!(in_flight.load(Ordering::SeqCst), 0);
}
