//! Internal limits: code constants, never user configuration. `DEFAULT` is
//! the one place each value lives; `ToolRuntime::with_limits` is the test
//! seam for a non-default value. Nothing here reads the environment or a
//! file.
use std::time::Duration;

/// Every internal limit a runtime's tools run under.
#[derive(Clone, Copy, Debug)]
pub struct RuntimeLimits {
    /// Request time of one network call (GitHub, registries, the
    /// classification provider), within the call's own deadline.
    pub network_timeout: Duration,
    /// Retries after a failed request.
    pub max_retries: u8,
    /// Longest GitHub rate-limit wait (primary or secondary limit,
    /// code-search window) before a call fails with `retryAfter`. It stays
    /// under the 60 s tool-call timeout some MCP hosts apply.
    pub max_rate_limit_wait: Duration,
    /// Characters of the automatic response page when a call names no
    /// `responseChars`.
    pub auto_page_chars: usize,
    /// Retention of captured URL evidence.
    pub capture_ttl: Duration,
    /// Process-wide in-flight classification provider requests per endpoint.
    pub classification_concurrency: usize,
    /// One context selection operation.
    pub selection: SelectionLimits,
    /// Client-side GitHub request spacing: octokit's in production; tests
    /// that do not exercise throttling run unpaced.
    pub github_pacing: octocode_github::GitHubPacing,
}

/// One context selection operation.
#[derive(Clone, Copy, Debug)]
pub struct SelectionLimits {
    /// Complete evidence units assessed; unassessed units stay unknown and
    /// recoverable.
    pub max_candidates: usize,
    /// Total characters of serialized sanitized candidate states, including
    /// source metadata.
    pub max_chars: usize,
    /// Deadline, bounded by the enclosing request deadline.
    pub timeout: Duration,
}

impl RuntimeLimits {
    pub const DEFAULT: Self = Self {
        network_timeout: Duration::from_secs(30),
        max_retries: 3,
        max_rate_limit_wait: Duration::from_secs(45),
        auto_page_chars: crate::id::DEFAULT_RESPONSE_CHARS,
        capture_ttl: Duration::from_secs(24 * 60 * 60),
        classification_concurrency: 10,
        selection: SelectionLimits {
            max_candidates: crate::id::CLASSIFICATION_UNITS_PER_PAGE,
            max_chars: 131_072,
            timeout: Duration::from_secs(10),
        },
        github_pacing: octocode_github::GitHubPacing::Octokit,
    };
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::RuntimeLimits;

    /// The automatic page size and the units one classified page judges are
    /// core's `limits`, read from the embedded contract: a change in core
    /// changes native behavior, with no native copy of either value.
    #[test]
    fn page_and_classification_defaults_are_the_contract_limits() {
        let contract: serde_json::Value =
            serde_json::from_str(crate::contracts::contract_json()).expect("embedded contract");
        let limit = |name: &str| {
            contract["limits"][name]
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or_else(|| panic!("contract limits.{name}"))
        };
        assert_eq!(
            RuntimeLimits::DEFAULT.auto_page_chars,
            limit("defaultResponseChars")
        );
        assert_eq!(
            RuntimeLimits::DEFAULT.selection.max_candidates,
            limit("classificationUnitsPerPage")
        );
    }
}
