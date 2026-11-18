use super::*;

fn at(year: i64, month: i64, day: i64) -> SystemTime {
    let seconds = days_from_civil(year, month, day) * 86_400;
    if seconds >= 0 {
        UNIX_EPOCH + Duration::from_secs(seconds.unsigned_abs())
    } else {
        UNIX_EPOCH - Duration::from_secs(seconds.unsigned_abs())
    }
}

#[test]
fn every_http_date_form_can_precede_the_unix_epoch() {
    for value in [
        "Wed, 31 Dec 1969 23:59:59 GMT",
        "Wednesday, 31-Dec-69 23:59:59 GMT",
        "Wed Dec 31 23:59:59 1969",
    ] {
        assert_eq!(
            parse_http_date(value, UNIX_EPOCH),
            Some(UNIX_EPOCH - Duration::from_secs(1)),
            "{value}"
        );
    }
}

#[test]
fn rfc850_years_follow_the_receipt_clock_and_fifty_year_boundary() {
    for (now, value, expected) in [
        (
            at(2026, 1, 1),
            "Wednesday, 01-Jan-70 00:00:00 GMT",
            at(2070, 1, 1),
        ),
        (
            at(2026, 1, 1),
            "Wednesday, 01-Jan-76 00:00:00 GMT",
            at(2076, 1, 1),
        ),
        (
            at(2026, 1, 1),
            "Friday, 02-Jan-76 00:00:00 GMT",
            at(1976, 1, 2),
        ),
        (
            at(2090, 1, 1),
            "Monday, 01-Mar-00 00:00:00 GMT",
            at(2100, 3, 1),
        ),
        (
            at(1990, 1, 1),
            "Tuesday, 29-Feb-00 00:00:00 GMT",
            at(2000, 2, 29),
        ),
        (
            UNIX_EPOCH - Duration::from_nanos(1),
            "Saturday, 01-Jan-00 00:00:00 GMT",
            at(2000, 1, 1),
        ),
    ] {
        assert_eq!(
            parse_http_date(value, now),
            Some(expected),
            "{value} at {now:?}"
        );
    }
}

#[test]
fn invalid_calendar_fields_and_unbounded_numeric_fields_are_rejected() {
    for value in [
        "Thu, 31 Feb 2028 00:00:00 GMT",
        "Mon, 29 Feb 2100 00:00:00 GMT",
        "Monday, 29-Feb-00 00:00:00 GMT",
        "Sun, 06 Nov 1994 08:49:37:00 GMT",
        "Sun, 06 Nov 1994 24:00:00 GMT",
        "Sun, 06 Nov 1994 08:60:00 GMT",
        "Sun, 06 Nov 1994 08:49:61 GMT",
        "Sun, 06 Nov 1994 08:49:37 UTC",
        "Sun, 06 Nov 10000 08:49:37 GMT",
        "Sun, 06 Nov 9223372036854775807 08:49:37 GMT",
        "Sun, 06 Nov -9223372036854775808 08:49:37 GMT",
        "Sun Nov 006 08:49:37 1994",
        "Sunday, 06-Nov-1994 08:49:37 GMT",
    ] {
        assert_eq!(parse_http_date(value, at(2090, 1, 1)), None, "{value}");
    }
}
