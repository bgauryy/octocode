//! Rate-limit classification of a GitHub error response (Octokit parity).

/// Octokit: `/\bsecondary rate\b/i` on the error message.
pub fn mentions_secondary_rate(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.match_indices("secondary rate").any(|(index, _)| {
        let before_ok = index == 0
            || !lower.as_bytes()[index - 1].is_ascii_alphanumeric()
                && lower.as_bytes()[index - 1] != b'_';
        let end = index + "secondary rate".len();
        let after_ok = lower
            .as_bytes()
            .get(end)
            .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_');
        before_ok && after_ok
    })
}

pub fn is_primary_rate_limit(status: u16, remaining: Option<u64>) -> bool {
    remaining == Some(0) && matches!(status, 403 | 429)
}

pub fn is_secondary_rate_limit(
    status: u16,
    remaining: Option<u64>,
    retry_after: Option<u64>,
    body: &str,
) -> bool {
    if !matches!(status, 403 | 429) {
        return false;
    }
    if mentions_secondary_rate(body) {
        return true;
    }
    if status == 429 && remaining != Some(0) {
        return true;
    }
    status == 403 && retry_after.is_some() && remaining != Some(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_secondary_limit_from_body_even_when_remaining_is_positive() {
        assert!(is_secondary_rate_limit(
            403,
            Some(21),
            None,
            "You have exceeded a secondary rate limit. Please wait a few minutes"
        ));
        assert!(!is_primary_rate_limit(403, Some(21)));
        assert!(is_primary_rate_limit(403, Some(0)));
        // Word boundary: "secondary rates" / 404 bodies do not qualify.
        assert!(!mentions_secondary_rate("nonsecondary ratex"));
        assert!(!is_secondary_rate_limit(
            404,
            None,
            None,
            "secondary rate limit"
        ));
    }
}
