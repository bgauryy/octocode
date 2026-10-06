//! Classification provider HTTP transport, deadline, cancellation, and error
//! handling, plus the provider question and answer shape: the one place the
//! public `type`/`ask`/`labels`/`known` question maps onto the provider's
//! `type`/`instructions`/`criteria`, and its answers map back to public
//! verdicts. The vendor (`providers::classification`) frames the request body.
use crate::providers::classification::gate::{GateDenied, GateLease, GatePermit};
use crate::providers::{BudgetStop, RequestBudget};
use bytes::BytesMut;
use futures_util::StreamExt;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Map, Value, json};
use std::future::Future;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant, SystemTime};
use url::Url;

const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
/// Error bodies are kept (parsed, never rendered) up to this size so callers
/// can map provider-specific error details such as `detail.error_type`.
const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Floor for every retry delay, so `Retry-After: 0` cannot hot-loop.
const MIN_RETRY_DELAY: Duration = Duration::from_millis(100);
const BACKOFF_BASE: Duration = Duration::from_millis(500);
const BACKOFF_CAP: Duration = Duration::from_secs(8);
/// Bound untrusted provider delay fields before converting from floating point.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(24 * 60 * 60);
/// Runtimes whose clients stay cached; older entries are evicted.
const MAX_CACHED_CLIENTS: usize = 8;

fn is_loopback_endpoint(endpoint: &Url) -> bool {
    match endpoint.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    }
}

/// One pooled client per Tokio runtime, shared by every classification
/// request on that runtime (all threads, all tool calls).
///
/// A single process-global `reqwest::Client` is unsafe here: hyper drives
/// each pooled connection from a task spawned on the runtime that opened it,
/// so once that runtime shuts down (every `#[tokio::test]`, or a closed
/// N-API `NativeRuntime`) the pooled connections are dead and reuse fails
/// with "dispatch task is gone". Keying by [`tokio::runtime::Id`] keeps
/// pooling across threads and calls within a runtime (a thread-local client
/// did not: clasify fans out on fresh scoped threads, so every call got a
/// cold pool) while never sharing connections across runtimes. The cache is
/// small and LRU-evicted, so short-lived runtimes cannot grow it unbounded.
fn shared_client(endpoint: &Url) -> Result<reqwest::Client, ClassificationError> {
    static CLIENTS: OnceLock<Mutex<Vec<(tokio::runtime::Id, bool, reqwest::Client)>>> =
        OnceLock::new();
    // A loopback provider is local to this machine. Avoid macOS system-proxy
    // discovery for it; remote providers still honor the user's proxy setup.
    let bypass_proxy = is_loopback_endpoint(endpoint);
    let build = || {
        let builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .user_agent(concat!("octocode-native/", env!("CARGO_PKG_VERSION")));
        let builder = if bypass_proxy {
            builder.no_proxy()
        } else {
            builder
        };
        builder.build().map_err(|_| transport_error())
    };
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return build();
    };
    let id = runtime.id();
    let mut clients = CLIENTS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(index) = clients
        .iter()
        .position(|(cached, bypass, _)| *cached == id && *bypass == bypass_proxy)
    {
        let entry = clients.remove(index);
        let client = entry.2.clone();
        clients.push(entry);
        return Ok(client);
    }
    let client = build()?;
    if clients.len() >= MAX_CACHED_CLIENTS {
        clients.remove(0);
    }
    clients.push((id, bypass_proxy, client.clone()));
    Ok(client)
}

/// Error returned by the classification transport layer. `code` is a stable
/// machine-readable identifier; `message` is human-readable prose; `hints`
/// are actionable suggestions shown to the caller. `retry_after` and
/// `provider_body` are internal metadata and are never rendered.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ClassificationError {
    pub code: String,
    pub message: String,
    pub hints: Vec<String>,
    /// Provider-requested (or gate-imposed) delay before a retry can succeed.
    pub retry_after: Option<Duration>,
    /// Parsed JSON body of a non-retried provider error response (e.g. HTTP
    /// 400 `{"detail":{"error_type":"max_tokens_exceeded"}}`), kept so
    /// callers can map vendor error details to distinct codes.
    /// Boxed to keep the error small in `Result`s.
    pub provider_body: Option<Box<Value>>,
    /// Failure kind of the delegated read behind a context error, so a call
    /// whose every read failed alike reports it like the read tool would.
    pub failure: Option<crate::tools::result::FailureKind>,
    /// HTTP requests actually attempted, including retries; never rendered as an error field.
    pub provider_calls: u64,
}

impl ClassificationError {
    pub(crate) fn new(code: &str, message: impl Into<String>, hint: &str) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            hints: vec![hint.to_owned()],
            ..Self::default()
        }
    }
}

/// The contract question template of a research `type`.
fn research_template(kind: &str) -> Option<&'static str> {
    let key = match kind {
        "sufficient" => "sufficient",
        "relevant" => "contribution",
        "supports" => "supportsClaim",
        "adds" => "addsEvidence",
        _ => return None,
    };
    crate::contracts::tool_contract(crate::tools::id::ToolId::Clasify)
        .ok()?
        .get("questionTemplates")?["questions"][key]
        .as_str()
}

/// The provider question for one public judge or research question
/// (`type` + `ask`, with `labels` or `known`). `locate` questions are built
/// per page by [`locate_questions`].
pub(crate) fn provider_question(question: &Value) -> Result<Value, ClassificationError> {
    let kind = question["type"].as_str().unwrap_or_default();
    let ask = question.get("ask").cloned().unwrap_or(Value::Null);
    let wire = match kind {
        "yesno" => "noul",
        "choice" => "choice",
        "score" => "score",
        _ => {
            let prompt = research_template(kind).ok_or_else(|| {
                ClassificationError::new(
                    "invalidClassificationRequest",
                    format!("Unknown question type {kind:?}."),
                    "Inspect the current clasify query schema.",
                )
            })?;
            let mut instructions = json!({"question":prompt,"target":ask});
            if let Some(known) = question.get("known") {
                instructions["knownEvidence"] = known.clone();
            }
            return Ok(json!({"type":"noul","instructions":instructions}));
        }
    };
    let mut provider = json!({"type":wire,"instructions":ask});
    if let Some(labels) = question.get("labels").filter(|labels| !labels.is_null()) {
        provider["criteria"] = labels.clone();
    }
    Ok(provider)
}

/// The two provider questions one `locate` asks of a page's passages: which
/// passage answers `target`, and whether any does.
pub(crate) fn locate_questions<'a>(
    target: &str,
    passages: impl Iterator<Item = &'a str>,
) -> [Value; 2] {
    let mut criteria = Map::new();
    for passage in passages {
        criteria.insert(passage.to_owned(), Value::Null);
    }
    if criteria.len() == 1 {
        criteria.insert(
            "NONE".into(),
            json!("No passage in the supplied source answers the target."),
        );
    }
    [
        json!({
            "type":"choice",
            "instructions":{
                "question":"Which passage ID best answers the target?",
                "target":target
            },
            "criteria":criteria
        }),
        json!({
            "type":"noul",
            "instructions":{
                "question":"Does any passage directly address or answer the target?",
                "target":target
            },
            "criteria":{
                "true":"At least one passage states or directly implies an answer.",
                "false":"No passage addresses the target."
            }
        }),
    ]
}

/// Attach the caller's search goal to one provider question. Public questions
/// and continuations keep the original text on the query, not inside each question.
pub(crate) fn with_goal(mut question: Value, goal: &str) -> Value {
    let goal = goal.trim();
    if goal.is_empty() {
        return question;
    }
    let Some(instructions) = question.get_mut("instructions") else {
        return question;
    };
    if let Some(map) = instructions.as_object_mut() {
        map.entry("goal")
            .or_insert_with(|| Value::String(goal.to_owned()));
    } else {
        let prior = instructions.take();
        *instructions = json!({"question": prior, "goal": goal});
    }
    question
}

/// One provider answer as a public verdict (`yesno`, `choice`, `score`) with
/// the models and usage that produced it.
pub(crate) fn project(
    question: &Value,
    answer: &Value,
    requested_model: &str,
    resolved_model: &str,
    usage: &Value,
) -> Result<Value, ClassificationError> {
    let answer = match question["type"].as_str() {
        Some("noul") => json!({"type":"yesno","yesno":answer["noul"]}),
        Some("choice") => {
            json!({"type":"choice","choice":answer["choice"],"confidence":answer["confidence"],"probabilities":answer["probabilities"]})
        }
        Some("score") => {
            let levels = question["criteria"].as_array().ok_or_else(|| {
                ClassificationError::new(
                    "invalidClassificationRequest",
                    "Score labels must be an array.",
                    "Inspect the current clasify query schema.",
                )
            })?;
            let legend: Map<String, Value> = levels
                .iter()
                .enumerate()
                .map(|(index, level)| (index.to_string(), level.clone()))
                .collect();
            json!({"type":"score","score":answer["score"],"confidence":answer["confidence"],"probabilities":answer["probabilities"],"legend":legend})
        }
        _ => {
            return Err(ClassificationError::new(
                "invalidClassificationRequest",
                "Invalid question type.",
                "Inspect the current clasify query schema.",
            ));
        }
    };
    Ok(
        json!({"requestedModel":requested_model,"resolvedModel":resolved_model,"answer":answer,"usage":{
            "input_tokens":usage["input_tokens"],"output_tokens":usage["output_tokens"]
        }}),
    )
}

pub(crate) fn endpoint(base_url: &str, path: &str) -> Result<Url, ClassificationError> {
    let base = Url::parse(base_url).map_err(|_| {
        ClassificationError::new(
            "invalidClassificationConfiguration",
            "OCTOCODE_CLASSIFICATION_API_HOST is not a valid URL.",
            "Use an HTTPS API root without a path, query, fragment, or credentials.",
        )
    })?;
    let host = base.host_str().unwrap_or_default();
    // `host_str` brackets IPv6 (`[::1]`); match the parsed host instead.
    let loopback = match base.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    };
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
            "invalidClassificationConfiguration",
            "OCTOCODE_CLASSIFICATION_API_HOST must be an HTTPS API root; HTTP is allowed only on loopback.",
            "Remove paths, credentials, query strings, and fragments from the configured API root.",
        ));
    }
    base.join(path).map_err(|_| {
        ClassificationError::new(
            "invalidClassificationConfiguration",
            "Cannot construct the classification provider endpoint.",
            "Check OCTOCODE_CLASSIFICATION_API_HOST.",
        )
    })
}

/// A configured key with control characters cannot be sent as a header.
pub(super) fn check_key(key: &SecretString) -> Result<(), ClassificationError> {
    if key.expose_secret().chars().any(char::is_control) {
        return Err(ClassificationError::new(
            "invalidClassificationConfiguration",
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
            "cancelled",
            "Classification request was cancelled.",
            "Retry only if this evaluation is still needed.",
        ),
        BudgetStop::Deadline => ClassificationError::new(
            "timeout",
            "Classification request exceeded its total deadline.",
            "Retry later or increase the shared request timeout.",
        ),
    }
}

pub(super) fn check_budget(budget: &RequestBudget) -> Result<(), ClassificationError> {
    budget.check().map_err(budget_error)
}

async fn wait<T>(
    budget: &RequestBudget,
    future: impl Future<Output = T>,
) -> Result<T, ClassificationError> {
    budget.wait(future).await.map_err(budget_error)
}

/// Parse an RFC 9110 IMF-fixdate (`Sun, 06 Nov 1994 08:49:37 GMT`).
fn parse_http_date(value: &str) -> Option<SystemTime> {
    let (_, rest) = value.trim().split_once(", ")?;
    let mut parts = rest.split_ascii_whitespace();
    let day = parts.next()?.parse::<i64>().ok()?;
    let month = match parts.next()? {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let year = parts.next()?.parse::<i64>().ok()?;
    let mut clock = parts.next()?.split(':').map(str::parse::<i64>);
    let (hour, minute, second) = (
        clock.next()?.ok()?,
        clock.next()?.ok()?,
        clock.next()?.ok()?,
    );
    if parts.next()? != "GMT"
        || parts.next().is_some()
        || !(1..=31).contains(&day)
        || !(0..24).contains(&hour)
        || !(0..60).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return None;
    }
    let seconds = crate::civil_date::days_from_civil(year, month, day) * 86_400
        + hour * 3600
        + minute * 60
        + second;
    let seconds = u64::try_from(seconds).ok()?;
    SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(seconds))
}

/// Provider-requested retry delay: `retry-after-ms`, then `Retry-After` as
/// delta-seconds or an HTTP-date (relative to `now`).
fn retry_after(headers: &reqwest::header::HeaderMap, now: SystemTime) -> Option<Duration> {
    if let Some(delay) = octocode_github::retry_after_delay(headers, MAX_RETRY_AFTER) {
        return Some(delay);
    }
    let date = headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()
        .and_then(parse_http_date)?;
    Some(
        date.duration_since(now)
            .unwrap_or(Duration::ZERO)
            .min(MAX_RETRY_AFTER),
    )
}

/// Full-jitter exponential backoff, floored at [`MIN_RETRY_DELAY`].
fn backoff(attempt: u32) -> Duration {
    octocode_github::full_jitter(BACKOFF_BASE, attempt, BACKOFF_CAP).max(MIN_RETRY_DELAY)
}

/// Delay before the next attempt: the provider's request (floored so `0`
/// cannot hot-loop), else jittered backoff.
fn retry_delay(provider: Option<Duration>, attempt: u32) -> Duration {
    provider.map_or_else(|| backoff(attempt), |delay| delay.max(MIN_RETRY_DELAY))
}

fn transport_error() -> ClassificationError {
    ClassificationError::new(
        "classificationProviderError",
        "Classification provider HTTPS request failed or timed out.",
        "Check connectivity, certificates, proxy settings, and the configured API root.",
    )
}

fn rate_limited(
    status: Option<u16>,
    retry_after: Option<Duration>,
    exceeds_deadline: bool,
) -> ClassificationError {
    let source = status.map_or_else(
        || "a shared provider cooldown is active".to_owned(),
        |status| format!("the provider returned HTTP {status}"),
    );
    let seconds = retry_after.map(|delay| delay.as_secs_f64().ceil().max(1.0));
    let message = match (seconds, exceeds_deadline) {
        (Some(seconds), true) => format!(
            "Classification provider is rate limiting ({source}); the retry delay of about {seconds:.0}s exceeds the remaining deadline."
        ),
        (Some(seconds), false) => format!(
            "Classification provider is rate limiting ({source}); retry after about {seconds:.0}s."
        ),
        (None, _) => format!("Classification provider is rate limiting ({source})."),
    };
    let hint = seconds.map_or_else(
        || "Retry later, or reduce request volume (OCTOCODE_CLASSIFICATION_CONCURRENCY).".to_owned(),
        |seconds| {
            format!(
                "Retry after about {seconds:.0}s, or reduce request volume (OCTOCODE_CLASSIFICATION_CONCURRENCY)."
            )
        },
    );
    ClassificationError {
        retry_after,
        ..ClassificationError::new("classificationRateLimited", message, &hint)
    }
}

fn denied(reason: GateDenied) -> ClassificationError {
    match reason {
        GateDenied::Cancelled => ClassificationError::new(
            "cancelled",
            "Classification request was cancelled.",
            "Retry only if this evaluation is still needed.",
        ),
        GateDenied::Deadline => ClassificationError::new(
            "timeout",
            "Classification request exceeded its total deadline while waiting for provider capacity.",
            "Retry later, reduce concurrent clasify calls, or increase the shared request timeout.",
        ),
        GateDenied::RateLimited { retry_after } => rate_limited(None, Some(retry_after), true),
        GateDenied::QuotaExhausted => quota_exhausted(),
        GateDenied::CircuitOpen { retry_after } => ClassificationError {
            retry_after: Some(retry_after),
            ..ClassificationError::new(
                "classificationProviderUnavailable",
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
        "classificationQuotaExhausted",
        "Classification provider billing or quota is exhausted (HTTP 402); retrying will not help.",
        "Add credit or quota to the OCTOCODE_CLASSIFICATION_API account, then rerun.",
    )
}

fn status_error(
    status: reqwest::StatusCode,
    provider_body: Option<Box<Value>>,
) -> ClassificationError {
    if status == reqwest::StatusCode::PAYMENT_REQUIRED {
        return ClassificationError {
            provider_body,
            ..quota_exhausted()
        };
    }
    let hint = match status.as_u16() {
        401 | 403 => "Check OCTOCODE_CLASSIFICATION_API and account access.",
        400 | 422 => "Check the supplied state, questions, configured model, and request size.",
        300..=399 => "Redirects are disabled; check OCTOCODE_CLASSIFICATION_API_HOST.",
        _ => "Check provider availability and OCTOCODE_CLASSIFICATION_API_HOST.",
    };
    let base = if error_type(provider_body.as_deref()) == Some("max_tokens_exceeded") {
        ClassificationError::new(
            "classificationStateTooLarge",
            "One page exceeded the classification provider's context window.",
            "Lower maxChars, read a narrower ranges window, or shorten the questions.",
        )
    } else {
        ClassificationError::new(
            "classificationProviderError",
            format!("Classification provider returned HTTP {status}; response body omitted."),
            hint,
        )
    };
    ClassificationError {
        provider_body,
        ..base
    }
}

fn error_type(body: Option<&Value>) -> Option<&str> {
    body?.pointer("/detail/error_type")?.as_str()
}

/// Throttle responses: shrink the shared window and honour `Retry-After`.
fn is_throttle(status: u16) -> bool {
    matches!(status, 429 | 503 | 529)
}

/// Server/transient failures that are safe to retry and count toward the
/// circuit breaker.
fn is_transient_failure(status: u16) -> bool {
    matches!(status, 408 | 500 | 502 | 504)
}

/// Vendor overload signal carried in the body (`detail.error_type ==
/// "system_overloaded"`), retried like 503 whatever the status.
fn is_overloaded(body: Option<&Value>) -> bool {
    error_type(body) == Some("system_overloaded")
}

fn is_retryable_transport(error: &reqwest::Error) -> bool {
    error.is_connect() || error.is_timeout() || error.is_request()
}

/// Read a bounded error body and keep it only when it parses as JSON.
async fn error_body(response: reqwest::Response, budget: &RequestBudget) -> Option<Box<Value>> {
    let mut body = BytesMut::new();
    let mut stream = response.bytes_stream();
    while let Ok(Some(Ok(chunk))) = wait(budget, stream.next()).await {
        if body.len().saturating_add(chunk.len()) > MAX_ERROR_BODY_BYTES {
            return None;
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).ok().map(Box::new)
}

enum Attempt {
    Done(Result<Value, ClassificationError>),
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
    request: &Value,
    key: &SecretString,
    endpoint: &Url,
    budget: &RequestBudget,
    gate: &GateLease,
    attempt: u32,
    provider_calls: &mut u64,
) -> Attempt {
    let permit = match gate.acquire(budget).await {
        Ok(permit) => permit,
        Err(reason) => return Attempt::Done(Err(denied(reason))),
    };
    if let Err(error) = check_budget(budget) {
        return Attempt::Done(Err(error));
    }
    let remaining = budget.deadline.saturating_duration_since(Instant::now());
    *provider_calls = provider_calls.saturating_add(1);
    let sent = wait(
        budget,
        client
            .post(endpoint.clone())
            .timeout(remaining)
            .bearer_auth(key.expose_secret())
            .header(reqwest::header::ACCEPT, "application/json")
            .json(request)
            .send(),
    )
    .await;
    let response = match sent {
        Err(error) => return Attempt::Done(Err(error)),
        Ok(Err(error)) => {
            permit.failed();
            return if is_retryable_transport(&error) {
                Attempt::Retry {
                    delay: backoff(attempt),
                    error: transport_error(),
                    throttle: None,
                }
            } else {
                Attempt::Done(Err(transport_error()))
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

/// A non-success response: a firewall block, a throttle or transient
/// failure to retry, or a final error (402 also stops later calls).
async fn failed_status(
    response: reqwest::Response,
    permit: GatePermit,
    budget: &RequestBudget,
    attempt: u32,
) -> Attempt {
    let status = response.status();
    let code = status.as_u16();
    // An HTML 403 is the edge firewall rejecting the content, not the API.
    let firewall_block = code == 403
        && response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("text/html"));
    if firewall_block {
        drop(permit);
        return Attempt::Done(Err(content_blocked()));
    }
    let requested = retry_after(response.headers(), SystemTime::now());
    let body = error_body(response, budget).await;
    if is_throttle(code) || is_overloaded(body.as_deref()) {
        permit.throttled(requested);
        return Attempt::Retry {
            delay: retry_delay(requested, attempt),
            error: ClassificationError {
                provider_body: body,
                ..rate_limited(Some(code), requested, false)
            },
            throttle: Some(code),
        };
    }
    if is_transient_failure(code) {
        permit.failed();
        return Attempt::Retry {
            delay: retry_delay(requested, attempt),
            error: ClassificationError {
                retry_after: requested,
                ..status_error(status, body)
            },
            throttle: None,
        };
    }
    // Not a provider-health signal (4xx/3xx, e.g. 400
    // `max_tokens_exceeded`): release neutrally, never retry, and keep the
    // parsed body for vendor error mapping.
    if status == reqwest::StatusCode::PAYMENT_REQUIRED {
        permit.quota_exhausted();
    } else {
        drop(permit);
    }
    Attempt::Done(Err(status_error(status, body)))
}

/// A success response's JSON body, within the body budget.
async fn read_body(
    response: reqwest::Response,
    permit: GatePermit,
    budget: &RequestBudget,
    attempt: u32,
) -> Attempt {
    let mut body = BytesMut::new();
    let mut stream = response.bytes_stream();
    loop {
        let chunk = match wait(budget, stream.next()).await {
            Err(error) => return Attempt::Done(Err(error)),
            Ok(None) => break,
            Ok(Some(Err(_))) => {
                permit.failed();
                return Attempt::Retry {
                    delay: backoff(attempt),
                    error: transport_error(),
                    throttle: None,
                };
            }
            Ok(Some(Ok(chunk))) => chunk,
        };
        if body.len().saturating_add(chunk.len()) > budget.max_body_bytes {
            drop(permit);
            return Attempt::Done(Err(ClassificationError::new(
                "invalidClassificationResponse",
                "Classification provider response exceeded the 4 MiB limit.",
                "Reduce the supplied state or question count.",
            )));
        }
        body.extend_from_slice(&chunk);
    }
    permit.success();
    Attempt::Done(serde_json::from_slice(&body).map_err(|_| {
        ClassificationError::new(
            "invalidClassificationResponse",
            "Classification provider returned invalid JSON.",
            "Inspect provider compatibility before using the response.",
        )
    }))
}

const CONTENT_BLOCKED: &str = "classificationContentBlocked";

fn content_blocked() -> ClassificationError {
    ClassificationError::new(
        CONTENT_BLOCKED,
        "The provider's firewall blocked this page's content (not an authentication failure).",
        "Judge a different line window, or read the region directly; the key is fine.",
    )
}

/// POST the supplied request unchanged through the process-wide provider gate.
/// Retry transient HTTP/transport failures within the deadline, preserving the
/// payload. Content rejection is terminal: rewriting evidence or instructions
/// would classify a different request without the caller's knowledge.
pub(crate) async fn post(
    request: &Value,
    key: &SecretString,
    endpoint: Url,
    budget: &RequestBudget,
    retries: u32,
    gate: &GateLease,
) -> Result<(Value, u64), ClassificationError> {
    let client = shared_client(&endpoint)?;
    let mut attempt_index = 0;
    let mut provider_calls = 0;
    let result = async {
        loop {
            check_budget(budget)?;
            match attempt(
                &client,
                request,
                key,
                &endpoint,
                budget,
                gate,
                attempt_index,
                &mut provider_calls,
            )
            .await
            {
                Attempt::Done(result) => return result,
                Attempt::Retry {
                    delay,
                    error,
                    throttle,
                } => {
                    if attempt_index >= retries {
                        return Err(error);
                    }
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
    RequestBudget {
        deadline,
        cancellation,
        max_body_bytes: MAX_BODY_BYTES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::classification::gate;
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
    use serde_json::json;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn loopback_proxy_policy_handles_domain_ipv4_and_ipv6() {
        for endpoint in [
            "http://localhost:1234/v1/systemone",
            "http://LOCALHOST:1234/v1/systemone",
            "http://127.0.0.2:1234/v1/systemone",
            "http://[::1]:1234/v1/systemone",
        ] {
            assert!(
                is_loopback_endpoint(&Url::parse(endpoint).unwrap()),
                "{endpoint}"
            );
        }
        for endpoint in [
            "https://api.jev.ai/v1/systemone",
            "http://192.0.2.1:1234/v1/systemone",
            "http://[2001:db8::1]:1234/v1/systemone",
        ] {
            assert!(
                !is_loopback_endpoint(&Url::parse(endpoint).unwrap()),
                "{endpoint}"
            );
        }
    }
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

    async fn send(
        server: &str,
        timeout: Duration,
        retries: u32,
        lease: &GateLease,
    ) -> Result<Value, ClassificationError> {
        post(
            &json!({}),
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
    async fn firewall_block_preserves_request_and_is_not_retried() {
        let server = MockServer::start().await;
        let request = json!({
            "state": {"content": "path is /etc/passwd and ../x"},
            "questions": {"q": {"instructions": "Does this read /etc/passwd?"}}
        });
        Mock::given(method("POST"))
            .and(wiremock::matchers::body_json(request.clone()))
            .respond_with(
                ResponseTemplate::new(403)
                    .set_body_raw("<!DOCTYPE html><html>blocked</html>", "text/html"),
            )
            .expect(1)
            .mount(&server)
            .await;
        let error = post(
            &request,
            &SecretString::from("test-key".to_owned()),
            endpoint(&server.uri(), "v1/systemone").unwrap(),
            &test_budget(Duration::from_secs(10)),
            3,
            &fresh_gate(4),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "classificationContentBlocked");
        assert!(!error.hints[0].contains("OCTOCODE_CLASSIFICATION_API"));
        let received = server.received_requests().await.unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].body_json::<Value>().unwrap(), request);
    }

    #[tokio::test]
    async fn transport_rejects_redirects_and_oversized_responses() {
        for (response, expected) in [
            (
                ResponseTemplate::new(302).insert_header("location", "https://example.com"),
                "classificationProviderError",
            ),
            (
                ResponseTemplate::new(200).set_body_string("x".repeat(65)),
                "invalidClassificationResponse",
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(response)
                .expect(1)
                .mount(&server)
                .await;
            let mut budget = test_budget(Duration::from_secs(30));
            budget.max_body_bytes = 64;
            let error = post(
                &json!({}),
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
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        let error = send(&server.uri(), Duration::ZERO, 0, &fresh_gate(4))
            .await
            .unwrap_err();
        assert_eq!(error.code, "timeout");
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
            let server = MockServer::start().await;
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

    #[tokio::test]
    async fn bad_request_is_not_retried_and_keeps_the_provider_body() {
        let server = MockServer::start().await;
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
        assert_eq!(error.code, "classificationStateTooLarge");
        assert!(error.hints[0].contains("maxChars"));
        assert_eq!(
            error.provider_body.as_ref().unwrap()["detail"]["error_type"],
            "max_tokens_exceeded"
        );
    }

    #[tokio::test]
    async fn payment_required_is_remembered_for_every_call_until_the_memo_expires() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(402).set_body_json(json!({"detail":"no credit"})))
            .expect(2)
            .mount(&server)
            .await;
        let call = fresh_gate(4);
        let error = send(&server.uri(), Duration::from_secs(10), 3, &call)
            .await
            .unwrap_err();
        assert_eq!(error.code, "classificationQuotaExhausted");
        assert!(error.message.contains("quota"), "{}", error.message);
        assert_eq!(error.provider_calls, 1);
        // Later requests of the same call fail fast with the same error.
        let again = send(&server.uri(), Duration::from_secs(10), 3, &call)
            .await
            .unwrap_err();
        assert_eq!(
            (
                again.code.as_str(),
                again.message.as_str(),
                again.provider_calls
            ),
            (error.code.as_str(), error.message.as_str(), 0)
        );
        // Another call within the memo fails fast too: no request is sent.
        let other = send(&server.uri(), Duration::from_secs(10), 3, &call.fork())
            .await
            .unwrap_err();
        assert_eq!(
            (other.code.as_str(), other.provider_calls),
            (error.code.as_str(), 0)
        );
        assert!(call.quota_exhausted());
        // After the memo, one call probes again: credit may have been added.
        call.gate().expire_quota_memo();
        assert!(!call.quota_exhausted());
        let probe = send(&server.uri(), Duration::from_secs(10), 3, &call.fork())
            .await
            .unwrap_err();
        assert_eq!(
            (probe.code.as_str(), probe.provider_calls),
            (error.code.as_str(), 1)
        );
    }

    #[tokio::test]
    async fn system_overloaded_body_is_retried_and_shrinks_the_gate() {
        for status in [500, 400] {
            let server = MockServer::start().await;
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
        // Retries exhausted: the overload surfaces as rate limiting with the
        // provider body kept.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(503)
                    .set_body_json(json!({"detail":{"error_type":"system_overloaded"}})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let error = send(&server.uri(), Duration::from_secs(10), 0, &fresh_gate(8))
            .await
            .unwrap_err();
        assert_eq!(error.code, "classificationRateLimited");
        assert_eq!(
            error.provider_body.unwrap()["detail"]["error_type"],
            "system_overloaded"
        );
    }

    #[tokio::test]
    async fn retry_after_beyond_the_deadline_is_rate_limited_not_timeout() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "120"))
            .expect(1)
            .mount(&server)
            .await;
        let lease = fresh_gate(4);
        let error = send(&server.uri(), Duration::from_secs(5), 3, &lease)
            .await
            .unwrap_err();
        assert_eq!(error.code, "classificationRateLimited");
        assert_eq!(error.retry_after, Some(Duration::from_secs(120)));
        assert!(error.message.contains("exceeds the remaining deadline"));
        // The cooldown is shared: another call on the same endpoint fails fast
        // without reaching the provider (the mock expects exactly one request).
        let other = lease.fork();
        let error = send(&server.uri(), Duration::from_secs(5), 3, &other)
            .await
            .unwrap_err();
        assert_eq!(error.code, "classificationRateLimited");
        assert!(error.retry_after.unwrap() > Duration::from_secs(100));
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
                retry_after(&headers, SystemTime::now()),
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
            retry_after(&date, SystemTime::UNIX_EPOCH),
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
    fn retry_after_parses_milliseconds_seconds_and_http_dates() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(784_111_777); // Sun, 06 Nov 1994 08:49:37 GMT
        let mut ms = HeaderMap::new();
        ms.insert("retry-after-ms", HeaderValue::from_static("1500"));
        ms.insert(RETRY_AFTER, HeaderValue::from_static("9"));
        assert_eq!(retry_after(&ms, now), Some(Duration::from_millis(1500)));

        let mut secs = HeaderMap::new();
        secs.insert(RETRY_AFTER, HeaderValue::from_static("2"));
        assert_eq!(retry_after(&secs, now), Some(Duration::from_secs(2)));

        let mut date = HeaderMap::new();
        date.insert(
            RETRY_AFTER,
            HeaderValue::from_static("Sun, 06 Nov 1994 08:50:07 GMT"),
        );
        assert_eq!(retry_after(&date, now), Some(Duration::from_secs(30)));
        // A date in the past means "now".
        date.insert(
            RETRY_AFTER,
            HeaderValue::from_static("Sat, 05 Nov 1994 08:49:37 GMT"),
        );
        assert_eq!(retry_after(&date, now), Some(Duration::ZERO));

        for invalid in ["soon", "-1", "Sun, 06 Nov 1994 25:00:00 GMT", ""] {
            let mut headers = HeaderMap::new();
            headers.insert(RETRY_AFTER, HeaderValue::from_str(invalid).unwrap());
            assert_eq!(retry_after(&headers, now), None, "{invalid}");
        }
        assert_eq!(retry_after(&HeaderMap::new(), now), None);
        assert_eq!(
            parse_http_date("Thu, 01 Jan 1970 00:00:00 GMT"),
            Some(SystemTime::UNIX_EPOCH)
        );
        assert_eq!(
            parse_http_date("Tue, 29 Feb 2028 12:00:00 GMT"),
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_835_438_400))
        );
    }

    #[test]
    fn retry_delay_floors_zero_and_backoff_uses_full_jitter() {
        assert_eq!(retry_delay(Some(Duration::ZERO), 0), MIN_RETRY_DELAY);
        assert_eq!(
            retry_delay(Some(Duration::from_secs(2)), 5),
            Duration::from_secs(2)
        );
        let mut distinct = std::collections::HashSet::new();
        for attempt in 0..6 {
            let ceiling = BACKOFF_BASE.saturating_mul(1 << attempt).min(BACKOFF_CAP);
            for _ in 0..20 {
                let delay = retry_delay(None, attempt);
                assert!(delay >= MIN_RETRY_DELAY && delay <= ceiling.max(MIN_RETRY_DELAY));
                distinct.insert(delay);
            }
        }
        assert!(distinct.len() > 10, "backoff is not jittered");
        assert!(retry_delay(None, 30) <= BACKOFF_CAP);
    }
}
