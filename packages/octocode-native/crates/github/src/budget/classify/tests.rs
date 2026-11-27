use super::*;

#[test]
fn detects_secondary_limit_from_body_even_when_remaining_is_positive() {
    assert!(is_secondary_rate_limit(
        403,
        Some(21),
        None,
        "You have exceeded a secondary rate limit. Please wait a few minutes"
    ));
    assert!(!is_primary_rate_limit(403, Some(21), ""));
    assert!(is_primary_rate_limit(403, Some(0), ""));
    // Word boundary: "secondary rates" / 404 bodies do not qualify.
    assert!(!mentions_secondary_rate("nonsecondary ratex"));
    assert!(!is_secondary_rate_limit(
        404,
        None,
        None,
        "secondary rate limit"
    ));
}
