//! The provider-requested retry delay, shared by every runtime HTTP client:
//! the delta forms come from `octocode_github::retry_after_delay`; this adds
//! the RFC 9110 HTTP-date form of `Retry-After`.
use reqwest::header::{HeaderMap, RETRY_AFTER};
use std::time::{Duration, SystemTime};

/// Provider-requested retry delay: `retry-after-ms`, then `Retry-After` as
/// delta-seconds or an HTTP-date (relative to `now`), each bounded by `max`.
pub(crate) fn retry_after(headers: &HeaderMap, max: Duration, now: SystemTime) -> Option<Duration> {
    if let Some(delay) = octocode_github::retry_after_delay(headers, max) {
        return Some(delay);
    }
    let date = headers
        .get(RETRY_AFTER)?
        .to_str()
        .ok()
        .and_then(parse_http_date)?;
    Some(date.duration_since(now).unwrap_or(Duration::ZERO).min(max))
}

/// Parse an RFC 9110 IMF-fixdate (`Sun, 06 Nov 1994 08:49:37 GMT`).
fn parse_http_date(value: &str) -> Option<SystemTime> {
    let (_, rest) = value.trim().split_once(", ")?;
    let mut parts = rest.split_ascii_whitespace();
    let day = parts.next()?.parse::<i64>().ok()?;
    let month = match parts.next()? {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
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
    let seconds = crate::civil_date::days_from_civil(year, month, day) * 86_400
        + hour * 3600
        + minute * 60
        + second;
    let seconds = u64::try_from(seconds).ok()?;
    SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    const MAX: Duration = Duration::from_secs(24 * 60 * 60);

    #[test]
    fn retry_after_parses_milliseconds_seconds_and_http_dates() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(784_111_777); // Sun, 06 Nov 1994 08:49:37 GMT
        let mut ms = HeaderMap::new();
        ms.insert("retry-after-ms", HeaderValue::from_static("1500"));
        ms.insert(RETRY_AFTER, HeaderValue::from_static("9"));
        assert_eq!(
            retry_after(&ms, MAX, now),
            Some(Duration::from_millis(1500))
        );

        let mut secs = HeaderMap::new();
        secs.insert(RETRY_AFTER, HeaderValue::from_static("2"));
        assert_eq!(retry_after(&secs, MAX, now), Some(Duration::from_secs(2)));

        let mut date = HeaderMap::new();
        date.insert(
            RETRY_AFTER,
            HeaderValue::from_static("Sun, 06 Nov 1994 08:50:07 GMT"),
        );
        assert_eq!(retry_after(&date, MAX, now), Some(Duration::from_secs(30)));
        // A date in the past means "now".
        date.insert(
            RETRY_AFTER,
            HeaderValue::from_static("Sat, 05 Nov 1994 08:49:37 GMT"),
        );
        assert_eq!(retry_after(&date, MAX, now), Some(Duration::ZERO));

        for invalid in ["soon", "-1", "Sun, 06 Nov 1994 25:00:00 GMT", ""] {
            let mut headers = HeaderMap::new();
            headers.insert(RETRY_AFTER, HeaderValue::from_str(invalid).expect("header"));
            assert_eq!(retry_after(&headers, MAX, now), None, "{invalid}");
        }
        assert_eq!(retry_after(&HeaderMap::new(), MAX, now), None);
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
    fn far_future_dates_are_bounded_by_max() {
        let mut date = HeaderMap::new();
        date.insert(
            RETRY_AFTER,
            HeaderValue::from_static("Fri, 31 Dec 9999 23:59:59 GMT"),
        );
        let max = Duration::from_secs(60);
        assert_eq!(retry_after(&date, max, SystemTime::UNIX_EPOCH), Some(max));
    }
}
