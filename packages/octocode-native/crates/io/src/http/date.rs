//! HTTP-date grammar and clock-relative RFC 850 years (RFC 9110 section 5.6.7).
use crate::civil::{civil_from_days, days_from_civil};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SHORT_DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const LONG_DAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Parse all three HTTP-date forms. A two-digit RFC 850 year is interpreted
/// relative to `now`, with dates over fifty years ahead moved back a century.
pub fn parse_http_date(value: &str, now: SystemTime) -> Option<SystemTime> {
    let value = value.trim();
    if !value.is_ascii() {
        return None;
    }
    let (weekday, day, month, year, clock, short_year) =
        if let Some((weekday, rest)) = value.split_once(", ") {
            if SHORT_DAYS.contains(&weekday) {
                let fields = rest.split(' ').collect::<Vec<_>>();
                let [day, month, year, clock, "GMT"] = fields.as_slice() else {
                    return None;
                };
                (weekday, *day, *month, *year, *clock, false)
            } else {
                let fields = rest.split(' ').collect::<Vec<_>>();
                let [date, clock, "GMT"] = fields.as_slice() else {
                    return None;
                };
                let fields = date.split('-').collect::<Vec<_>>();
                let [day, month, year] = fields.as_slice() else {
                    return None;
                };
                (weekday, *day, *month, *year, *clock, true)
            }
        } else {
            // asctime uses a space-padded day rather than a zero-padded one.
            let fields = value
                .split(' ')
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>();
            let [weekday, month, day, clock, year] = fields.as_slice() else {
                return None;
            };
            (*weekday, *day, *month, *year, *clock, false)
        };
    let weekday = if short_year { &LONG_DAYS } else { &SHORT_DAYS }
        .iter()
        .position(|name| *name == weekday)?;
    if !value.contains(',') && !(1..=2).contains(&day.len()) {
        return None;
    }
    let day = number(day, if value.contains(',') { 2 } else { day.len() })?;
    let month = i64::try_from(MONTHS.iter().position(|name| *name == month)?).ok()? + 1;
    let (hour, minute, second) = time_of_day(clock)?;
    let mut year = number(year, if short_year { 2 } else { 4 })?;
    if short_year {
        let seconds = unix_seconds(now)?;
        let (now_year, now_month, now_day) = civil_from_days(seconds.div_euclid(86_400));
        let limit = now_year.checked_add(50)?;
        year += limit.div_euclid(100).checked_mul(100)?;
        if (year, month, day, hour, minute, second)
            > (
                limit,
                now_month,
                now_day,
                seconds.rem_euclid(86_400) / 3600,
                seconds.rem_euclid(3600) / 60,
                seconds.rem_euclid(60),
            )
        {
            year = year.checked_sub(100)?;
        }
    }
    if !(1..=31).contains(&day) {
        return None;
    }
    let days = days_from_civil(year, month, day);
    if civil_from_days(days) != (year, month, day)
        || usize::try_from((days + 4).rem_euclid(7)).ok()? != weekday
    {
        return None;
    }
    let seconds = days
        .checked_mul(86_400)?
        .checked_add(hour * 3600 + minute * 60 + second)?;
    if seconds >= 0 {
        UNIX_EPOCH.checked_add(Duration::from_secs(u64::try_from(seconds).ok()?))
    } else {
        #[cfg_attr(
            octocode_clippy_ratchet,
            expect(
                clippy::disallowed_methods,
                reason = "Converts pre-epoch HTTP-date seconds into an epoch offset, not a work limit"
            )
        )]
        let before_epoch = Duration::from_secs(seconds.unsigned_abs());
        UNIX_EPOCH.checked_sub(before_epoch)
    }
}

fn number(value: &str, width: usize) -> Option<i64> {
    (value.len() == width && !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse().ok())
        .flatten()
}

fn time_of_day(value: &str) -> Option<(i64, i64, i64)> {
    let fields = value.split(':').collect::<Vec<_>>();
    let [hour, minute, second] = fields.as_slice() else {
        return None;
    };
    let (hour, minute, second) = (number(hour, 2)?, number(minute, 2)?, number(second, 2)?);
    ((0..24).contains(&hour) && (0..60).contains(&minute) && (0..=60).contains(&second))
        .then_some((hour, minute, second))
}

fn unix_seconds(time: SystemTime) -> Option<i64> {
    match time.duration_since(UNIX_EPOCH) {
        Ok(elapsed) => i64::try_from(elapsed.as_secs()).ok(),
        Err(before) => {
            let elapsed = before.duration();
            i64::try_from(elapsed.as_secs())
                .ok()?
                .checked_add(i64::from(elapsed.subsec_nanos() != 0))?
                .checked_neg()
        }
    }
}

#[cfg(test)]
mod tests {
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
}
