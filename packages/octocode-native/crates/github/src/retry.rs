//! Retry timing shared by every HTTP client: response headers, the
//! provider-requested delay, and full-jitter backoff.
use reqwest::header::{HeaderMap, RETRY_AFTER};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

/// Provider-requested delay in its delta forms: `retry-after-ms`, then
/// `Retry-After` delta-seconds, each bounded by `max`. An HTTP-date
/// `Retry-After` is not a delta and yields `None`.
pub fn retry_after_delay(headers: &HeaderMap, max: Duration) -> Option<Duration> {
    if let Some(milliseconds) = header_seconds(headers, "retry-after-ms") {
        return Some(Duration::from_secs_f64(
            (milliseconds / 1000.0).min(max.as_secs_f64()),
        ));
    }
    header_seconds(headers, RETRY_AFTER.as_str())
        .map(|seconds| Duration::from_secs_f64(seconds.min(max.as_secs_f64())))
}

/// Full-jitter exponential backoff: uniform in `[0, base * 2^attempt]`,
/// bounded by `cap`.
pub fn full_jitter(base: Duration, attempt: u32, cap: Duration) -> Duration {
    let ceiling = base
        .saturating_mul(1_u32 << attempt.min(16))
        .min(cap)
        .as_millis() as u64;
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
        let both = headers(&[("retry-after-ms", "1500"), ("retry-after", "9")]);
        assert_eq!(
            retry_after_delay(&both, max),
            Some(Duration::from_millis(1500))
        );
        let seconds = headers(&[("retry-after", " 2 ")]);
        assert_eq!(
            retry_after_delay(&seconds, max),
            Some(Duration::from_secs(2))
        );
        let huge = headers(&[("retry-after", "1e300")]);
        assert_eq!(retry_after_delay(&huge, max), Some(max));
        for invalid in ["-1", "NaN", "Sun, 06 Nov 1994 08:49:37 GMT"] {
            let map = headers(&[("retry-after", invalid)]);
            assert_eq!(retry_after_delay(&map, max), None, "{invalid}");
        }
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
}
