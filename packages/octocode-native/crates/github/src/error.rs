use octocode_io::budget::BudgetStop;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderErrorKind {
    Authentication,
    Permission,
    NotFound,
    /// The request was rejected as malformed (`invalidInput`, the code every
    /// tool uses for rejected input).
    #[serde(rename = "invalidInput")]
    Validation,
    RateLimited,
    Transport,
    Timeout,
    Cancelled,
    ResponseTooLarge,
    RedirectDenied,
    Decode,
    Configuration,
    CredentialStoreUnavailable,
    Server,
    /// HTTP 451: the resource is blocked for legal reasons; retrying cannot help.
    Unavailable,
    /// Any other unmapped non-2xx status; the status code is in the message.
    HttpStatus,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RateLimit {
    pub remaining: Option<u64>,
    pub reset_epoch_seconds: Option<u64>,
    pub retry_after_seconds: Option<u64>,
    /// GitHub `x-ratelimit-resource` (core, search, code_search, graphql).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<Box<str>>,
}

/// Typed discriminator for provider failures whose recovery differs; the
/// runtime keys its hints and continuations on this, never on message text.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderErrorReason {
    /// An issue number resolved to a pull request.
    IssueIsPullRequest,
    /// A file read named a directory.
    PathIsDirectory,
    /// A tree listing named a file.
    PathIsFile,
    /// The requested search page is past GitHub's 1,000-result window.
    SearchWindowExceeded,
    /// The repository itself did not resolve (missing, private, or not
    /// visible to the token), as opposed to a path or ref inside it.
    RepositoryNotFound,
    /// A search qualifier (`author:`, `assignee:`, …) names a user GitHub
    /// cannot find or the token cannot see; the repository scope is fine.
    UserNotFound,
    /// An explicit branch, tag, or SHA does not exist in the repository.
    RefNotFound,
    /// A pull-request number names an issue.
    PullRequestIsIssue,
    /// A file read named binary content, which is never returned as text.
    BinaryFile,
    /// A file read named a submodule: [`crate::GitHubProvider::submodule`]
    /// reads where it points.
    Submodule,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderError {
    pub kind: ProviderErrorKind,
    pub message: Box<str>,
    pub status: Option<u16>,
    pub request_id: Option<Box<str>>,
    pub documentation_url: Option<Box<str>>,
    pub rate_limit: Option<Box<RateLimit>>,
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<ProviderErrorReason>,
    /// `x-github-sso` authorization URL of a SAML-protected organization: the
    /// token must be authorized there before the request can succeed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sso_url: Option<Box<str>>,
}

impl ProviderError {
    pub fn new(kind: ProviderErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into().into_boxed_str(),
            status: None,
            request_id: None,
            documentation_url: None,
            rate_limit: None,
            // Timeouts, dropped connections, and 5xx are transient by nature;
            // an identical retry of a 60 s timeout succeeded in 2 s in evals.
            retryable: matches!(
                kind,
                ProviderErrorKind::Timeout
                    | ProviderErrorKind::Transport
                    | ProviderErrorKind::Server
            ),
            reason: None,
            sso_url: None,
        }
    }

    pub fn with_reason(mut self, reason: ProviderErrorReason) -> Self {
        self.reason = Some(reason);
        self
    }

    /// The call was cancelled or ran out of time: a caller stops here and
    /// returns the error instead of degrading or skipping the step.
    #[must_use]
    pub fn is_cancelled_or_timed_out(&self) -> bool {
        matches!(
            self.kind,
            ProviderErrorKind::Cancelled | ProviderErrorKind::Timeout
        )
    }
}
impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for ProviderError {}

impl From<BudgetStop> for ProviderError {
    fn from(stop: BudgetStop) -> Self {
        match stop {
            BudgetStop::Cancelled => {
                ProviderError::new(ProviderErrorKind::Cancelled, "GitHub request cancelled")
            }
            BudgetStop::Deadline => ProviderError::new(
                ProviderErrorKind::Timeout,
                "GitHub request deadline exceeded",
            ),
        }
    }
}
