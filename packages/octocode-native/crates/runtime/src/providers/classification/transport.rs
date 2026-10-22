//! Classification provider HTTP transport: endpoint, deadline, cancellation,
//! bounded retries and error mapping. The vendor
//! (`providers::classification`) frames and validates request bodies.
use crate::providers::RuntimeClients;
use crate::providers::classification::CIRCUIT_OPEN;
use crate::providers::classification::gate::{GateDenied, GateLease, GatePermit};
use octocode_contract_types::ErrorCode;
use octocode_io::budget::{BudgetStop, RequestBudget};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use std::future::Future;
use std::time::{Duration, Instant, SystemTime};
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
        let policy = octocode_io::http::ClientPolicy {
            connect_timeout: Some(CONNECT_TIMEOUT),
            user_agent: Some(concat!("octocode-native/", env!("CARGO_PKG_VERSION"))),
            proxy: if bypass_proxy {
                octocode_io::http::Proxy::Direct
            } else {
                octocode_io::http::Proxy::System
            },
            ..octocode_io::http::ClientPolicy::default()
        };
        octocode_io::http::client(&policy).map_err(|_| transport_error())
    })
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
    pub failure: Option<crate::response::result::FailureKind>,
    /// HTTP requests actually attempted, including retries; never rendered as an error field.
    pub provider_calls: u64,
    /// Usage reported before failure; absent when no response reported usage.
    pub(crate) reported_usage: Option<Box<ReportedUsage>>,
    /// The open circuit refused the request: a consequence of earlier
    /// failures, reported only when no cause was seen.
    pub(crate) paused: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ReportedUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub known_usage_calls: u64,
}

impl ClassificationError {
    pub(crate) fn usage(&self) -> ReportedUsage {
        self.reported_usage.as_deref().copied().unwrap_or_default()
    }

    pub(crate) fn new(code: ErrorCode, message: impl Into<String>, hint: &str) -> Self {
        Self::with_hints(code, message, vec![hint.to_owned()])
    }

    /// An error with its own recovery `hints` and nothing else recorded.
    pub(crate) fn with_hints(
        code: ErrorCode,
        message: impl Into<String>,
        hints: Vec<String>,
    ) -> Self {
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

    /// The provider answered, but not in the shape the contract requires.
    pub(crate) fn invalid_response(message: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::InvalidClassificationResponse,
            message,
            "Inspect provider compatibility before using the answer.",
        )
    }
}

pub(crate) fn endpoint(base_url: &str, path: &str) -> Result<Url, ClassificationError> {
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
pub(crate) fn check_key(key: &SecretString) -> Result<(), ClassificationError> {
    if key.expose_secret().chars().any(char::is_control) {
        return Err(ClassificationError::new(
            ErrorCode::InvalidClassificationConfiguration,
            "OCTOCODE_CLASSIFICATION_API contains invalid control characters.",
            "Replace the configured key.",
        ));
    }
    Ok(())
}

/// The classification error for a request whose budget stopped it.
fn budget_error(stop: BudgetStop) -> ClassificationError {
    match stop {
        BudgetStop::Cancelled => ClassificationError::new(
            ErrorCode::Cancelled,
            "Classification request was cancelled.",
            crate::response::rows::CANCELLED_HINT,
        ),
        BudgetStop::Deadline => ClassificationError::new(
            ErrorCode::Timeout,
            "Classification request exceeded its total deadline.",
            "Retry unresolved evidence in a smaller request.",
        ),
    }
}

pub(crate) fn check_budget(budget: &RequestBudget) -> Result<(), ClassificationError> {
    budget.check().map_err(budget_error)
}

pub(crate) async fn wait<T>(
    budget: &RequestBudget,
    future: impl Future<Output = T>,
) -> Result<T, ClassificationError> {
    budget.wait(future).await.map_err(budget_error)
}

fn transport_error() -> ClassificationError {
    ClassificationError::new(
        CIRCUIT_OPEN,
        "Classification provider request failed or timed out.",
        "Check connectivity, certificates, proxy settings, and the configured API root.",
    )
}

/// A request that got no response, named by how it failed: no connection
/// to the configured host, a timeout, or another transport failure.
fn send_error(error: &reqwest::Error) -> ClassificationError {
    let what = if error.is_connect() {
        "could not connect to the configured API host"
    } else if error.is_timeout() {
        "timed out"
    } else {
        return transport_error();
    };
    ClassificationError::new(
        CIRCUIT_OPEN,
        format!("Classification provider request {what}."),
        "Check connectivity, certificates, proxy settings, and the configured API root.",
    )
}

/// A provider overloaded without naming a retry delay (503/529, or the
/// vendor's `system_overloaded`).
fn overloaded_error(status: u16) -> ClassificationError {
    ClassificationError::new(
        CIRCUIT_OPEN,
        format!(
            "Classification provider is overloaded or unavailable (HTTP {status}) and named no retry delay."
        ),
        "Check provider availability and OCTOCODE_CLASSIFICATION_API_HOST, then retry.",
    )
}

/// A throttle: a 429 is rate limiting, a 503/529 with a delay is an
/// overload; either states the delay it asked for.
fn rate_limited(
    status: Option<u16>,
    retry_after: Option<Duration>,
    exceeds_deadline: bool,
) -> ClassificationError {
    let (state, source) = match status {
        None => (
            "is rate limiting",
            "a shared provider cooldown is active".to_owned(),
        ),
        Some(429) => (
            "is rate limiting",
            "the provider returned HTTP 429".to_owned(),
        ),
        Some(status) => (
            "is overloaded",
            format!("the provider returned HTTP {status}"),
        ),
    };
    let seconds = retry_after.map(|delay| delay.as_secs_f64().ceil().max(1.0));
    let message = match (seconds, exceeds_deadline) {
        (Some(seconds), true) => format!(
            "Classification provider {state} ({source}); the retry delay of about {seconds:.0}s exceeds the remaining deadline."
        ),
        (Some(seconds), false) => {
            format!("Classification provider {state} ({source}); retry after about {seconds:.0}s.")
        }
        (None, _) => format!("Classification provider {state} ({source})."),
    };
    let hint = seconds.map_or_else(
        || "Retry later.".to_owned(),
        |seconds| format!("Retry after about {seconds:.0}s."),
    );
    ClassificationError::new(ErrorCode::ClassificationRateLimited, message, &hint)
}

fn denied(reason: GateDenied) -> ClassificationError {
    match reason {
        GateDenied::Cancelled => ClassificationError::new(
            ErrorCode::Cancelled,
            "Classification request was cancelled.",
            crate::response::rows::CANCELLED_HINT,
        ),
        GateDenied::Deadline => ClassificationError::new(
            ErrorCode::Timeout,
            "Classification request exceeded its total deadline while waiting for provider capacity.",
            "Retry later or reduce concurrent clasify calls.",
        ),
        GateDenied::RateLimited { retry_after } => rate_limited(None, Some(retry_after), true),
        GateDenied::QuotaExhausted => quota_exhausted(),
        GateDenied::CircuitOpen { retry_after } => ClassificationError {
            paused: true,
            ..ClassificationError::new(
                CIRCUIT_OPEN,
                format!(
                    "Classification provider failed repeatedly; requests are paused for about {:.0}s.",
                    retry_after.as_secs_f64().ceil().max(1.0)
                ),
                "Check provider availability and OCTOCODE_CLASSIFICATION_API_HOST, then retry.",
            )
        },
    }
}

pub(crate) fn quota_exhausted() -> ClassificationError {
    ClassificationError::new(
        ErrorCode::ClassificationQuotaExhausted,
        "Classification provider billing or quota is exhausted (HTTP 402); retrying will not help.",
        "Add credit or quota to the OCTOCODE_CLASSIFICATION_API account, then rerun.",
    )
}

/// Maps a non-success status to its error; the provider body only picks
/// the code (`max_tokens_exceeded`) and is never kept or rendered.
fn status_error(status: reqwest::StatusCode, provider_body: Option<&Value>) -> ClassificationError {
    if status == reqwest::StatusCode::PAYMENT_REQUIRED {
        return quota_exhausted();
    }
    let hint = match status.as_u16() {
        401 | 403 => "Check OCTOCODE_CLASSIFICATION_API and account access.",
        400 | 422 => {
            "Check state, questions and request size; list models with clasify {\"operation\":\"models\"} before naming one."
        }
        300..=399 => "Redirects are disabled; check OCTOCODE_CLASSIFICATION_API_HOST.",
        _ => "Check provider availability and OCTOCODE_CLASSIFICATION_API_HOST.",
    };
    if status.is_server_error() || status == reqwest::StatusCode::REQUEST_TIMEOUT {
        return ClassificationError::new(
            CIRCUIT_OPEN,
            format!("Classification provider returned HTTP {status}; response body omitted."),
            hint,
        );
    }
    if error_type(provider_body) == Some("max_tokens_exceeded") {
        ClassificationError::new(
            ErrorCode::ClassificationStateTooLarge,
            "The state exceeded the classification provider's context window.",
            "Narrow the state or the source ranges, or shorten the questions.",
        )
    } else {
        ClassificationError::new(
            ErrorCode::ClassificationProviderError,
            format!("Classification provider returned HTTP {status}; response body omitted."),
            hint,
        )
    }
}

fn error_type(body: Option<&Value>) -> Option<&str> {
    body?.pointer("/detail/error_type")?.as_str()
}

/// Vendor overload signal carried in the body (`detail.error_type ==
/// "system_overloaded"`), retried like 503 whatever the status.
fn is_overloaded(body: Option<&Value>) -> bool {
    error_type(body) == Some("system_overloaded")
}

/// Read a bounded error body and keep it only when it parses as JSON.
async fn error_body(response: reqwest::Response, budget: &RequestBudget) -> Option<Value> {
    let body = octocode_io::retry::read_bounded(response, budget, MAX_ERROR_BODY_BYTES)
        .await
        .ok()?;
    serde_json::from_slice(&body).ok()
}

enum Attempt {
    Done(Result<Value, ClassificationError>),
    /// The open circuit refused the request: a consequence of the failures
    /// before it, which the caller reports instead when it saw one.
    Refused(ClassificationError),
    /// Retry after the delay; `error` is returned if no attempt remains or
    /// the delay does not fit the deadline.
    Retry {
        delay: Duration,
        error: ClassificationError,
        /// Status of a throttle response, for the rate-limited error.
        throttle: Option<u16>,
    },
}

async fn attempt(
    client: &reqwest::Client,
    request: &Request,
    key: &SecretString,
    endpoint: &Url,
    budget: &RequestBudget,
    gate: &GateLease,
    attempt: u32,
    provider_calls: &mut u64,
) -> Attempt {
    let permit = match gate.acquire(budget).await {
        Ok(permit) => permit,
        Err(reason @ GateDenied::CircuitOpen { .. }) => return Attempt::Refused(denied(reason)),
        Err(reason) => return Attempt::Done(Err(denied(reason))),
    };
    if let Err(error) = check_budget(budget) {
        return Attempt::Done(Err(error));
    }
    *provider_calls = provider_calls.saturating_add(1);
    let builder = match &request.body {
        Some(body) => client
            .post(endpoint.clone())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.clone()),
        None => client.get(endpoint.clone()),
    };
    let sent = wait(
        budget,
        builder
            .bearer_auth(key.expose_secret())
            .header(reqwest::header::ACCEPT, "application/json")
            .headers(request.headers.clone())
            .send(),
    )
    .await;
    let response = match sent {
        Err(error) => return Attempt::Done(Err(error)),
        Ok(Err(error)) => {
            permit.failed();
            return if octocode_io::retry::is_retryable_send(&error) {
                Attempt::Retry {
                    delay: BACKOFF.next_delay(attempt),
                    error: send_error(&error),
                    throttle: None,
                }
            } else {
                Attempt::Done(Err(send_error(&error)))
            };
        }
        Ok(Ok(response)) => response,
    };
    if response.status().is_success() {
        read_body(response, permit, budget, attempt).await
    } else {
        failed_status(response, permit, budget, attempt).await
    }
}

/// A non-success response: a refusal, a throttle or transient
/// failure to retry, or a final error (402 also stops later calls).
async fn failed_status(
    response: reqwest::Response,
    permit: GatePermit,
    budget: &RequestBudget,
    attempt: u32,
) -> Attempt {
    let status = response.status();
    let code = status.as_u16();
    let requested = octocode_io::retry::retry_after_delay(
        response.headers(),
        MAX_RETRY_AFTER,
        SystemTime::now(),
    );
    let body = error_body(response, budget).await;
    let class = octocode_io::retry::classify_status(code);
    let overloaded =
        class == octocode_io::retry::StatusClass::Overloaded || is_overloaded(body.as_ref());
    // A 429, or an overload that names its retry delay, is a throttle: it
    // shrinks the shared window and every call honours the delay.
    if class == octocode_io::retry::StatusClass::RateLimited || (overloaded && requested.is_some())
    {
        permit.throttled(requested);
        return Attempt::Retry {
            delay: BACKOFF.after(requested, attempt),
            error: rate_limited(Some(code), requested, false),
            throttle: Some(code),
        };
    }
    // An overload without a delay says the provider is down, not that this
    // caller sends too much: it is retried and counts toward the circuit,
    // whose cooldown then pauses every chunk instead of each retrying it.
    if overloaded {
        permit.failed();
        return Attempt::Retry {
            delay: BACKOFF.after(None, attempt),
            error: overloaded_error(code),
            throttle: None,
        };
    }
    // Transient server failures are retried and count toward the circuit.
    if matches!(
        class,
        octocode_io::retry::StatusClass::Timeout | octocode_io::retry::StatusClass::Transient
    ) {
        permit.failed();
        return Attempt::Retry {
            delay: BACKOFF.after(requested, attempt),
            error: status_error(status, body.as_ref()),
            throttle: None,
        };
    }
    // Not a provider-health signal (4xx/3xx, e.g. 400
    // `max_tokens_exceeded`): release neutrally, never retry, and keep the
    // parsed body only to pick the vendor error code.
    if status == reqwest::StatusCode::PAYMENT_REQUIRED {
        permit.quota_exhausted();
    } else {
        drop(permit);
    }
    Attempt::Done(Err(status_error(status, body.as_ref())))
}

/// A success response's JSON body, within the body budget.
async fn read_body(
    response: reqwest::Response,
    permit: GatePermit,
    budget: &RequestBudget,
    attempt: u32,
) -> Attempt {
    let body = match octocode_io::retry::read_bounded(response, budget, budget.max_body_bytes).await
    {
        Ok(body) => body,
        Err(octocode_io::retry::BodyError::Stopped(stop)) => {
            return Attempt::Done(Err(budget_error(stop)));
        }
        Err(octocode_io::retry::BodyError::Transport) => {
            permit.failed();
            return Attempt::Retry {
                delay: BACKOFF.next_delay(attempt),
                error: transport_error(),
                throttle: None,
            };
        }
        Err(octocode_io::retry::BodyError::TooLarge) => {
            drop(permit);
            return Attempt::Done(Err(ClassificationError::new(
                ErrorCode::InvalidClassificationResponse,
                "Classification provider response exceeded the 4 MiB limit.",
                "Reduce the supplied state or question count.",
            )));
        }
    };
    permit.success();
    Attempt::Done(serde_json::from_slice(&body).map_err(|_| {
        ClassificationError::invalid_response("Classification provider returned invalid JSON.")
    }))
}

/// POST the serialized request `body` unchanged through the process-wide
/// provider gate. The caller measured these bytes against its size limit;
/// every retry of a transient HTTP/transport failure within the deadline
/// sends them again without re-serializing. Content rejection is terminal:
/// rewriting evidence or instructions would classify a different request
/// without the caller's knowledge.
pub(crate) async fn post(
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

pub(crate) async fn get(
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

struct Request {
    body: Option<bytes::Bytes>,
    headers: reqwest::header::HeaderMap,
}

async fn request(
    request: Request,
    key: &SecretString,
    endpoint: Url,
    budget: &RequestBudget,
    retries: u32,
    gate: &GateLease,
) -> Result<(Value, u64), ClassificationError> {
    let client = shared_client(&endpoint)?;
    let mut attempt_index = 0;
    let mut provider_calls = 0;
    // A retry the circuit refuses reports the failure that preceded it: the
    // cause, not the pause it led to.
    let mut last_error: Option<ClassificationError> = None;
    let result = async {
        loop {
            check_budget(budget)?;
            match attempt(
                &client,
                &request,
                key,
                &endpoint,
                budget,
                gate,
                attempt_index,
                &mut provider_calls,
            )
            .await
            {
                Attempt::Refused(error) => {
                    return Err(last_error.take().unwrap_or(error));
                }
                Attempt::Done(result) => return result,
                Attempt::Retry {
                    delay,
                    error,
                    throttle,
                } => {
                    if attempt_index >= retries {
                        return Err(error);
                    }
                    last_error = Some(error.clone());
                    if Instant::now() + delay >= budget.deadline {
                        return Err(match throttle {
                            Some(status) => rate_limited(Some(status), Some(delay), true),
                            None => error,
                        });
                    }
                    wait(budget, tokio::time::sleep(delay)).await?;
                    attempt_index += 1;
                }
            }
        }
    }
    .await;
    result
        .map(|body| (body, provider_calls))
        .map_err(|mut error| {
            error.provider_calls = provider_calls;
            error
        })
}

pub fn budget(
    deadline: Instant,
    cancellation: tokio_util::sync::CancellationToken,
) -> RequestBudget {
    RequestBudget::new(deadline, cancellation, MAX_BODY_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::classification::gate;
    use serde_json::json;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

    fn test_budget(timeout: Duration) -> RequestBudget {
        budget(
            Instant::now() + timeout,
            tokio_util::sync::CancellationToken::new(),
        )
    }

    /// A fresh gate per test so shared cooldowns never leak across tests.
    fn fresh_gate(limit: usize) -> GateLease {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        gate::lease(
            &format!("test://transport-{}", NEXT.fetch_add(1, Ordering::Relaxed)),
            limit,
        )
    }

    const OK_BODY: &str = r#"{"model":"m","answers":{},"usage":{}}"#;

    fn body(request: &Value) -> bytes::Bytes {
        serde_json::to_vec(request).unwrap().into()
    }

    async fn send(
        server: &str,
        timeout: Duration,
        retries: u32,
        lease: &GateLease,
    ) -> Result<Value, ClassificationError> {
        post(
            body(&json!({})),
            reqwest::header::HeaderMap::new(),
            &SecretString::from("test-key".to_owned()),
            endpoint(server, "v1/systemone").unwrap(),
            &test_budget(timeout),
            retries,
            lease,
        )
        .await
        .map(|(body, _)| body)
    }

    #[tokio::test]
    async fn html_forbidden_preserves_request_without_claiming_the_cause() {
        let server = MockServer::builder().start().await;
        let request = json!({
            "state": {"content": "path is /etc/passwd and ../x"},
            "questions": {"q": {"instructions": "Does this read /etc/passwd?"}}
        });
        Mock::given(method("POST"))
            .and(wiremock::matchers::body_json(request.clone()))
            .respond_with(ResponseTemplate::new(403).set_body_raw(
                "<!DOCTYPE html><html>Access denied: invalid credentials</html>",
                "text/html",
            ))
            .expect(1)
            .mount(&server)
            .await;
        let error = post(
            body(&request),
            reqwest::header::HeaderMap::new(),
            &SecretString::from("test-key".to_owned()),
            endpoint(&server.uri(), "v1/systemone").unwrap(),
            &test_budget(Duration::from_secs(10)),
            3,
            &fresh_gate(4),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::ClassificationProviderError);
        assert!(error.message.contains("403"));
        assert!(error.hints[0].contains("OCTOCODE_CLASSIFICATION_API"));
        assert!(!error.message.contains("firewall"));
        assert!(!error.hints.iter().any(|hint| hint.contains("key is fine")));
        let received = server.received_requests().await.unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].body_json::<Value>().unwrap(), request);
    }

    #[tokio::test]
    async fn transport_rejects_redirects_and_oversized_responses() {
        for (response, expected) in [
            (
                ResponseTemplate::new(302).insert_header("location", "https://example.com"),
                ErrorCode::ClassificationProviderError,
            ),
            (
                ResponseTemplate::new(200).set_body_string("x".repeat(65)),
                ErrorCode::InvalidClassificationResponse,
            ),
        ] {
            let server = MockServer::builder().start().await;
            Mock::given(method("POST"))
                .respond_with(response)
                .expect(1)
                .mount(&server)
                .await;
            let mut budget = test_budget(Duration::from_secs(30));
            budget.max_body_bytes = 64;
            let error = post(
                body(&json!({})),
                reqwest::header::HeaderMap::new(),
                &SecretString::from("test-key".to_owned()),
                endpoint(&server.uri(), "v1/systemone").unwrap(),
                &budget,
                0,
                &fresh_gate(4),
            )
            .await
            .unwrap_err();
            assert_eq!(error.code, expected);
        }
    }

    #[tokio::test]
    async fn expired_deadline_prevents_transport() {
        let server = MockServer::builder().start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        let error = send(&server.uri(), Duration::ZERO, 0, &fresh_gate(4))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Timeout);
    }

    /// One successful provider answer, expected exactly once.
    async fn mount_ok_once(server: &MockServer) {
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(OK_BODY))
            .expect(1)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn transient_statuses_are_retried_then_succeed() {
        for status in [408, 500, 502, 504, 503, 529, 429] {
            let server = MockServer::builder().start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(status).insert_header("retry-after-ms", "1"))
                .up_to_n_times(1)
                .expect(1)
                .mount(&server)
                .await;
            mount_ok_once(&server).await;
            let started = Instant::now();
            let value = send(&server.uri(), Duration::from_secs(10), 1, &fresh_gate(4))
                .await
                .unwrap_or_else(|error| panic!("HTTP {status} was not retried: {error:?}"));
            assert_eq!(value["model"], "m");
            assert!(
                started.elapsed() >= MIN_RETRY_DELAY,
                "retry-after-ms: 1 must be floored"
            );
        }
    }

    /// The retry budget bounds the attempts exactly: `retries` retries after
    /// the first attempt, then the throttle is reported. Replaces the
    /// classification-retry harness cases that varied MAX_RETRIES, now a
    /// runtime constant (`RuntimeLimits::max_retries`).
    #[tokio::test]
    async fn the_retry_budget_bounds_attempts_exactly() {
        for retries in [0, 1, 3] {
            let server = MockServer::builder().start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(429).insert_header("retry-after-ms", "1"))
                .expect(u64::from(retries) + 1)
                .mount(&server)
                .await;
            let error = send(
                &server.uri(),
                Duration::from_secs(10),
                retries,
                &fresh_gate(4),
            )
            .await
            .unwrap_err();
            assert_eq!(
                error.code,
                ErrorCode::ClassificationRateLimited,
                "{retries}: {error:?}"
            );
            assert_eq!(error.provider_calls, u64::from(retries) + 1, "{retries}");
            server.verify().await;
        }
    }

    #[test]
    fn generic_request_errors_offer_discovery_without_inventing_a_capability_cause() {
        for code in [400, 422] {
            let error = status_error(
                reqwest::StatusCode::from_u16(code).unwrap(),
                Some(
                    &json!({"detail":{"error_type":"api_usage_error","message":"private provider detail"}}),
                ),
            );
            assert_eq!(error.code, ErrorCode::ClassificationProviderError);
            assert!(error.message.contains(&code.to_string()));
            assert!(!error.message.contains("private provider detail"));
            assert!(error.hints[0].contains(r#"clasify {"operation":"models"}"#));
            assert!(error.hints[0].contains("state"));
            assert!(error.hints[0].contains("request size"));
            assert!(!error.hints[0].contains("jev-preview"));
            assert!(!error.message.contains("unsupported"));
        }
    }

    #[tokio::test]
    async fn bad_request_is_not_retried_and_maps_the_provider_error_type() {
        let server = MockServer::builder().start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(json!({"detail":{"error_type":"max_tokens_exceeded"}})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let error = send(&server.uri(), Duration::from_secs(10), 3, &fresh_gate(4))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::ClassificationStateTooLarge);
        assert!(error.hints[0].contains("source ranges"), "{error:?}");
    }

    #[tokio::test]
    async fn payment_required_is_remembered_for_every_call_until_the_memo_expires() {
        let server = MockServer::builder().start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(402).set_body_json(json!({"detail":"no credit"})))
            .expect(2)
            .mount(&server)
            .await;
        let call = fresh_gate(4);
        let error = send(&server.uri(), Duration::from_secs(10), 3, &call)
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::ClassificationQuotaExhausted);
        assert!(error.message.contains("quota"), "{}", error.message);
        assert_eq!(error.provider_calls, 1);
        // Later requests of the same call fail fast with the same error.
        let again = send(&server.uri(), Duration::from_secs(10), 3, &call)
            .await
            .unwrap_err();
        assert_eq!(
            (again.code, again.message.as_str(), again.provider_calls),
            (error.code, error.message.as_str(), 0)
        );
        // Another call within the memo fails fast too: no request is sent.
        let other = send(&server.uri(), Duration::from_secs(10), 3, &call.fork())
            .await
            .unwrap_err();
        assert_eq!((other.code, other.provider_calls), (error.code, 0));
        assert!(call.quota_exhausted());
        // After the memo, one call probes again: credit may have been added.
        call.gate().expire_quota_memo();
        assert!(!call.quota_exhausted());
        let probe = send(&server.uri(), Duration::from_secs(10), 3, &call.fork())
            .await
            .unwrap_err();
        assert_eq!((probe.code, probe.provider_calls), (error.code, 1));
    }

    #[tokio::test]
    async fn system_overloaded_body_is_retried_and_shrinks_the_gate() {
        for status in [500, 400] {
            let server = MockServer::builder().start().await;
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(status)
                        .insert_header("retry-after-ms", "1")
                        .set_body_json(json!({"detail":{"error_type":"system_overloaded"}})),
                )
                .up_to_n_times(1)
                .expect(1)
                .mount(&server)
                .await;
            mount_ok_once(&server).await;
            let lease = fresh_gate(8);
            let value = send(&server.uri(), Duration::from_secs(10), 1, &lease)
                .await
                .unwrap_or_else(|error| panic!("HTTP {status} overload not retried: {error:?}"));
            assert_eq!(value["model"], "m");
            assert_eq!(
                lease.gate().effective_limit(),
                4,
                "overload is an AIMD signal"
            );
        }
        // Retries exhausted: an overload that names its delay is a throttle.
        let server = MockServer::builder().start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(503)
                    .insert_header("retry-after-ms", "1")
                    .set_body_json(json!({"detail":{"error_type":"system_overloaded"}})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let error = send(&server.uri(), Duration::from_secs(10), 0, &fresh_gate(8))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::ClassificationRateLimited);
        assert!(
            error
                .message
                .contains("is overloaded (the provider returned HTTP 503)"),
            "{}",
            error.message
        );
    }

    /// A 503/529 without a retry delay is the provider being down, not a
    /// throttle: it is never called rate limiting, and it counts toward the
    /// circuit, so after the threshold every further chunk fails fast
    /// without reaching the provider (11 chunks sent 44 requests before).
    #[tokio::test]
    async fn an_overload_without_a_delay_opens_the_circuit_instead_of_throttling() {
        for status in [503, 529] {
            let server = MockServer::builder().start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;
            let lease = fresh_gate(8);
            let mut codes = Vec::new();
            for _ in 0..5 {
                let error = send(&server.uri(), Duration::from_secs(30), 1, &lease.fork())
                    .await
                    .unwrap_err();
                assert!(
                    !error.message.contains("rate limiting"),
                    "{status}: {}",
                    error.message
                );
                codes.push((error.code, error.message));
            }
            // Every call reports the provider unavailable: the ones that
            // reached it with its failure, the refused ones as paused.
            assert!(
                codes.iter().all(|(code, _)| *code == CIRCUIT_OPEN),
                "{codes:?}"
            );
            assert!(
                codes[..3]
                    .iter()
                    .all(|(_, message)| message.contains(&format!("HTTP {status}"))),
                "{codes:?}"
            );
            assert!(
                codes[3..]
                    .iter()
                    .all(|(_, message)| message.contains("paused")),
                "{codes:?}"
            );
            let received = server.received_requests().await.unwrap_or_default().len();
            assert_eq!(
                received,
                gate::CIRCUIT_THRESHOLD as usize,
                "{status}: the open circuit stops the requests"
            );
            assert_eq!(lease.gate().effective_limit(), 8, "no throttle signal");
        }
    }

    #[tokio::test]
    async fn retry_after_beyond_the_deadline_is_rate_limited_not_timeout() {
        let server = MockServer::builder().start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "120"))
            .expect(1)
            .mount(&server)
            .await;
        let lease = fresh_gate(4);
        let error = send(&server.uri(), Duration::from_secs(5), 3, &lease)
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::ClassificationRateLimited);
        assert!(
            error
                .message
                .contains("retry delay of about 120s exceeds the remaining deadline"),
            "{}",
            error.message
        );
        // The cooldown is shared: another call on the same endpoint fails fast
        // without reaching the provider (the mock expects exactly one request).
        let other = lease.fork();
        let error = send(&server.uri(), Duration::from_secs(5), 3, &other)
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::ClassificationRateLimited);
        let seconds: u64 = error
            .message
            .split("about ")
            .nth(1)
            .and_then(|rest| rest.split('s').next())
            .and_then(|n| n.parse().ok())
            .expect("the shared cooldown names its delay");
        assert!(seconds > 100, "{}", error.message);
    }

    #[test]
    fn huge_retry_after_headers_are_bounded_before_duration_conversion() {
        for name in ["retry-after-ms", "retry-after"] {
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                reqwest::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                reqwest::header::HeaderValue::from_static("1e20"),
            );
            assert_eq!(
                octocode_io::retry::retry_after_delay(&headers, MAX_RETRY_AFTER, SystemTime::now()),
                Some(MAX_RETRY_AFTER)
            );
        }
        // A far-future HTTP-date is bounded by the same cap.
        let mut date = reqwest::header::HeaderMap::new();
        date.insert(
            reqwest::header::RETRY_AFTER,
            reqwest::header::HeaderValue::from_static("Fri, 31 Dec 9999 23:59:59 GMT"),
        );
        assert_eq!(
            octocode_io::retry::retry_after_delay(&date, MAX_RETRY_AFTER, SystemTime::UNIX_EPOCH),
            Some(MAX_RETRY_AFTER)
        );
    }

    /// Minimal HTTP/1.1 server: one request per connection. `script` decides,
    /// per accepted connection index, to reset (`None`) or answer after a delay.
    async fn raw_server(
        script: impl Fn(usize) -> Option<Duration> + Send + Sync + 'static,
        in_flight: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
    ) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let script = Arc::new(script);
        tokio::spawn(async move {
            let mut index = 0;
            while let Ok((mut socket, _)) = listener.accept().await {
                let Some(delay) = script(index) else {
                    index += 1;
                    drop(socket);
                    continue;
                };
                index += 1;
                let (in_flight, peak) = (in_flight.clone(), peak.clone());
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut buffer = [0_u8; 4096];
                    loop {
                        let Ok(read) = socket.read(&mut buffer).await else {
                            return;
                        };
                        if read == 0 {
                            return;
                        }
                        request.extend_from_slice(&buffer[..read]);
                        let text = String::from_utf8_lossy(&request);
                        if let Some(head_end) = text.find("\r\n\r\n") {
                            let length = text[..head_end]
                                .lines()
                                .find_map(|line| {
                                    line.to_ascii_lowercase()
                                        .strip_prefix("content-length:")
                                        .and_then(|value| value.trim().parse::<usize>().ok())
                                })
                                .unwrap_or(0);
                            if request.len() >= head_end + 4 + length {
                                break;
                            }
                        }
                    }
                    let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(delay).await;
                    in_flight.fetch_sub(1, Ordering::SeqCst);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{OK_BODY}",
                        OK_BODY.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn connection_reset_is_retried() {
        let counters = (Arc::default(), Arc::default());
        let server = raw_server(
            |index| (index > 0).then_some(Duration::ZERO),
            counters.0,
            counters.1,
        )
        .await;
        let value = send(&server, Duration::from_secs(10), 2, &fresh_gate(4))
            .await
            .unwrap();
        assert_eq!(value["model"], "m");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn gate_bounds_in_flight_requests_across_concurrent_calls() {
        let in_flight = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let server = raw_server(
            |_| Some(Duration::from_millis(40)),
            in_flight.clone(),
            peak.clone(),
        )
        .await;
        let first = fresh_gate(3);
        let calls = [first.clone(), first.fork(), first.fork()];
        let mut tasks = Vec::new();
        for call in calls {
            for _ in 0..6 {
                let (server, call) = (server.clone(), call.clone());
                tasks.push(tokio::spawn(async move {
                    send(&server, Duration::from_secs(20), 0, &call).await
                }));
            }
        }
        for task in tasks {
            task.await.unwrap().unwrap();
        }
        assert_eq!(peak.load(Ordering::SeqCst), 3);
        assert_eq!(in_flight.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn endpoint_accepts_an_https_root_and_appends_the_vendor_path() {
        let url = endpoint("https://api.typesafe.ai", "v1/systemone").expect("valid root");
        assert_eq!(url.as_str(), "https://api.typesafe.ai/v1/systemone");
        // A trailing slash is also a bare root.
        assert!(endpoint("https://api.typesafe.ai/", "v1/systemone").is_ok());
        // HTTP is allowed only on loopback.
        assert!(endpoint("http://127.0.0.1", "v1/systemone").is_ok());
        assert!(endpoint("http://[::1]:8080", "v1/systemone").is_ok());
        assert!(endpoint("http://LOCALHOST", "v1/systemone").is_ok());
    }

    #[test]
    fn endpoint_rejects_non_root_or_insecure_bases() {
        for bad in [
            "http://api.typesafe.ai",            // http, non-loopback
            "http://[2001:db8::1]",              // http, non-loopback IPv6
            "https://api.typesafe.ai/v1",        // has a path
            "https://api.typesafe.ai/?x=1",      // has a query
            "https://api.typesafe.ai/#frag",     // has a fragment
            "https://user:pass@api.typesafe.ai", // has credentials
            "not a url",
        ] {
            assert!(
                endpoint(bad, "v1/systemone").is_err(),
                "should reject {bad}"
            );
        }
    }

    #[test]
    fn retry_delay_floors_zero_and_backoff_uses_full_jitter() {
        assert_eq!(BACKOFF.after(Some(Duration::ZERO), 0), MIN_RETRY_DELAY);
        assert_eq!(
            BACKOFF.after(Some(Duration::from_secs(2)), 5),
            Duration::from_secs(2)
        );
        let mut distinct = std::collections::HashSet::new();
        for attempt in 0..6 {
            let ceiling = BACKOFF.base.saturating_mul(1 << attempt).min(BACKOFF.cap);
            for _ in 0..20 {
                let delay = BACKOFF.after(None, attempt);
                assert!(delay >= MIN_RETRY_DELAY && delay <= ceiling.max(MIN_RETRY_DELAY));
                distinct.insert(delay);
            }
        }
        assert!(distinct.len() > 10, "backoff is not jittered");
        assert!(BACKOFF.after(None, 30) <= BACKOFF.cap);
    }
}

#[cfg(test)]
mod endpoint_policy_tests {
    #[test]
    fn judgment_endpoint_uses_the_shared_loopback_rule() {
        for host in [
            "localhost",
            "localhost.",
            "provider.localhost",
            "127.42.0.1",
            "[::1]",
            "example.com",
        ] {
            let base = format!("http://{host}:1234/");
            let url = url::Url::parse(&base).unwrap();
            assert_eq!(
                super::endpoint(&base, "v1/test").is_ok(),
                octocode_github::is_loopback(&url),
                "{host}"
            );
        }
    }
}
