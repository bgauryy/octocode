use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderErrorKind {
    Authentication,
    Permission,
    NotFound,
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderError {
    pub kind: ProviderErrorKind,
    pub message: Box<str>,
    pub status: Option<u16>,
    pub request_id: Option<Box<str>>,
    pub documentation_url: Option<Box<str>>,
    pub rate_limit: Option<RateLimit>,
    pub retryable: bool,
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
        }
    }
}
impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for ProviderError {}
