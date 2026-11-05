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
    /// The body arrived and does not decode as its `Content-Encoding` says:
    /// asking again gets the same bytes.
    Decode,
}

/// What stopped a body stream. reqwest reports every failure of the stream as
/// a decode error, so its cause tells them apart: the decoder fails with an I/O
/// error of its own, and the connection with the HTTP client's error.
fn body_failure(error: &reqwest::Error) -> BodyError {
    let decoder = std::error::Error::source(error)
        .is_some_and(|cause| cause.downcast_ref::<std::io::Error>().is_some());
    if error.is_decode() && decoder {
        BodyError::Decode
    } else {
        BodyError::Transport
    }
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
        let chunk = chunk.map_err(|error| body_failure(&error))?;
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
mod tests;
