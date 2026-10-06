//! Convert relative GitHub history windows (`30d`) into ISO-8601.

use std::time::{SystemTime, UNIX_EPOCH};

pub struct DateWindow {
    pub value: Option<String>,
    pub warning: Option<String>,
}

impl DateWindow {
    /// True when both bounds resolved and `self` falls strictly after `other`
    /// (used to reject `since` > `until`). Date-only values compare as
    /// midnight UTC; timezone offsets are not normalized.
    pub fn is_after(&self, other: &DateWindow) -> bool {
        match (self.value.as_deref(), other.value.as_deref()) {
            (Some(left), Some(right)) => comparable(left) > comparable(right),
            _ => false,
        }
    }
}

fn comparable(value: &str) -> String {
    let mut out: String = value.chars().take(19).collect();
    const MIDNIGHT: &str = "0000-00-00T00:00:00";
    if out.len() < MIDNIGHT.len() {
        out.push_str(&MIDNIGHT[out.len()..]);
    }
    out
}

pub fn resolve_date_window(value: &str) -> DateWindow {
    let trimmed = value.trim();
    let bytes = trimmed.as_bytes();
    let split = bytes
        .iter()
        .position(|c| !c.is_ascii_digit())
        .unwrap_or(bytes.len());
    if split > 0
        && split < bytes.len()
        && bytes[..split].iter().all(|c| c.is_ascii_digit())
        && trimmed[split..].trim().len() == 1
    {
        let n: i64 = trimmed[..split].parse().unwrap_or(0);
        let unit = trimmed[split..].trim().to_ascii_lowercase();
        let seconds = match unit.as_str() {
            "h" => n.saturating_mul(3600),
            "d" => n.saturating_mul(86400),
            "w" => n.saturating_mul(86400 * 7),
            "m" => n.saturating_mul(86400 * 30),
            "y" => n.saturating_mul(86400 * 365),
            _ => {
                return invalid(value);
            }
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        return DateWindow {
            value: Some(iso8601(now.saturating_sub(seconds))),
            warning: None,
        };
    }
    if looks_like_iso(trimmed) {
        return DateWindow {
            value: Some(trimmed.to_owned()),
            warning: None,
        };
    }
    invalid(value)
}

fn invalid(value: &str) -> DateWindow {
    DateWindow {
        value: None,
        warning: Some(format!(
            "\"{value}\" is not a valid date or relative window — use e.g. \"30d\", \"2w\", \"6m\", \"1y\", or an ISO date like \"2026-01-01\". Filter skipped."
        )),
    }
}

fn looks_like_iso(value: &str) -> bool {
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

/// A GitHub timestamp (`…Z`, `….000Z`, `…+02:00`) as UTC `YYYY-MM-DDTHH:MM:SSZ`;
/// `None` when the value is not a full timestamp.
pub fn utc_timestamp(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let number = |range: std::ops::Range<usize>| -> Option<i64> {
        let part = bytes.get(range)?;
        part.iter()
            .all(u8::is_ascii_digit)
            .then(|| part.iter().fold(0, |acc, d| acc * 10 + i64::from(d - b'0')))
    };
    if !looks_like_iso(value.get(..10)?)
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
    let mut rest = value.get(19..)?;
    if let Some(fraction) = rest.strip_prefix('.') {
        let digits = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
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
    let days = crate::civil_date::days_from_civil(number(0..4)?, number(5..7)?, number(8..10)?);
    Some(iso8601(
        days * 86_400 + hour * 3_600 + minute * 60 + second - offset * 60,
    ))
}

fn iso8601(epoch: i64) -> String {
    let epoch = epoch.max(0) as u64;
    let days = epoch / 86400;
    let rem = epoch % 86400;
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    let second = rem % 60;
    let (year, month, day) = crate::civil_date::civil_from_days(days as i64);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_relative_windows() {
        let window = resolve_date_window("30d");
        assert!(window.value.as_ref().is_some_and(|v| v.ends_with('Z')));
        assert!(window.warning.is_none());
    }

    #[test]
    fn keeps_iso_and_warns_on_garbage() {
        assert_eq!(
            resolve_date_window("2026-01-01T00:00:00Z").value.as_deref(),
            Some("2026-01-01T00:00:00Z")
        );
        assert!(resolve_date_window("not-a-date").warning.is_some());
    }

    #[test]
    fn rejects_out_of_range_iso_months_and_days() {
        for bad in [
            "2030-13-45",
            "2030-00-10",
            "2030-02-30",
            "2030-04-31",
            "2030-1x-01",
        ] {
            let window = resolve_date_window(bad);
            assert!(window.value.is_none(), "{bad} accepted");
            assert!(window.warning.is_some(), "{bad} not warned");
        }
        for good in ["2024-02-29", "2030-12-31", "2030-01-01T10:00:00Z"] {
            assert_eq!(resolve_date_window(good).value.as_deref(), Some(good));
        }
    }

    #[test]
    fn github_timestamps_normalize_to_utc() {
        for (raw, utc) in [
            ("2024-03-01T01:30:00+02:00", "2024-02-29T23:30:00Z"),
            ("2024-01-01T12:00:00.000-05:30", "2024-01-01T17:30:00Z"),
            ("2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z"),
            ("2026-01-01T00:00:00.123Z", "2026-01-01T00:00:00Z"),
        ] {
            assert_eq!(utc_timestamp(raw).as_deref(), Some(utc), "{raw}");
        }
        for bad in [
            "2026-01-01",
            "2026-13-01T00:00:00Z",
            "soon",
            "2026-01-01T00:00:00+2",
        ] {
            assert_eq!(utc_timestamp(bad), None, "{bad}");
        }
    }

    #[test]
    fn detects_inverted_windows_across_formats() {
        let after = |a: &str, b: &str| resolve_date_window(a).is_after(&resolve_date_window(b));
        assert!(after("2026-05-01", "2026-01-01T00:00:00Z"));
        assert!(!after("2026-01-01", "2026-01-01T12:00:00Z"));
        assert!(!after("2026-01-01T00:00:00Z", "2026-05-01"));
        assert!(!after("garbage", "2026-05-01"));
    }
}
