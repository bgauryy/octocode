//! Proleptic Gregorian day arithmetic (Howard Hinnant's `days_from_civil` /
//! `civil_from_days`), shared by every timestamp parser and formatter so the
//! runtime needs no date crate.

/// Days since 1970-01-01 for a proleptic Gregorian date. `month` is 1-based.
pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// `(year, month, day)` for a count of days since 1970-01-01; month and day
/// are 1-based.
pub(crate) fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
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

fn date_and_clock(secs: i64) -> (String, String) {
    let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
    let rem = secs.rem_euclid(86_400);
    (
        format!("{year:04}-{month:02}-{day:02}"),
        format!("{:02}:{:02}:{:02}", rem / 3_600, rem % 3_600 / 60, rem % 60),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
