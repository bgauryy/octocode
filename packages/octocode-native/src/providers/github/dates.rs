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
}
