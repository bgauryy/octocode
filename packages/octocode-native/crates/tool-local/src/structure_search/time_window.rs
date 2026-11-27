//! Relative time bounds: an ISO 8601 duration back from now (`-P1D`,
//! `-PT6H`) in `modifiedAt` or `accessedAt`, resolved once per query, at the
//! query's start, to the absolute bound the listing applies, its cursor
//! carries and its response states (`window`).
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::response::result::ToolError;

/// The seconds an ISO 8601 duration back from now names: `-P`, then weeks
/// and days, then a `T` part of hours, minutes and seconds, at least one
/// number in all and one after a `T` (years and months, whose length
/// varies, are not taken). `None` for any other text.
pub(super) fn seconds_back(text: &str) -> Option<i64> {
    let rest = text.strip_prefix("-P")?;
    let (date, time) = match rest.split_once('T') {
        Some((date, time)) => (date, Some(time)),
        None => (rest, None),
    };
    let mut total = 0i64;
    let mut any = false;
    let mut add = |part: &str, units: &[(char, i64)]| -> Option<()> {
        let mut digits = String::new();
        let mut next_unit = 0;
        for ch in part.chars() {
            if ch.is_ascii_digit() {
                digits.push(ch);
                continue;
            }
            // Each unit at most once, in order, after its number.
            let at = units[next_unit..]
                .iter()
                .position(|(unit, _)| *unit == ch)?
                + next_unit;
            let count = digits.parse::<i64>().ok()?;
            total = total.checked_add(count.checked_mul(units[at].1)?)?;
            digits.clear();
            next_unit = at + 1;
            any = true;
        }
        digits.is_empty().then_some(())
    };
    add(date, &[('W', 7 * 86_400), ('D', 86_400)])?;
    if let Some(time) = time {
        // A `T` names at least one time unit.
        if time.is_empty() {
            return None;
        }
        add(time, &[('H', 3_600), ('M', 60), ('S', 1)])?;
    }
    any.then_some(total)
}

/// `value` (a bound as the query wrote it) as the absolute RFC 3339 bound
/// it names at `now` (Unix seconds), and whether it was a duration; a
/// date-time stays as written.
pub(super) fn resolved(value: &str, now: i64) -> (String, bool) {
    match seconds_back(value) {
        Some(back) => (
            octocode_runtime_base::civil_date::iso8601_secs(now.saturating_sub(back)),
            true,
        ),
        None => (value.to_owned(), false),
    }
}

/// The time-bound fields of a `files` query.
const FIELDS: [&str; 2] = ["modifiedAt", "accessedAt"];

/// `query` with each duration in its time bounds resolved at `now` (Unix
/// seconds, read once for the query), and the `window` its response states:
/// each field that held a duration, with the absolute bounds it applies.
/// `None` for the window when no bound was a duration. A window whose `to`
/// falls before its `from` once resolved is refused, as an inverted date
/// range is.
pub(super) fn resolve_query(
    query: &octocode_contract_types::StructureSearchQueryFiles,
    now: i64,
) -> Result<
    (
        octocode_contract_types::StructureSearchQueryFiles,
        Option<serde_json::Value>,
    ),
    ToolError,
> {
    let invalid = |message: String| ToolError::new(ErrorCode::InvalidInput, message);
    let mut fields = serde_json::to_value(query).map_err(|error| invalid(error.to_string()))?;
    let mut window = serde_json::Map::new();
    for field in FIELDS {
        let Some(range) = fields
            .get_mut(field)
            .and_then(serde_json::Value::as_object_mut)
        else {
            continue;
        };
        let mut relative = false;
        for bound in ["from", "to"] {
            if let Some(value) = range.get(bound).and_then(serde_json::Value::as_str) {
                let (absolute, was_duration) = resolved(value, now);
                relative |= was_duration;
                range.insert(bound.to_owned(), absolute.into());
            }
        }
        let at = |bound: &str| {
            range
                .get(bound)
                .and_then(serde_json::Value::as_str)
                .and_then(octocode_runtime_base::civil_date::parse_rfc3339)
        };
        if let (Some(from), Some(to)) = (at("from"), at("to"))
            && to < from
        {
            return Err(invalid(format!("{field}.to: to must be at least from.")));
        }
        if relative {
            window.insert(field.to_owned(), serde_json::Value::Object(range.clone()));
        }
    }
    let resolved = serde_json::from_value(fields).map_err(|error| invalid(error.to_string()))?;
    Ok((
        resolved,
        (!window.is_empty()).then_some(serde_json::Value::Object(window)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_back_from_now_name_their_seconds() {
        for (text, seconds) in [
            ("-P1D", 86_400),
            ("-P2W", 14 * 86_400),
            ("-P1W2D", 9 * 86_400),
            ("-PT6H", 6 * 3_600),
            ("-PT90M", 90 * 60),
            ("-PT30S", 30),
            ("-P1DT1H1M1S", 86_400 + 3_661),
            ("-P0D", 0),
        ] {
            assert_eq!(seconds_back(text), Some(seconds), "{text}");
        }
        for text in [
            "P1D",
            "-P",
            "-PT",
            "-P1Y",
            "-P1M",
            "-PD",
            "-P1DT",
            "-PT1D",
            "-P1D2W",
            "-P1H",
            "yesterday",
            "2026-10-01T00:00:00Z",
            "-P1.5D",
            "-P1D1D",
        ] {
            assert_eq!(seconds_back(text), None, "{text}");
        }
    }

    #[test]
    fn a_duration_resolves_against_one_now_and_a_date_time_stays() {
        let now = 1_800_000_000;
        assert_eq!(
            resolved("-P1D", now),
            (
                octocode_runtime_base::civil_date::iso8601_secs(now - 86_400),
                true
            )
        );
        assert_eq!(
            resolved("2026-10-01T00:00:00Z", now),
            ("2026-10-01T00:00:00Z".to_owned(), false)
        );
    }
}
