use super::*;
use reqwest::header::HeaderValue;

fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in pairs {
        headers.insert(*name, HeaderValue::from_str(value).expect("header"));
    }
    headers
}

#[test]
fn integer_headers_ignore_surrounding_whitespace() {
    let map = headers(&[("x-ratelimit-reset", " 42 "), ("x-bad", "4.2")]);
    assert_eq!(header_u64(&map, "x-ratelimit-reset"), Some(42));
    assert_eq!(header_u64(&map, "x-bad"), None);
    assert_eq!(header_u64(&map, "x-missing"), None);
}

#[test]
fn retry_delay_prefers_milliseconds_then_seconds_and_is_bounded() {
    let max = Duration::from_secs(60);
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(784_111_777); // Sun, 06 Nov 1994 08:49:37 GMT
    let both = headers(&[("retry-after-ms", "1500"), ("retry-after", "9")]);
    assert_eq!(
        retry_after_delay(&both, max, now),
        Some(Duration::from_millis(1500))
    );
    let seconds = headers(&[("retry-after", " 2 ")]);
    assert_eq!(
        retry_after_delay(&seconds, max, now),
        Some(Duration::from_secs(2))
    );
    let huge = headers(&[("retry-after", "1e300")]);
    assert_eq!(retry_after_delay(&huge, max, now), Some(max));
    for invalid in ["-1", "NaN", "soon", "Sun, 06 Nov 1994 25:00:00 GMT", ""] {
        let map = headers(&[("retry-after", invalid)]);
        assert_eq!(retry_after_delay(&map, max, now), None, "{invalid}");
    }
    assert_eq!(retry_after_delay(&HeaderMap::new(), max, now), None);
}

#[test]
fn retry_after_large_durations_saturate_without_panicking() {
    for name in ["retry-after", "retry-after-ms"] {
        for value in [u64::MAX.to_string(), "1e300".to_owned()] {
            let map = headers(&[(name, &value)]);
            assert!(retry_after_delay(&map, Duration::MAX, SystemTime::UNIX_EPOCH).is_some());
        }
    }
    let map = headers(&[("retry-after", &u64::MAX.to_string())]);
    assert_eq!(
        retry_after_delay(&map, Duration::MAX, SystemTime::UNIX_EPOCH),
        Some(Duration::from_secs(u64::MAX))
    );
}

#[test]
fn retry_after_accepts_an_http_date_relative_to_now() {
    let max = Duration::from_secs(24 * 60 * 60);
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(784_111_777);
    let date = headers(&[("retry-after", "Sun, 06 Nov 1994 08:50:07 GMT")]);
    assert_eq!(
        retry_after_delay(&date, max, now),
        Some(Duration::from_secs(30))
    );
    // A date in the past means "now"; a far-future date is bounded.
    let past = headers(&[("retry-after", "Sat, 05 Nov 1994 08:49:37 GMT")]);
    assert_eq!(retry_after_delay(&past, max, now), Some(Duration::ZERO));
    let far = headers(&[("retry-after", "Fri, 31 Dec 9999 23:59:59 GMT")]);
    assert_eq!(
        retry_after_delay(&far, max, SystemTime::UNIX_EPOCH),
        Some(max)
    );
    assert_eq!(
        parse_http_date("Thu, 01 Jan 1970 00:00:00 GMT"),
        Some(SystemTime::UNIX_EPOCH)
    );
    assert_eq!(
        parse_http_date("Tue, 29 Feb 2028 12:00:00 GMT"),
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_835_438_400))
    );
}

#[test]
fn an_http_date_year_cannot_overflow_calendar_arithmetic() {
    for year in [i64::MIN.to_string(), i64::MAX.to_string(), "10000".into()] {
        let value = format!("Sun, 06 Nov {year} 08:49:37 GMT");
        let map = headers(&[("retry-after", &value)]);
        assert_eq!(
            retry_after_delay(&map, Duration::from_secs(60), UNIX_EPOCH),
            None,
            "an HTTP date has a four-digit year: {value}"
        );
    }
}

#[test]
fn jitter_accepts_the_full_duration_range_without_overflow() {
    let _ = full_jitter(Duration::MAX, 0, Duration::MAX);
}

#[test]
fn full_jitter_stays_within_bounds() {
    let base = Duration::from_millis(100);
    let cap = Duration::from_millis(1000);
    for attempt in 0..8 {
        let ceiling = (base * (1 << attempt)).min(cap);
        for _ in 0..50 {
            assert!(full_jitter(base, attempt, cap) <= ceiling);
        }
    }
    assert_eq!(
        full_jitter(Duration::ZERO, 3, Duration::from_secs(1)),
        Duration::ZERO
    );
}

#[test]
fn backoff_is_jittered_capped_and_floored() {
    let backoff = Backoff {
        base: Duration::from_millis(500),
        cap: Duration::from_secs(8),
        floor: Duration::from_millis(100),
    };
    let mut distinct = std::collections::HashSet::new();
    for attempt in 0..40 {
        let delay = backoff.next_delay(attempt);
        assert!(delay >= backoff.floor && delay <= backoff.cap);
        distinct.insert(delay);
    }
    assert!(distinct.len() > 10, "backoff is not jittered");
    assert_eq!(backoff.after(Some(Duration::ZERO), 0), backoff.floor);
    assert_eq!(
        backoff.after(Some(Duration::from_secs(2)), 5),
        Duration::from_secs(2)
    );
    let unfloored = Backoff {
        floor: Duration::ZERO,
        ..backoff
    };
    assert_eq!(unfloored.after(Some(Duration::ZERO), 0), Duration::ZERO);
}

#[test]
fn statuses_classify_once_for_every_client() {
    use StatusClass::*;
    for (status, class) in [
        (200, Success),
        (304, Redirect),
        (301, Redirect),
        (429, RateLimited),
        (503, Overloaded),
        (529, Overloaded),
        (408, Timeout),
        (500, Transient),
        (502, Transient),
        (504, Transient),
        (501, ServerFault),
        (505, ServerFault),
        (400, Final),
        (403, Final),
        (404, Final),
    ] {
        assert_eq!(classify_status(status), class, "{status}");
    }
    assert!(Overloaded.is_server() && Transient.is_server() && ServerFault.is_server());
    assert!(!RateLimited.is_server() && !Timeout.is_server());
}

#[test]
fn retryable_statuses_are_decided_once_with_408_retried() {
    for status in [408, 429, 500, 501, 502, 503, 504, 505, 529, 599] {
        assert!(classify_status(status).is_retryable(), "{status}");
    }
    for status in [200, 204, 301, 304, 400, 401, 403, 404, 410, 422, 451] {
        assert!(!classify_status(status).is_retryable(), "{status}");
    }
}

#[tokio::test]
async fn bodies_are_read_within_the_limit_and_the_budget() {
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500).set_body_bytes(vec![b'x'; 64]))
        .mount(&server)
        .await;
    // One client, and the responses in hand before the budget starts:
    // client setup and connects under load must not spend the deadline
    // the body reads are checked against.
    let client = crate::http::loopback_client(crate::http::ClientPolicy::default());
    let mut responses = Vec::new();
    for _ in 0..3 {
        responses.push(client.get(server.uri()).send().await.expect("response"));
    }
    let [within, over, cancelled] = <[_; 3]>::try_from(responses).expect("three responses");
    let budget = RequestBudget::with_timeout(Duration::from_secs(60), 1024);
    let body = read_bounded(within, &budget, 64)
        .await
        .expect("within the limit");
    assert_eq!(body.len(), 64);
    assert_eq!(
        read_bounded(over, &budget, 63).await,
        Err(BodyError::TooLarge)
    );
    let response = cancelled;
    budget.cancellation.cancel();
    assert_eq!(
        read_bounded(response, &budget, 64).await,
        Err(BodyError::Stopped(BudgetStop::Cancelled))
    );
}
