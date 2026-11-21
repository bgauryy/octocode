//! Internal limits: code constants, never user configuration. `DEFAULT` is
//! the one place each value lives; `ToolRuntime::with_limits` is the test
//! seam for a non-default value. Nothing here reads the environment or a
//! file.
use std::time::Duration;

/// Every internal limit a runtime's tools run under.
#[derive(Clone, Copy, Debug)]
pub struct RuntimeLimits {
    /// Request time of one network call (GitHub, registries), within the
    /// call's own deadline.
    pub network_timeout: Duration,
    /// Request time of one classification evaluation. `None`: the call's own
    /// deadline decides. A model's time follows its input, and the provider
    /// documents no time limit, so a fixed client cap would abandon an answer
    /// the provider is still computing; this is the typed test seam for a
    /// shorter wait.
    pub classification_timeout: Option<Duration>,
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
    /// OSV's API base, securityCheck's source of known vulnerabilities: the
    /// public API in production; tests point it at a mock server.
    pub osv_api: &'static str,
    /// How long a call on the MCP channel may spend in waits that produce
    /// nothing yet, when the caller set no budget of its own: an lspSearch
    /// row's wait for a language server that is still indexing, and a GitHub
    /// row's client-side waits (quota pacing, a rate-limit reset, a
    /// `retry-after`). MCP hosts give up on a request after 60 s by default
    /// (the MCP SDKs' request timeout) and drop the answer, so a wait that
    /// would outlast the host is not sat out: the row answers what it knows
    /// (`languageServerIndexing`, or `rateLimited` with its `retryAfter`)
    /// with the call that runs it again, which reaches the host where the
    /// waited-for answer would not. The margin under 60 s covers shaping and
    /// sending the reply. It ends only those waits: it never times a request
    /// out, and a request already sent is never cut by it. Not a setting: a
    /// caller that grants more time says so with its own budget, which wins,
    /// and the CLI, whose caller sets its own wait, has no default.
    pub mcp_reply_budget: Duration,
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
        classification_timeout: None,
        max_retries: 3,
        auto_page_chars: crate::id::DEFAULT_RESPONSE_CHARS,
        capture_ttl: Duration::from_secs(24 * 60 * 60),
        classification_concurrency: 10,
        selection: SelectionLimits { max_chars: 131_072 },
        github_pacing: octocode_github::GitHubPacing::Octokit,
        osv_api: "https://api.osv.dev/v1",
        #[cfg_attr(
            octocode_clippy_ratchet,
            expect(
                clippy::disallowed_methods,
                reason = "The limit itself, stated where the ban sends every duration: the MCP hosts' 60 s request timeout less the time to shape and send the reply; it ends idle waits only and cuts no request or evidence"
            )
        )]
        mcp_reply_budget: Duration::from_secs(50),
    };
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests;
