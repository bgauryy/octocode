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

/// Whether an error's message names a secondary limit: what a GraphQL limit
/// arriving on HTTP 200 has instead of a status.
pub(crate) fn names_secondary_limit(message: &str) -> bool {
    mentions_secondary_rate(message) || mentions_abuse_detection(message)
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
    if names_secondary_limit(body) {
        return true;
    }
    if status == 429 && remaining != Some(0) {
        return true;
    }
    status == 403 && retry_after.is_some() && remaining != Some(0)
}

/// The bucket a response's rate-limit headers describe: its
/// `x-ratelimit-resource`, or `fallback` when that is missing or blank.
pub(crate) fn rate_limit_resource(headers: &reqwest::header::HeaderMap, fallback: &str) -> String {
    headers
        .get("x-ratelimit-resource")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_owned()
}

#[cfg(test)]
mod tests;
