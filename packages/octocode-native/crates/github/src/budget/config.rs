//! Executor tuning and identity: request families, Octokit throttling groups,
//! [`ExecutorConfig`] timings and the [`LimiterKey`] every state is keyed by.
use super::key_state::{Bucket, KeyFacts};
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
    pub(super) const ALL: [Self; 5] = [
        Self::Search,
        Self::CodeSearch,
        Self::Graphql,
        Self::Write,
        Self::Auth,
    ];

    /// Groups whose start spacing follows the quota GitHub reports for
    /// their bucket rather than a fixed interval.
    pub(super) fn quota_paced(self) -> bool {
        matches!(self, Self::Search | Self::CodeSearch)
    }

    /// Whether `bucket` is the reported quota of a quota-paced group.
    pub(super) fn paces_bucket(bucket: &str) -> bool {
        Self::ALL
            .into_iter()
            .any(|group| group.quota_paced() && group.bucket() == bucket)
    }

    /// The quota a quota-paced bucket is assumed to hold until a response
    /// reports its real one, which replaces the assumption from the first
    /// answer on. GitHub documents 10 requests a minute for the code search
    /// endpoint, which requires authentication
    /// (<https://docs.github.com/en/rest/search/search#rate-limit>). It is an
    /// assumption for planning, never a cap: no request waits for it.
    pub(super) fn assumed_quota(self, now: u64) -> Option<Bucket> {
        match self {
            Self::CodeSearch => Some(Bucket {
                remaining: 10,
                reset: now / 1000 + 60,
            }),
            _ => None,
        }
    }

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
    /// Space search and code-search starts by the quota GitHub reports for
    /// their bucket (see [`Self::spacing_ms`]).
    pub quota_pacing: bool,
    pub graphql_spacing: Duration,
    pub write_spacing: Duration,
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
            quota_pacing: true,
            graphql_spacing: Duration::from_millis(1000),
            write_spacing: Duration::from_millis(1000),
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
            .saturating_add(super::millis(self.reset_grace))
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
            quota_pacing: false,
            graphql_spacing: Duration::ZERO,
            write_spacing: Duration::ZERO,
            reset_grace: Duration::ZERO,
            ..Self::octokit()
        }
    }

    /// Whether `group`'s starts are spaced at all under this config.
    pub(super) fn paces(&self, group: Group) -> bool {
        (self.quota_pacing && group.quota_paced()) || !self.fixed_spacing(group).is_zero()
    }

    /// Milliseconds between two of `group`'s starts for an admission planning
    /// `planned` requests. A quota-covered admission bursts. Only requests
    /// beyond the reported remaining quota are spread over the time until
    /// reset (plus grace). Before any answer reports the quota, or once its
    /// reset has passed, the group's assumed quota stands in
    /// ([`Group::assumed_quota`]); a group with none applies no spacing, and its
    /// single permit already holds the next request until an answer reports
    /// the quota.
    pub(super) fn spacing_ms(
        &self,
        group: Group,
        facts: &KeyFacts,
        now: u64,
        planned: usize,
    ) -> u64 {
        if self.quota_pacing && group.quota_paced() {
            return facts
                .quota(group, now)
                .map_or(0, |bucket| bucket.spacing_ms(self, now, planned));
        }
        super::millis(self.fixed_spacing(group))
    }

    fn fixed_spacing(&self, group: Group) -> Duration {
        match group {
            Group::Graphql => self.graphql_spacing,
            Group::Write => self.write_spacing,
            Group::Search | Group::CodeSearch | Group::Auth => Duration::ZERO,
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
