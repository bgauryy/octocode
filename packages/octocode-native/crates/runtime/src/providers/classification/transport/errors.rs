use super::ClassificationError;
use crate::providers::classification::CIRCUIT_OPEN;
use crate::providers::classification::gate::GateDenied;
use octocode_contract_types::ErrorCode;
use octocode_io::budget::BudgetStop;
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
            "Classification request exceeded its total deadline.",
            "Retry unresolved evidence in a smaller request.",
        ),
    }
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
        "Check provider availability and OCTOCODE_CLASSIFICATION_API_HOST, then retry.",
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
pub(super) fn status_error(
    status: reqwest::StatusCode,
    provider_body: Option<&Value>,
) -> ClassificationError {
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
    let class = classify_status(status.as_u16());
    if class.is_server() || class == StatusClass::Timeout {
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
pub(super) fn is_overloaded(body: Option<&Value>) -> bool {
    error_type(body) == Some("system_overloaded")
}
