//! Convert relative GitHub history windows (`30d`) into ISO-8601.

pub struct DateWindow {
    pub value: Option<String>,
    pub warning: Option<String>,
}

impl DateWindow {
    /// True when both bounds resolved and `self` falls strictly after `other`
    /// (used to reject `since` > `until`). Date-only values compare as
    /// midnight UTC; timezone offsets are not normalized.
    pub(crate) fn is_after(&self, other: &DateWindow) -> bool {
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
        let now = crate::civil_date::unix_secs();
        return DateWindow {
            value: Some(crate::civil_date::iso8601_secs(
                now.saturating_sub(seconds).max(0),
            )),
            warning: None,
        };
    }
    if crate::civil_date::has_iso_date(trimmed) {
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

/// A GitHub timestamp (`…Z`, `….000Z`, `…+02:00`) as UTC `YYYY-MM-DDTHH:MM:SSZ`;
/// `None` when the value is not a full timestamp.
pub fn utc_timestamp(value: &str) -> Option<String> {
    crate::civil_date::parse_rfc3339(value)
        .map(|(secs, _)| crate::civil_date::iso8601_secs(secs.max(0)))
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
