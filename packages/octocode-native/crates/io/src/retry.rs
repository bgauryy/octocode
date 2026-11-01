//! The one retry owner every HTTP client shares (GitHub, artifact
//! registries, classification): response headers, the provider-requested
//! delay, jittered [`Backoff`], [`classify_status`], and the bounded body
//! reader [`read_bounded`]. Each client keeps its own constants and policy.
use crate::budget::{BudgetStop, RequestBudget};
use crate::civil::days_from_civil;
use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, RETRY_AFTER};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Full-jitter exponential backoff: uniform in `[0, base * 2^attempt]`,
/// bounded by `cap`, never below `floor` (so `Retry-After: 0` cannot
/// hot-loop where a floor is set).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Backoff {
    pub base: Duration,
    pub cap: Duration,
    pub floor: Duration,
}

impl Backoff {
    /// The jittered wait before retry `attempt` (0-based).
    pub fn next_delay(&self, attempt: u32) -> Duration {
        full_jitter(self.base, attempt, self.cap).max(self.floor)
    }

    /// The provider's requested delay (floored), else [`Self::next_delay`].
    pub fn after(&self, requested: Option<Duration>, attempt: u32) -> Duration {
        requested.map_or_else(|| self.next_delay(attempt), |delay| delay.max(self.floor))
    }
}

/// What an HTTP status says about retrying. The classification and its
/// retry verdict ([`StatusClass::is_retryable`]) are shared; each client
/// only bounds how often it retries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusClass {
    /// 2xx.
    Success,
    /// 3xx.
    Redirect,
    /// 429: the client is over its rate.
    RateLimited,
    /// 503 or 529: the provider sheds load.
    Overloaded,
    /// 408: the provider timed the request out.
    Timeout,
    /// 500, 502, 504: a transient server failure.
    Transient,
    /// Any other 5xx (501, 505, ...).
    ServerFault,
    /// Any other status.
    Final,
}

impl StatusClass {
    /// Any 5xx class.
    pub fn is_server(self) -> bool {
        matches!(self, Self::Overloaded | Self::Transient | Self::ServerFault)
    }

    /// Worth another attempt: a rate limit, a provider timeout (408) or any
    /// 5xx. Every client shares this verdict; each still bounds how often
    /// it retries.
    pub fn is_retryable(self) -> bool {
        self.is_server() || matches!(self, Self::RateLimited | Self::Timeout)
    }
}

pub fn classify_status(status: u16) -> StatusClass {
    match status {
        200..=299 => StatusClass::Success,
        300..=399 => StatusClass::Redirect,
        429 => StatusClass::RateLimited,
        503 | 529 => StatusClass::Overloaded,
        408 => StatusClass::Timeout,
        500 | 502 | 504 => StatusClass::Transient,
        500..=599 => StatusClass::ServerFault,
        _ => StatusClass::Final,
    }
}

/// A send failure worth another attempt: the connection could not be
/// opened, timed out, or failed while the request was in flight.
pub fn is_retryable_send(error: &reqwest::Error) -> bool {
    error.is_connect() || error.is_timeout() || error.is_request()
}

/// Why a bounded body read stopped short.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BodyError {
    /// The request budget stopped the read.
    Stopped(BudgetStop),
    /// The body exceeded the limit.
    TooLarge,
    /// The connection failed mid-body.
    Transport,
}

/// A response body read within `budget` and at most `limit` bytes. A body
/// that stalls after its headers is bounded by the request deadline too.
pub async fn read_bounded(
    response: reqwest::Response,
    budget: &RequestBudget,
    limit: usize,
) -> Result<Bytes, BodyError> {
    let mut body = BytesMut::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = budget
        .wait(stream.next())
        .await
        .map_err(BodyError::Stopped)?
    {
        let chunk = chunk.map_err(|_| BodyError::Transport)?;
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(BodyError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body.freeze())
}

/// A header's non-negative integer value, surrounding whitespace ignored.
pub fn header_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers.get(name)?.to_str().ok()?.trim().parse().ok()
}

fn header_seconds(headers: &HeaderMap, name: &str) -> Option<f64> {
    headers
        .get(name)?
        .to_str()
        .ok()?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
}

/// Provider-requested delay: `retry-after-ms`, then `Retry-After` as
/// delta-seconds or an RFC 9110 HTTP-date (relative to `now`), each bounded
/// by `max`.
pub fn retry_after_delay(headers: &HeaderMap, max: Duration, now: SystemTime) -> Option<Duration> {
    if let Some(milliseconds) = header_seconds(headers, "retry-after-ms") {
        return Some(Duration::from_secs_f64(
            (milliseconds / 1000.0).min(max.as_secs_f64()),
        ));
    }
    if let Some(seconds) = header_seconds(headers, RETRY_AFTER.as_str()) {
        return Some(Duration::from_secs_f64(seconds.min(max.as_secs_f64())));
    }
    let date = parse_http_date(headers.get(RETRY_AFTER)?.to_str().ok()?)?;
    Some(date.duration_since(now).unwrap_or(Duration::ZERO).min(max))
}

/// Parse an RFC 9110 IMF-fixdate (`Sun, 06 Nov 1994 08:49:37 GMT`).
fn parse_http_date(value: &str) -> Option<SystemTime> {
    let (_, rest) = value.trim().split_once(", ")?;
    let mut parts = rest.split_ascii_whitespace();
    let day = parts.next()?.parse::<i64>().ok()?;
    let month_name = parts.next()?;
    let month_index = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|name| *name == month_name)?;
    let month = 1 + i64::try_from(month_index).ok()?;
    let year = parts.next()?.parse::<i64>().ok()?;
    let mut clock = parts.next()?.split(':').map(str::parse::<i64>);
    let (hour, minute, second) = (
        clock.next()?.ok()?,
        clock.next()?.ok()?,
        clock.next()?.ok()?,
    );
    if parts.next()? != "GMT"
        || parts.next().is_some()
        || !(1..=31).contains(&day)
        || !(0..24).contains(&hour)
        || !(0..60).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return None;
    }
    let seconds = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second;
    SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(u64::try_from(seconds).ok()?))
}

/// Full-jitter exponential backoff: uniform in `[0, base * 2^attempt]`,
/// bounded by `cap`.
fn full_jitter(base: Duration, attempt: u32, cap: Duration) -> Duration {
    let ceiling = u64::try_from(
        base.saturating_mul(1_u32 << attempt.min(16))
            .min(cap)
            .as_millis(),
    )
    .unwrap_or(u64::MAX);
    if ceiling == 0 {
        return Duration::ZERO;
    }
    let mut bytes = [0_u8; 8];
    let random = if getrandom::fill(&mut bytes).is_ok() {
        u64::from_le_bytes(bytes)
    } else {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.subsec_nanos() as u64)
            .unwrap_or(0)
    };
    Duration::from_millis(random % (ceiling + 1))
}

#[cfg(test)]
mod tests {
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
        let client = reqwest::Client::new();
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
}
