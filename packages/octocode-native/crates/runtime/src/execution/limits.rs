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
    /// The ghCloneRepo cache.
    pub clone: CloneLimits,
    /// Process-wide in-flight classification provider requests per endpoint.
    pub classification_concurrency: usize,
    /// One context selection operation.
    pub selection: SelectionLimits,
    /// Client-side GitHub request spacing: octokit's in production; tests
    /// that do not exercise throttling run unpaced.
    pub github_pacing: octocode_github::GitHubPacing,
}

/// The on-disk ghCloneRepo cache.
#[derive(Clone, Copy, Debug)]
pub struct CloneLimits {
    /// How long a checkout stays fresh before it is re-fetched; also how
    /// long a clasify URL capture is kept.
    pub ttl: Duration,
    /// Byte cap; least-recently-used checkouts are evicted above it.
    pub(crate) max_bytes: u64,
    /// Repositories the cache keeps.
    pub(crate) max_clones: usize,
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
        auto_page_chars: 20_000,
        clone: CloneLimits {
            ttl: Duration::from_secs(24 * 60 * 60),
            max_bytes: 2 * 1024 * 1024 * 1024,
            max_clones: 50,
        },
        classification_concurrency: 10,
        selection: SelectionLimits {
            max_candidates: 64,
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
