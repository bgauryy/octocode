use super::*;

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
