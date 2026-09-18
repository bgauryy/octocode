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
            retryable: false,
        }
    }
}
impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for ProviderError {}
