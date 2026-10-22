//! Proleptic Gregorian day arithmetic (Howard Hinnant's `days_from_civil` /
//! `civil_from_days`), shared by every timestamp parser and formatter so the
//! runtime needs no date crate.

use octocode_io::civil::{civil_from_days, days_from_civil};

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Wall-clock time since the Unix epoch: the one clock every runtime reader
/// shares. A clock set before the epoch reads as zero rather than failing,
/// so a timestamp, TTL or expiry computed from it stays defined.
fn since_epoch() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

/// Seconds since the Unix epoch, saturating at `i64::MAX`.
pub(crate) fn unix_secs() -> i64 {
    i64::try_from(since_epoch().as_secs()).unwrap_or(i64::MAX)
}

/// Milliseconds since the Unix epoch, saturating at `i64::MAX`.
pub(crate) fn unix_millis() -> i64 {
    i64::try_from(since_epoch().as_millis()).unwrap_or(i64::MAX)
}

/// Nanoseconds since the Unix epoch.
pub(crate) fn unix_nanos() -> u128 {
    since_epoch().as_nanos()
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ` for `ttl` from now.
pub(crate) fn iso8601_millis_after(ttl: Duration) -> String {
    let ttl = i64::try_from(ttl.as_millis()).unwrap_or(i64::MAX);
    iso8601_millis(unix_millis().saturating_add(ttl))
}

/// `YYYY-MM-DDTHH:MM:SSZ` for seconds since the Unix epoch.
pub(crate) fn iso8601_secs(secs: i64) -> String {
    let (date, clock) = date_and_clock(secs);
    format!("{date}T{clock}Z")
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ` for milliseconds since the Unix epoch.
pub(crate) fn iso8601_millis(millis: i64) -> String {
    let (date, clock) = date_and_clock(millis.div_euclid(1_000));
    format!("{date}T{clock}.{:03}Z", millis.rem_euclid(1_000))
}

/// `YYYY-MM-DD` (UTC) for milliseconds since the Unix epoch.
pub(crate) fn date_millis(millis: i64) -> String {
    date(millis.div_euclid(86_400_000))
}

fn date(days: i64) -> String {
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn date_and_clock(secs: i64) -> (String, String) {
    let rem = secs.rem_euclid(86_400);
    (
        date(secs.div_euclid(86_400)),
        format!("{:02}:{:02}:{:02}", rem / 3_600, rem % 3_600 / 60, rem % 60),
    )
}

pub(crate) fn has_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let digits = |range: std::ops::Range<usize>| -> Option<u32> {
        let part = &bytes[range];
        part.iter()
            .all(u8::is_ascii_digit)
            .then(|| part.iter().fold(0, |acc, d| acc * 10 + u32::from(d - b'0')))
    };
    let (Some(year), Some(month), Some(day)) = (digits(0..4), digits(5..7), digits(8..10)) else {
        return false;
    };
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=max_day).contains(&day)
}

/// UTC epoch seconds and nanoseconds with an optional fraction and a `Z` or
/// `±hh:mm` offset. Sub-nanosecond digits are truncated.
pub(crate) fn parse_rfc3339(value: &str) -> Option<(i64, u32)> {
    let bytes = value.as_bytes();
    let number = |range: std::ops::Range<usize>| -> Option<i64> {
        let part = bytes.get(range)?;
        part.iter()
            .all(u8::is_ascii_digit)
            .then(|| part.iter().fold(0, |acc, d| acc * 10 + i64::from(d - b'0')))
    };
    if !has_iso_date(value.get(..10)?)
        || bytes.get(10) != Some(&b'T')
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
    {
        return None;
    }
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let mut nanos = 0;
    let mut rest = value.get(19..)?;
    if let Some(fraction) = rest.strip_prefix('.') {
        let digits = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        let precision = digits.min(9);
        nanos = fraction[..precision].parse::<u32>().ok()? * 10_u32.pow(9 - precision as u32);
        rest = &fraction[digits..];
    }
    let offset = match rest.as_bytes() {
        [b'Z' | b'z'] => 0,
        [sign @ (b'+' | b'-'), h1, h2, b':', m1, m2] => {
            let digits = [*h1, *h2, *m1, *m2];
            if !digits.iter().all(u8::is_ascii_digit) {
                return None;
            }
            let [h1, h2, m1, m2] = digits.map(|d| i64::from(d - b'0'));
            let minutes = (h1 * 10 + h2) * 60 + m1 * 10 + m2;
            if *sign == b'-' { -minutes } else { minutes }
        }
        _ => return None,
    };
    let days = days_from_civil(number(0..4)?, number(5..7)?, number(8..10)?);
    Some((
        days * 86_400 + hour * 3_600 + minute * 60 + second - offset * 60,
        nanos,
    ))
}

/// Milliseconds from a validated timestamp, truncated below millisecond precision.
pub(crate) fn rfc3339_millis(value: &str) -> Option<i64> {
    let (secs, nanos) = parse_rfc3339(value)?;
    secs.checked_mul(1_000)?
        .checked_add(i64::from(nanos / 1_000_000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_preserve_fractional_seconds_and_negative_epochs() {
        for value in [-1, 0, 1, 951_825_599_123, 1_900_000_000_999] {
            assert_eq!(rfc3339_millis(&iso8601_millis(value)), Some(value));
        }
        for (fraction, nanos) in [
            ("1", 100_000_000),
            ("123", 123_000_000),
            ("123456789", 123_456_789),
            ("1234567895", 123_456_789),
        ] {
            assert_eq!(
                parse_rfc3339(&format!("1970-01-01T00:00:00.{fraction}Z")),
                Some((0, nanos))
            );
        }
        assert_eq!(parse_rfc3339("1970-01-01T02:30:00+02:30"), Some((0, 0)));
        for bad in [
            "2026-01-01T00x00y00z000Z",
            "2026-02-31T00:00:00.000Z",
            "2026-01-01T00:00:00.Z",
        ] {
            assert_eq!(parse_rfc3339(bad), None);
        }
    }

    #[test]
    fn round_trips_across_leap_and_century_boundaries() {
        for days in -800_000..800_000 {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(days_from_civil(year, month, day), days, "{days}");
        }
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(2000, 2, 29), 11_016);
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn formats_seconds_and_milliseconds_as_utc_iso8601() {
        assert_eq!(iso8601_secs(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601_secs(951_825_599), "2000-02-29T11:59:59Z");
        assert_eq!(iso8601_secs(-1), "1969-12-31T23:59:59Z");
        assert_eq!(
            iso8601_millis(1_600_000_000_443),
            "2020-09-13T12:26:40.443Z"
        );
        assert_eq!(iso8601_millis(-1), "1969-12-31T23:59:59.999Z");
    }
}
