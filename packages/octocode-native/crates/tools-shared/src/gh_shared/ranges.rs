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

pub fn numeric(range: &NumericRange) -> Option<String> {
    bounds(range.min.as_ref(), range.max.as_ref())
}

pub fn date(range: &DateRange) -> Option<String> {
    bounds(range.from.as_ref(), range.to.as_ref())
}

#[cfg(test)]
mod tests;
