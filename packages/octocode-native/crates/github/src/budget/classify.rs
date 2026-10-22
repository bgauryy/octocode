//! Rate-limit classification of a GitHub error response (Octokit parity).

/// Octokit: `/\bsecondary rate\b/i` on the error message.
pub(crate) fn mentions_secondary_rate(body: &str) -> bool {
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

/// Octokit: the legacy abuse-detection 403 is a secondary limit.
fn mentions_abuse_detection(body: &str) -> bool {
    body.to_ascii_lowercase().contains("abuse detection")
}

/// GitHub's primary-limit message (`API rate limit exceeded for …`).
fn mentions_primary_rate(body: &str) -> bool {
    body.to_ascii_lowercase().contains("rate limit exceeded")
}

/// An exhausted bucket (`x-ratelimit-remaining: 0`) on a 403/429, or, when
/// a proxy or GitHub Enterprise sent no `x-ratelimit-*` headers, GitHub's
/// own rate-limit message.
pub(crate) fn is_primary_rate_limit(status: u16, remaining: Option<u64>, body: &str) -> bool {
    matches!(status, 403 | 429)
        && (remaining == Some(0) || (remaining.is_none() && mentions_primary_rate(body)))
}

pub(crate) fn is_secondary_rate_limit(
    status: u16,
    remaining: Option<u64>,
    retry_after: Option<u64>,
    body: &str,
) -> bool {
    if !matches!(status, 403 | 429) {
        return false;
    }
    if mentions_secondary_rate(body) || mentions_abuse_detection(body) {
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
}
