//! Convert relative GitHub history windows (`30d`) into ISO-8601.

use std::time::{SystemTime, UNIX_EPOCH};

pub struct DateWindow {
    pub value: Option<String>,
    pub warning: Option<String>,
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
    value.len() >= 10 && value.as_bytes()[4] == b'-' && value.as_bytes().get(7) == Some(&b'-')
}

pub(crate) fn rfc3339_from_epoch_secs(epoch: i64) -> String {
    iso8601(epoch)
}

pub(crate) fn epoch_secs_from_rfc3339(value: &str) -> Option<i64> {
    let value = value.trim();
    if value.len() < 10 {
        return None;
    }
    let year: i64 = value.get(0..4)?.parse().ok()?;
    if value.as_bytes().get(4) != Some(&b'-') {
        return None;
    }
    let month: u32 = value.get(5..7)?.parse().ok()?;
    if value.as_bytes().get(7) != Some(&b'-') {
        return None;
    }
    let day: u32 = value.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let rest = value.get(10..).unwrap_or("");
    let (hour, minute, second) = if rest.is_empty() {
        (0_u32, 0_u32, 0_u32)
    } else {
        let rest = rest.strip_prefix('T').or_else(|| rest.strip_prefix(' '))?;
        let hour: u32 = rest.get(0..2)?.parse().ok()?;
        if rest.as_bytes().get(2) != Some(&b':') {
            return None;
        }
        let minute: u32 = rest.get(3..5)?.parse().ok()?;
        if rest.as_bytes().get(5) != Some(&b':') {
            return None;
        }
        let second: u32 = rest.get(6..8)?.parse().ok()?;
        let mut idx = 8;
        let bytes = rest.as_bytes();
        if bytes.get(idx) == Some(&b'.') {
            idx += 1;
            while bytes.get(idx).is_some_and(u8::is_ascii_digit) {
                idx += 1;
            }
        }
        let tz = rest.get(idx..)?;
        if !matches!(tz, "Z" | "z" | "+00:00" | "-00:00" | "+0000" | "-0000") {
            return None;
        }
        if hour > 23 || minute > 59 || second > 60 {
            return None;
        }
        (hour, minute, second)
    };
    Some(
        days_from_civil(year, month, day) * 86_400
            + i64::from(hour) * 3_600
            + i64::from(minute) * 60
            + i64::from(second),
    )
}

fn iso8601(epoch: i64) -> String {
    let epoch = epoch.max(0) as u64;
    let days = epoch / 86400;
    let rem = epoch % 86400;
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    let second = rem % 60;
    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_from_days(mut z: i64) -> (i64, u32, u32) {
    z += 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if m > 2 {
        u64::from(m) - 3
    } else {
        u64::from(m) + 9
    };
    let doy = (153 * mp + 2) / 5 + u64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468
}

pub fn quote_search_keyword(keyword: &str) -> String {
    if keyword.starts_with('"') {
        return keyword.to_owned();
    }
    if keyword.chars().any(char::is_whitespace) {
        return format!("\"{}\"", keyword.replace('"', "\\\""));
    }
    keyword.to_owned()
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
    fn quotes_multiword_keywords() {
        assert_eq!(quote_search_keyword("fix login"), "\"fix login\"");
        assert_eq!(quote_search_keyword("already"), "already");
    }

    #[test]
    fn rfc3339_round_trips_unix_seconds() {
        let stamp = "2026-01-01T00:00:00Z";
        let epoch = epoch_secs_from_rfc3339(stamp).expect("parse");
        assert_eq!(rfc3339_from_epoch_secs(epoch), stamp);
        assert_eq!(
            epoch_secs_from_rfc3339("2026-01-01T00:00:00.000Z"),
            Some(epoch)
        );
    }
}
