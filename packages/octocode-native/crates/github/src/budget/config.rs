//! Executor tuning and identity: request families, Octokit throttling groups,
//! [`ExecutorConfig`] timings and the [`LimiterKey`] every state is keyed by.
use super::now_ms;
use crate::credential_host;
use std::time::Duration;
use url::Url;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitHubResource {
    Core,
    Search,
    CodeSearch,
    Graphql,
}

impl GitHubResource {
    /// GitHub's `x-ratelimit-resource` name for this request family.
    pub(crate) fn bucket(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Search => "search",
            Self::CodeSearch => "code_search",
            Self::Graphql => "graphql",
        }
    }
}

/// Octokit throttling groups layered under the per-key global group.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Group {
    Search,
    CodeSearch,
    Graphql,
    Write,
    Auth,
}

impl Group {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::CodeSearch => "code_search",
            Self::Graphql => "graphql",
            Self::Write => "write",
            Self::Auth => "auth",
        }
    }

    /// The GitHub rate-limit bucket this group's requests draw from.
    pub(super) fn bucket(self) -> &'static str {
        match self {
            Self::Search => GitHubResource::Search.bucket(),
            Self::CodeSearch => GitHubResource::CodeSearch.bucket(),
            Self::Graphql => GitHubResource::Graphql.bucket(),
            Self::Write | Self::Auth => GitHubResource::Core.bucket(),
        }
    }
}

/// Executor tuning. [`ExecutorConfig::octokit`] mirrors
/// `@octokit/plugin-throttling`; tests shrink the timings.
#[derive(Clone, Debug)]
pub struct ExecutorConfig {
    pub global_concurrency: usize,
    pub git_concurrency: usize,
    pub search_spacing: Duration,
    pub graphql_spacing: Duration,
    pub write_spacing: Duration,
    pub code_search_per_minute: usize,
    /// Added to a primary reset before retrying (Octokit waits reset + 1s).
    pub reset_grace: Duration,
    /// Secondary-limit wait when GitHub sends no `retry-after`.
    pub secondary_default: Duration,
    pub circuit_failures: u32,
    pub circuit_open: Duration,
}

impl ExecutorConfig {
    pub(crate) fn octokit() -> Self {
        Self {
            global_concurrency: 10,
            git_concurrency: 2,
            search_spacing: Duration::from_millis(2000),
            graphql_spacing: Duration::from_millis(1000),
            write_spacing: Duration::from_millis(1000),
            code_search_per_minute: 10,
            reset_grace: Duration::from_secs(1),
            secondary_default: Duration::from_secs(60),
            circuit_failures: 5,
            circuit_open: Duration::from_secs(30),
        }
    }

    /// Epoch milliseconds from which a primary bucket that resets at
    /// `reset` (epoch seconds) may be retried: the reset plus the grace.
    pub(crate) fn reset_until_ms(&self, reset: u64) -> u64 {
        reset
            .saturating_mul(1000)
            .saturating_add(self.reset_grace.as_millis() as u64)
    }

    /// How long from now until [`Self::reset_until_ms`].
    pub(crate) fn reset_wait(&self, reset: u64) -> Duration {
        Duration::from_millis(self.reset_until_ms(reset).saturating_sub(now_ms()))
    }

    /// No pacing — used by unit tests that do not exercise throttling.
    pub(crate) fn relaxed() -> Self {
        Self {
            global_concurrency: 32,
            git_concurrency: 8,
            search_spacing: Duration::ZERO,
            graphql_spacing: Duration::ZERO,
            write_spacing: Duration::ZERO,
            code_search_per_minute: usize::MAX,
            reset_grace: Duration::ZERO,
            ..Self::octokit()
        }
    }

    pub(super) fn spacing(&self, group: Group) -> Duration {
        match group {
            Group::Search | Group::CodeSearch => self.search_spacing,
            Group::Graphql => self.graphql_spacing,
            Group::Write => self.write_spacing,
            Group::Auth => Duration::ZERO,
        }
    }
}

/// Limiter identity: the API host (`api.github.com` folds to `github.com`,
/// explicit ports kept) and a short SHA-256 digest of the token.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct LimiterKey {
    pub(super) host: String,
    pub(super) token: String,
}

impl LimiterKey {
    pub(crate) fn new(host: &str, token: Option<&str>) -> Self {
        let host = credential_host(&host.trim().to_ascii_lowercase()).to_owned();
        let token = token.map(secrecy::SecretString::from);
        let token = crate::Partition::new(host.as_str(), token.as_ref())
            .credential_tag()
            .to_owned();
        Self { host, token }
    }

    /// Key for any URL on the host (API, web origin, or git remote).
    pub fn for_url(url: &Url, token: Option<&str>) -> Self {
        let host = url.host_str().unwrap_or_default();
        match url.port() {
            Some(port) => Self::new(&format!("{host}:{port}"), token),
            None => Self::new(host, token),
        }
    }

    pub(super) fn file_name(&self) -> String {
        let host: String = self
            .host
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        format!("{host}-{}.json", self.token)
    }
}
