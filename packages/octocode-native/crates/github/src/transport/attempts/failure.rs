//! How a failed response is classified: primary or secondary rate limit,
//! a retried server error, or a final answer, and the wait each allows.
use bytes::Bytes;
use reqwest::{StatusCode, header::HeaderMap};
use std::time::Duration;

use octocode_io::retry::{classify_status, header_u64, retry_after_delay};

use super::super::response::response_error;
use super::{Attempts, RetryWait};
use crate::budget::{
    CircuitCause, ExecutorConfig, is_primary_rate_limit, is_secondary_rate_limit, now_ms,
};
use crate::{ProviderError, ProviderErrorKind, RateLimit};

impl Attempts<'_> {
    /// The error for a failed response, after recording any rate limit it
    /// carries, and how it may be retried (`None`: not at all).
    pub(super) fn classify(
        &mut self,
        status: StatusCode,
        headers: &HeaderMap,
        body: Bytes,
        body_note: Option<&str>,
    ) -> (ProviderError, Option<RetryWait>) {
        let failure = classify_failure(status, headers, &body, self.config);
        let mut error = response_error(status, headers, body);
        if let Some(note) = body_note {
            error.message = format!("{} ({note})", error.message).into_boxed_str();
        }
        let resource = self.resource;
        let backoff = match failure {
            Failure::Primary { reset, wait } => {
                let bucket = headers
                    .get("x-ratelimit-resource")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or(resource.bucket())
                    .to_owned();
                self.state.exhaust(&bucket, reset);
                error.kind = ProviderErrorKind::RateLimited;
                error.retryable = true;
                error.rate_limit =
                    Some(Box::new(RateLimit {
                        remaining: Some(0),
                        reset_epoch_seconds: Some(reset),
                        retry_after_seconds: Some(provider_retry_after(headers).map_or_else(
                            || crate::budget::ceil_secs(wait),
                            crate::budget::ceil_secs,
                        )),
                        resource: Some(bucket.into()),
                    }));
                // The next loop turn waits in `wait_unblocked`.
                Some(RetryWait {
                    wait,
                    sleep: Duration::ZERO,
                    rate_limited: true,
                })
            }
            Failure::Secondary { wait } => {
                *self.failed = Some(CircuitCause::Secondary);
                let cooldown_end = now_ms().saturating_add(crate::budget::millis(wait));
                self.state.cool_down(cooldown_end);
                error.kind = ProviderErrorKind::RateLimited;
                error.retryable = true;
                error.message = "GitHub secondary rate limit exceeded".into();
                error.rate_limit = Some(Box::new(RateLimit {
                    remaining: header_u64(headers, "x-ratelimit-remaining"),
                    // The cooldown end: the primary window reset GitHub
                    // also sends is unrelated to when a retry can run.
                    reset_epoch_seconds: Some(cooldown_end.div_ceil(1000)),
                    retry_after_seconds: Some(crate::budget::ceil_secs(wait)),
                    resource: Some(
                        headers
                            .get("x-ratelimit-resource")
                            .and_then(|value| value.to_str().ok())
                            .unwrap_or(resource.bucket())
                            .into(),
                    ),
                }));
                Some(RetryWait {
                    wait,
                    sleep: Duration::ZERO,
                    rate_limited: true,
                })
            }
            Failure::Server => {
                *self.failed = Some(CircuitCause::Server);
                let requested = provider_retry_after(headers);
                if let Some(requested) = requested {
                    // A long provider delay fails fast in `retry`; the
                    // caller still learns when to retry.
                    error.rate_limit = Some(Box::new(RateLimit {
                        remaining: None,
                        reset_epoch_seconds: None,
                        retry_after_seconds: Some(crate::budget::ceil_secs(requested)),
                        resource: None,
                    }));
                }
                let retry = &self.transport.retry;
                let delay = retry.backoff().after(requested, u32::from(*self.attempt));
                Some(RetryWait {
                    wait: delay,
                    sleep: delay,
                    rate_limited: false,
                })
            }
            Failure::Final => None,
        };
        (error, backoff)
    }
}

enum Failure {
    /// Primary limit: bucket exhausted until `reset` (epoch seconds).
    Primary { reset: u64, wait: Duration },
    /// Secondary limit: key-wide cooldown for `wait`.
    Secondary { wait: Duration },
    /// 5xx or 408: retried with jittered backoff or `retry-after`.
    Server,
    /// Octokit doNotRetry (400/401/403/404/410/422/451) and everything else.
    Final,
}

/// The longest provider-requested retry delay honored: a day.
const MAX_PROVIDER_RETRY_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

/// `Retry-After` (delta-seconds or HTTP-date) or `retry-after-ms`.
fn provider_retry_after(headers: &HeaderMap) -> Option<Duration> {
    retry_after_delay(
        headers,
        MAX_PROVIDER_RETRY_AFTER,
        std::time::SystemTime::now(),
    )
}

fn classify_failure(
    status: StatusCode,
    headers: &HeaderMap,
    body: &[u8],
    config: &ExecutorConfig,
) -> Failure {
    let code = status.as_u16();
    let remaining = header_u64(headers, "x-ratelimit-remaining");
    let requested = provider_retry_after(headers);
    let retry_after = requested.map(crate::budget::ceil_secs);
    let reset = header_u64(headers, "x-ratelimit-reset");
    let text = String::from_utf8_lossy(body);
    if is_secondary_rate_limit(code, remaining, retry_after, &text) {
        let wait = match (requested, remaining, reset) {
            (Some(requested), _, _) => requested,
            (None, Some(0), Some(reset)) => config.reset_wait(reset),
            _ => config.secondary_default,
        };
        return Failure::Secondary { wait };
    }
    if is_primary_rate_limit(code, remaining, &text) {
        let reset = reset.unwrap_or_else(|| {
            now_ms() / 1000 + retry_after.unwrap_or(config.secondary_default.as_secs())
        });
        return Failure::Primary {
            reset,
            wait: config.reset_wait(reset),
        };
    }
    if classify_status(code).is_retryable() {
        Failure::Server
    } else {
        Failure::Final
    }
}
