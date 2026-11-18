//! Classification provider HTTP transport: endpoint, deadline, cancellation,
//! bounded retries and error mapping. The vendor
//! (`providers::classification`) frames and validates request bodies.
mod errors;
mod request;
use crate::providers::RuntimeClients;
use crate::providers::classification::gate::GateLease;
use errors::{
    BAD_REQUEST_HINT, budget_error, denied, is_overloaded, overloaded_error, rate_limited,
    send_error, stall_of, stalled, status_error, transport_error,
};
use octocode_contract_types::ErrorCode;
use octocode_io::budget::RequestBudget;
use request::{Request, request};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use std::future::Future;
use std::time::{Duration, Instant};
use url::Url;

const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
/// Error bodies are kept (parsed, never rendered) up to this size so callers
/// can map provider-specific error details such as `detail.error_type`.
const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Floor for every retry delay, so `Retry-After: 0` cannot hot-loop.
const MIN_RETRY_DELAY: Duration = Duration::from_millis(100);
/// Full-jitter exponential backoff, floored so `Retry-After: 0` cannot
/// hot-loop.
const BACKOFF: octocode_io::retry::Backoff = octocode_io::retry::Backoff {
    base: Duration::from_millis(500),
    cap: Duration::from_secs(8),
    floor: MIN_RETRY_DELAY,
};
/// Bound untrusted provider delay fields before converting from floating point.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(24 * 60 * 60);
/// Runtimes whose clients stay cached; older entries are evicted.
const MAX_CACHED_CLIENTS: usize = 8;

/// The pooled client for the current Tokio runtime, shared by every
/// classification request on it (all threads, all tool calls). A thread-local
/// client would not pool: clasify fans out on fresh scoped threads.
fn shared_client(endpoint: &Url) -> Result<reqwest::Client, ClassificationError> {
    static CLIENTS: RuntimeClients<bool> = RuntimeClients::new(MAX_CACHED_CLIENTS);
    // A loopback provider is local to this machine. Avoid macOS system-proxy
    // discovery for it; remote providers still honor the user's proxy setup.
    let bypass_proxy = octocode_github::is_loopback(endpoint);
    CLIENTS.get(bypass_proxy, || {
        octocode_io::http::client(&provider_policy(bypass_proxy)).map_err(|_| transport_error())
    })
}

/// The client policy of a classification provider. It sets no timeout of its
/// own: a request is ended by its budget ([`budget`]), whose stall bound
/// ends the request, never one attempt that a retry would repeat.
fn provider_policy(bypass_proxy: bool) -> octocode_io::http::ClientPolicy {
    octocode_io::http::ClientPolicy {
        connect_timeout: Some(CONNECT_TIMEOUT),
        user_agent: Some(concat!("octocode-native/", env!("CARGO_PKG_VERSION"))),
        proxy: if bypass_proxy {
            octocode_io::http::Proxy::Direct
        } else {
            octocode_io::http::Proxy::System
        },
        ..octocode_io::http::ClientPolicy::default()
    }
}

/// Error returned by the classification transport layer. `code` is a stable
/// machine-readable identifier; `message` is human-readable prose; `hints`
/// are actionable suggestions shown to the caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassificationError {
    pub code: ErrorCode,
    pub message: String,
    pub hints: Vec<String>,
    /// The failed read's own leads by name (`viewTree`, `broadenSearch`):
    /// executable `{tool, query}` calls, rendered beside `hints.text`.
    pub leads: Vec<(String, serde_json::Value)>,
    /// Failure kind of the delegated read behind a context error, so a call
    /// whose every read failed alike reports it like the read tool would.
    pub failure: Option<crate::response::result::ErrorClass>,
    /// HTTP requests actually attempted, including retries; never rendered as an error field.
    pub provider_calls: u64,
    /// Usage reported before failure; absent when no response reported usage.
    pub reported_usage: Option<Box<ReportedUsage>>,
    /// The open circuit refused the request: a consequence of earlier
    /// failures, reported only when no cause was seen.
    pub paused: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReportedUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub known_usage_calls: u64,
}

impl ClassificationError {
    pub fn usage(&self) -> ReportedUsage {
        self.reported_usage.as_deref().copied().unwrap_or_default()
    }

    pub fn new(code: ErrorCode, message: impl Into<String>, hint: &str) -> Self {
        Self::with_hints(code, message, vec![hint.to_owned()])
    }

    /// An error with its own recovery `hints` and nothing else recorded.
    pub fn with_hints(code: ErrorCode, message: impl Into<String>, hints: Vec<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hints,
            leads: Vec::new(),
            failure: None,
            provider_calls: 0,
            reported_usage: None,
            paused: false,
        }
    }

    /// A provider refusal of the request itself (HTTP 400 or 422) names the
    /// model the request asked for: the provider's body is never kept, and an
    /// unknown model is one of its causes. No local models list exists to
    /// refuse it before the call.
    pub fn naming_model(mut self, model: &str) -> Self {
        if self.code == ErrorCode::ClassificationProviderError
            && self.hints.first().map(String::as_str) == Some(BAD_REQUEST_HINT)
        {
            self.message
                .push_str(&format!(" The request named model `{model}`."));
        }
        self
    }

    /// The provider answered, but not in the shape the contract requires.
    pub fn invalid_response(message: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::InvalidClassificationResponse,
            message,
            "Inspect provider compatibility before using the answer.",
        )
    }
}

pub fn endpoint(base_url: &str, path: &str) -> Result<Url, ClassificationError> {
    let base = Url::parse(base_url).map_err(|_| {
        ClassificationError::new(
            ErrorCode::InvalidClassificationConfiguration,
            "OCTOCODE_CLASSIFICATION_API_HOST is not a valid URL.",
            "Use an HTTPS API root without a path, query, fragment, or credentials.",
        )
    })?;
    let host = base.host_str().unwrap_or_default();
    let loopback = octocode_github::is_loopback(&base);
    let valid_scheme = base.scheme() == "https" || (base.scheme() == "http" && loopback);
    if !valid_scheme
        || host.is_empty()
        || !base.username().is_empty()
        || base.password().is_some()
        || !matches!(base.path(), "" | "/")
        || base.query().is_some()
        || base.fragment().is_some()
    {
        return Err(ClassificationError::new(
            ErrorCode::InvalidClassificationConfiguration,
            "OCTOCODE_CLASSIFICATION_API_HOST must be an HTTPS API root; HTTP is allowed only on loopback.",
            "Remove paths, credentials, query strings, and fragments from the configured API root.",
        ));
    }
    base.join(path).map_err(|_| {
        ClassificationError::new(
            ErrorCode::InvalidClassificationConfiguration,
            "Cannot construct the classification provider endpoint.",
            "Check OCTOCODE_CLASSIFICATION_API_HOST.",
        )
    })
}

/// A configured key with control characters cannot be sent as a header.
pub fn check_key(key: &SecretString) -> Result<(), ClassificationError> {
    if key.expose_secret().chars().any(char::is_control) {
        return Err(ClassificationError::new(
            ErrorCode::InvalidClassificationConfiguration,
            "OCTOCODE_CLASSIFICATION_API contains invalid control characters.",
            "Replace the configured key.",
        ));
    }
    Ok(())
}

pub fn check_budget(budget: &RequestBudget) -> Result<(), ClassificationError> {
    budget.check().map_err(budget_error)
}

pub(crate) async fn wait<T>(
    budget: &RequestBudget,
    future: impl Future<Output = T>,
) -> Result<T, ClassificationError> {
    budget.wait(future).await.map_err(budget_error)
}

/// POST the serialized request `body` unchanged through the process-wide
/// provider gate. The caller measured these bytes against its size limit;
/// every retry of a transient HTTP/transport failure within the deadline
/// sends them again without re-serializing. Content rejection is terminal:
/// rewriting evidence or instructions would classify a different request
/// without the caller's knowledge.
pub async fn post(
    body: bytes::Bytes,
    headers: reqwest::header::HeaderMap,
    key: &SecretString,
    endpoint: Url,
    budget: &RequestBudget,
    retries: u32,
    gate: &GateLease,
) -> Result<(Value, u64), ClassificationError> {
    request(
        Request {
            body: Some(body),
            headers,
        },
        key,
        endpoint,
        budget,
        retries,
        gate,
    )
    .await
}

pub async fn get(
    key: &SecretString,
    endpoint: Url,
    budget: &RequestBudget,
    retries: u32,
    gate: &GateLease,
) -> Result<(Value, u64), ClassificationError> {
    request(
        Request {
            body: None,
            headers: reqwest::header::HeaderMap::new(),
        },
        key,
        endpoint,
        budget,
        retries,
        gate,
    )
    .await
}

/// The budget of one classification request: the call's `deadline`, which
/// may lie far away (a model's time follows its input), and a stall bound of
/// [`octocode_io::http::READ_STALL`]. A provider still sending is never cut;
/// one that sends nothing for the stall bound ends the request, retries
/// included: the stall bound is the most one request waits on silence.
pub fn budget(
    deadline: Instant,
    cancellation: octocode_io::cancellation::Cancellation,
) -> RequestBudget {
    RequestBudget::new(deadline, cancellation, MAX_BODY_BYTES)
        .with_stall(octocode_io::http::READ_STALL)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod endpoint_policy_tests;
