use octocode_contract_types::ErrorCode;
use octocode_io::budget::BudgetStop;
use serde::{Deserialize, Serialize};

/// Why a provider call failed. It reports, and serializes as, its contract
/// `errorCode` ([`Self::code`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(into = "ErrorCode", try_from = "ErrorCode")]
pub enum ProviderErrorKind {
    Authentication,
    Permission,
    NotFound,
    /// The request was rejected as malformed (`invalidInput`, the code every
    /// tool uses for rejected input).
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

impl ProviderErrorKind {
    /// Every kind.
    pub const ALL: [Self; 16] = [
        Self::Authentication,
        Self::Permission,
        Self::NotFound,
        Self::Validation,
        Self::RateLimited,
        Self::Transport,
        Self::Timeout,
        Self::Cancelled,
        Self::ResponseTooLarge,
        Self::RedirectDenied,
        Self::Decode,
        Self::Configuration,
        Self::CredentialStoreUnavailable,
        Self::Server,
        Self::Unavailable,
        Self::HttpStatus,
    ];

    /// The kind of a non-2xx HTTP answer: the one status table GitHub and
    /// the package registries share. A provider's own reading of a status
    /// (GitHub's typed missing ref, a registry 404 read as an empty answer)
    /// stays at its call site.
    #[must_use]
    pub const fn from_status(status: u16) -> Self {
        match status {
            401 => Self::Authentication,
            403 => Self::Permission,
            404 | 410 => Self::NotFound,
            400 | 422 => Self::Validation,
            429 => Self::RateLimited,
            451 => Self::Unavailable,
            500..=599 => Self::Server,
            _ => Self::HttpStatus,
        }
    }

    /// The declared `errorCode` this failure reports.
    #[must_use]
    pub const fn code(self) -> ErrorCode {
        match self {
            Self::Authentication => ErrorCode::Authentication,
            Self::Permission => ErrorCode::Permission,
            Self::NotFound => ErrorCode::NotFound,
            Self::Validation => ErrorCode::InvalidInput,
            Self::RateLimited => ErrorCode::RateLimited,
            Self::Transport => ErrorCode::Transport,
            Self::Timeout => ErrorCode::Timeout,
            Self::Cancelled => ErrorCode::Cancelled,
            Self::ResponseTooLarge => ErrorCode::ResponseTooLarge,
            Self::RedirectDenied => ErrorCode::RedirectDenied,
            Self::Decode => ErrorCode::Decode,
            Self::Configuration => ErrorCode::Configuration,
            Self::CredentialStoreUnavailable => ErrorCode::CredentialStoreUnavailable,
            Self::Server => ErrorCode::Server,
            Self::Unavailable => ErrorCode::Unavailable,
            Self::HttpStatus => ErrorCode::HttpStatus,
        }
    }
}

impl From<ProviderErrorKind> for ErrorCode {
    fn from(kind: ProviderErrorKind) -> Self {
        kind.code()
    }
}

impl TryFrom<ErrorCode> for ProviderErrorKind {
    type Error = String;

    fn try_from(code: ErrorCode) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.code() == code)
            .ok_or_else(|| format!("{code} is not a provider error code"))
    }
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
                Self::new(ProviderErrorKind::Cancelled, "GitHub request cancelled")
            }
            BudgetStop::Deadline => Self::new(
                ProviderErrorKind::Timeout,
                "GitHub request deadline exceeded",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each kind's wire spelling is its contract `errorCode`, and it reads
    /// back as the same kind.
    #[test]
    fn every_kind_serializes_as_its_error_code() {
        let expected = [
            "authentication",
            "permission",
            "notFound",
            "invalidInput",
            "rateLimited",
            "transport",
            "timeout",
            "cancelled",
            "responseTooLarge",
            "redirectDenied",
            "decode",
            "configuration",
            "credentialStoreUnavailable",
            "server",
            "unavailable",
            "httpStatus",
        ];
        for (kind, spelling) in ProviderErrorKind::ALL.into_iter().zip(expected) {
            let wire = serde_json::to_value(kind).expect("serialize");
            assert_eq!(wire, spelling, "{kind:?}");
            assert_eq!(wire, serde_json::to_value(kind.code()).expect("code"));
            assert_eq!(
                serde_json::from_value::<ProviderErrorKind>(wire).expect("deserialize"),
                kind
            );
        }
        assert!(serde_json::from_value::<ProviderErrorKind>("pathNotFound".into()).is_err());
    }

    #[test]
    fn statuses_map_to_one_kind_each() {
        for (status, kind) in [
            (401, ProviderErrorKind::Authentication),
            (403, ProviderErrorKind::Permission),
            (404, ProviderErrorKind::NotFound),
            (410, ProviderErrorKind::NotFound),
            (400, ProviderErrorKind::Validation),
            (422, ProviderErrorKind::Validation),
            (429, ProviderErrorKind::RateLimited),
            (451, ProviderErrorKind::Unavailable),
            (500, ProviderErrorKind::Server),
            (503, ProviderErrorKind::Server),
            (599, ProviderErrorKind::Server),
            (405, ProviderErrorKind::HttpStatus),
            (408, ProviderErrorKind::HttpStatus),
            (304, ProviderErrorKind::HttpStatus),
        ] {
            assert_eq!(
                ProviderErrorKind::from_status(status),
                kind,
                "HTTP {status}"
            );
        }
    }
}
