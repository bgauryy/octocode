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
    assert_eq!(parse_rfc3339("1970-01-01T02:30+02:30"), Some((0, 0)));
    for bad in [
        "2026-01-01T00x00y00z000Z",
        "2026-02-31T00:00:00.000Z",
        "2026-01-01T00:00:00.Z",
        "2026-01-01T00:00:00+24:00",
        "2026-01-01T00:00:00+00:60",
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
