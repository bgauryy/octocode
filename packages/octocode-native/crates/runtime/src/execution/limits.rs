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

/// One page of classification or one `sections` batch.
#[derive(Clone, Copy, Debug)]
pub struct SelectionLimits {
    /// Characters of states and questions one page sends before it stops and
    /// shows what it found: the checkpoint that lets a caller see the cost of
    /// a large walk before paying for it (about 60k input tokens at most). It
    /// never loses work: a first unit always runs whatever its size, and the
    /// next page takes the rest.
    pub max_chars: usize,
}

impl RuntimeLimits {
    pub const DEFAULT: Self = Self {
        network_timeout: Duration::from_secs(30),
        max_retries: 3,
        auto_page_chars: crate::id::DEFAULT_RESPONSE_CHARS,
        capture_ttl: Duration::from_secs(24 * 60 * 60),
        classification_concurrency: 10,
        selection: SelectionLimits { max_chars: 131_072 },
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

    /// The automatic page size is core's `limits`, read from the embedded
    /// contract: a change in core changes native behavior, with no native
    /// copy of the value.
    #[test]
    fn page_default_is_the_contract_limit() {
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
    }
}
