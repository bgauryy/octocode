use super::{ClassificationError, host_variable, key_variable};
use crate::providers::classification::CIRCUIT_OPEN;
use crate::providers::classification::gate::GateDenied;
use octocode_contract_types::ErrorCode;
use octocode_io::budget::{BudgetStop, RequestBudget};
use octocode_io::retry::{StatusClass, classify_status};
use serde_json::Value;
use std::time::Duration;

/// The classification error for a request whose budget stopped it.
pub(super) fn budget_error(stop: BudgetStop) -> ClassificationError {
    match stop {
        BudgetStop::Cancelled => ClassificationError::new(
            ErrorCode::Cancelled,
            "Classification request was cancelled.",
            crate::response::rows::CANCELLED_HINT,
        ),
        BudgetStop::Deadline => ClassificationError::new(
            ErrorCode::Timeout,
            "The classification provider is slow: it did not answer within the request time.",
            "Retry later; the evidence stays readable without classification.",
        ),
    }
}

/// The stall bound that stopped a request, when its budget stopped it at the
/// stall bound rather than at the call's deadline or by cancellation.
pub(super) fn stall_of(budget: &RequestBudget, stop: BudgetStop) -> Option<Duration> {
    budget
        .stall()
        .filter(|_| stop == BudgetStop::Deadline && budget.check().is_ok())
}

/// A provider that took the request and then sent nothing for `stall`. It is
/// not retried: another attempt would wait on the same silent provider again.
pub(super) fn stalled(stall: Duration) -> ClassificationError {
    ClassificationError::new(
        CIRCUIT_OPEN,
        format!(
            "Classification provider accepted the request, then sent nothing for {}s; the request ended there, without retrying.",
            stall.as_secs_f64().ceil()
        ),
        &format!(
            "Retry later; the evidence stays readable without classification. If it repeats, check {} and the provider's status.",
            host_variable()
        ),
    )
}

/// What a provider that could not answer asks of the caller.
fn availability_hint() -> String {
    format!(
        "Check provider availability and {}, then retry.",
        host_variable()
    )
}

pub(super) fn transport_error() -> ClassificationError {
    ClassificationError::new(
        CIRCUIT_OPEN,
        "Classification provider request failed or timed out.",
        "Check connectivity, certificates, proxy settings, and the configured API root.",
    )
}

/// A request that got no response, named by how it failed: no connection
/// to the configured host, a timeout, or another transport failure.
pub(super) fn send_error(error: &reqwest::Error) -> ClassificationError {
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
pub(super) fn overloaded_error(status: u16) -> ClassificationError {
    ClassificationError::new(
        CIRCUIT_OPEN,
        format!(
            "Classification provider is overloaded or unavailable (HTTP {status}) and named no retry delay."
        ),
        &availability_hint(),
    )
}

/// A throttle: a 429 is rate limiting, a 503/529 with a delay is an
/// overload; either states the delay it asked for.
pub(super) fn rate_limited(
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
        || BUSY_HINT.to_owned(),
        |seconds| format!("Retry after about {seconds:.0}s, or reduce concurrent clasify calls."),
    );
    ClassificationError::new(ErrorCode::ClassificationRateLimited, message, &hint)
}

/// The provider is out of capacity and named no delay.
const BUSY_HINT: &str = "Retry later or reduce concurrent clasify calls.";

pub(super) fn denied(reason: GateDenied) -> ClassificationError {
    match reason {
        GateDenied::Cancelled => ClassificationError::new(
            ErrorCode::Cancelled,
            "Classification request was cancelled.",
            crate::response::rows::CANCELLED_HINT,
        ),
        GateDenied::Deadline => ClassificationError::new(
            ErrorCode::Timeout,
            "Classification request exceeded its total deadline while waiting for provider capacity.",
            BUSY_HINT,
        ),
        GateDenied::RateLimited { retry_after } => rate_limited(None, Some(retry_after), true),
        GateDenied::QuotaExhausted => quota_exhausted(),
        GateDenied::CredentialRejected => credential_rejected(),
        GateDenied::CircuitOpen { retry_after } => ClassificationError {
            paused: true,
            ..ClassificationError::new(
                CIRCUIT_OPEN,
                format!(
                    "Classification provider failed repeatedly; requests are paused for about {:.0}s.",
                    retry_after.as_secs_f64().ceil().max(1.0)
                ),
                &availability_hint(),
            )
        },
    }
}

pub(crate) fn quota_exhausted() -> ClassificationError {
    ClassificationError::new(
        ErrorCode::ClassificationQuotaExhausted,
        "Classification provider billing or quota is exhausted (HTTP 402); retrying will not help.",
        // A rerun sooner is refused the same way (`gate::QUOTA_MEMO`).
        &format!(
            "Add credit or quota to the {} account, then rerun in a minute: this process repeats the refusal until then.",
            key_variable()
        ),
    )
}

/// The provider refused the account's credential (HTTP 401).
pub(super) fn credential_rejected() -> ClassificationError {
    ClassificationError::new(
        ErrorCode::ClassificationProviderError,
        "Classification provider returned HTTP 401; response body omitted.",
        // A rerun sooner is refused the same way (`gate::AUTH_MEMO`).
        &format!(
            "Check {} and account access, then rerun in a minute: this process repeats the refusal until then.",
            key_variable()
        ),
    )
}

/// What a 400 or 422 asks of the caller; [`ClassificationError::naming_model`]
/// recognises the error it names the request's model on by this hint.
pub(super) const BAD_REQUEST_HINT: &str = "Check state, questions and request size; list models with clasify {\"operation\":\"models\"} before naming one.";

/// Maps a non-success status to its error; the provider body only picks
/// the code (`max_tokens_exceeded`) and is never kept or rendered.
pub(super) fn status_error(
    status: reqwest::StatusCode,
    provider_body: Option<&Value>,
) -> ClassificationError {
    if status == reqwest::StatusCode::PAYMENT_REQUIRED {
        return quota_exhausted();
    }
    let hint = match status.as_u16() {
        401 | 403 => format!("Check {} and account access.", key_variable()),
        400 | 422 => BAD_REQUEST_HINT.to_owned(),
        300..=399 => format!("Redirects are disabled; check {}.", host_variable()),
        _ => format!("Check provider availability and {}.", host_variable()),
    };
    let class = classify_status(status.as_u16());
    if class.is_server() || class == StatusClass::Timeout {
        return ClassificationError::new(
            CIRCUIT_OPEN,
            format!("Classification provider returned HTTP {status}; response body omitted."),
            &hint,
        );
    }
    if error_type(provider_body) == Some("max_tokens_exceeded") {
        ClassificationError::new(
            ErrorCode::ClassificationStateTooLarge,
            "The state exceeded the classification provider's context window; the provider does not document whether a refused request is billed.",
            "Split it: narrower ranges, one row per part, or operation:\"sections\" with the source under resources.",
        )
    } else {
        ClassificationError::new(
            ErrorCode::ClassificationProviderError,
            format!("Classification provider returned HTTP {status}; response body omitted."),
            &hint,
        )
    }
}

fn error_type(body: Option<&Value>) -> Option<&str> {
    body?.pointer("/detail/error_type")?.as_str()
}

/// Vendor overload signal carried in the body (`detail.error_type ==
/// "system_overloaded"`), retried like 503 whatever the status.
pub(super) fn is_overloaded(body: Option<&Value>) -> bool {
    error_type(body) == Some("system_overloaded")
}
