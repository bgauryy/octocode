//! Inclusive canonical ranges rendered in GitHub's provider syntax.
use octocode_contract_types::{DateRange, NumericRange};
use std::fmt::Display;

fn bounds(lower: Option<&impl Display>, upper: Option<&impl Display>) -> Option<String> {
    match (lower, upper) {
        (Some(lower), Some(upper)) => Some(format!("{lower}..{upper}")),
        (Some(lower), None) => Some(format!(">={lower}")),
        (None, Some(upper)) => Some(format!("<={upper}")),
        (None, None) => None,
    }
}

pub(crate) fn numeric(range: &NumericRange) -> Option<String> {
    bounds(range.min.as_ref(), range.max.as_ref())
}

pub(crate) fn date(range: &DateRange) -> Option<String> {
    bounds(range.from.as_ref(), range.to.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inclusive_bounds_preserve_zero_and_both_endpoints() {
        let numeric_range =
            serde_json::from_value(serde_json::json!({"min":0,"max":12})).expect("range");
        assert_eq!(numeric(&numeric_range).as_deref(), Some("0..12"));
        let date_range = serde_json::from_value(
            serde_json::json!({"from":"2026-01-01T00:00:00Z","to":"2026-02-01T00:00:00+02:00"}),
        )
        .expect("range");
        assert_eq!(
            date(&date_range).as_deref(),
            Some("2026-01-01T00:00:00Z..2026-02-01T00:00:00+02:00")
        );
    }
}
