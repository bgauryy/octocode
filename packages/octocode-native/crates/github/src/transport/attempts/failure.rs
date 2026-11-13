//! How a failed response is classified: primary or secondary rate limit,
//! a retried server error, or a final answer, and the wait each allows.
use bytes::Bytes;
use reqwest::{StatusCode, header::HeaderMap};
use std::{sync::Arc, time::Duration};

use octocode_io::retry::{classify_status, header_u64, retry_after_delay};

use super::super::response::response_error;
use super::{Attempts, RetryWait};
use crate::budget::{
    CircuitCause, ExecutorConfig, GitHubResource, KeyState, is_primary_rate_limit,
    is_secondary_rate_limit, names_secondary_limit, now_ms,
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
            Failure::Primary {
                reset,
                wait,
                secondary,
            } => {
                if secondary {
                    // GitHub named a secondary limit too: the key cools down
                    // for its documented wait beside the spent bucket, since
                    // requests sent during a secondary limit can get the
                    // integration banned. The spent bucket still answers
                    // this request at once when it is not paced (D1).
                    *self.failed = Some(CircuitCause::Secondary);
                    secondary_cooldown(
                        &self.state,
                        headers,
                        secondary_wait(headers, self.config),
                        resource,
                    );
                }
                let bucket = headers
                    .get("x-ratelimit-resource")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or(resource.bucket())
                    .to_owned();
                // `retry-after` wins over the reset header: the bucket stays
                // blocked until the moment GitHub says a retry may run.
                let blocked_until = provider_retry_after(headers).map_or(reset, |after| {
                    now_ms()
                        .saturating_add(crate::budget::millis(after))
                        .div_ceil(1000)
                });
                self.state.exhaust(&bucket, blocked_until);
                let waited_out = crate::budget::waits_out_spent(&bucket);
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
                // A paced bucket's next loop turn waits in `wait_unblocked`;
                // any other spent quota is this answer.
                waited_out.then_some(RetryWait {
                    wait,
                    sleep: Duration::ZERO,
                    rate_limited: true,
                })
            }
            Failure::Secondary { wait } => {
                *self.failed = Some(CircuitCause::Secondary);
                error.kind = ProviderErrorKind::RateLimited;
                error.retryable = true;
                error.message = "GitHub secondary rate limit exceeded".into();
                error.rate_limit = Some(Box::new(secondary_cooldown(
                    &self.state,
                    headers,
                    wait,
                    resource,
                )));
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
    /// Primary limit: bucket exhausted until `reset` (epoch seconds);
    /// `secondary` when GitHub's message names a secondary limit as well.
    Primary {
        reset: u64,
        wait: Duration,
        secondary: bool,
    },
    /// Secondary limit: key-wide cooldown for `wait`.
    Secondary { wait: Duration },
    /// 5xx or 408: retried with jittered backoff or `retry-after`.
    Server,
    /// Octokit doNotRetry (400/401/403/404/410/422/451) and everything else.
    Final,
}

/// How long a secondary limit answered with `headers` holds: what GitHub says,
/// else the documented default. A spent bucket's reset is not a secondary
/// wait: such an answer is that bucket's limit ([`spent_bucket_reset`]).
pub(in crate::transport) fn secondary_wait(
    headers: &HeaderMap,
    config: &ExecutorConfig,
) -> Duration {
    provider_retry_after(headers).unwrap_or(config.secondary_default)
}

/// The reset of the bucket a rate-limited answer's headers say is spent
/// (`x-ratelimit-remaining: 0` with `x-ratelimit-reset`) when they name no
/// `retry-after`. GitHub's rule for any limit is then to wait until that
/// reset, so the answer is that bucket's limit whatever its message calls
/// it, and the one rule for a spent bucket
/// ([`crate::budget::waits_out_spent`]) decides whether it is waited out.
pub(in crate::transport) fn spent_bucket_reset(headers: &HeaderMap) -> Option<u64> {
    if header_u64(headers, "x-ratelimit-remaining") != Some(0)
        || provider_retry_after(headers).is_some()
    {
        return None;
    }
    header_u64(headers, "x-ratelimit-reset")
}

/// Start the key's cooldown for a secondary limit, whatever status carried it.
/// Its `resetEpochSeconds` is the cooldown end: the primary window reset GitHub
/// also sends is unrelated to when a retry can run.
pub(in crate::transport) fn secondary_cooldown(
    state: &Arc<KeyState>,
    headers: &HeaderMap,
    wait: Duration,
    resource: GitHubResource,
) -> RateLimit {
    let cooldown_end = now_ms().saturating_add(crate::budget::millis(wait));
    state.cool_down(cooldown_end);
    RateLimit {
        remaining: header_u64(headers, "x-ratelimit-remaining"),
        reset_epoch_seconds: Some(cooldown_end.div_ceil(1000)),
        retry_after_seconds: Some(crate::budget::ceil_secs(wait)),
        resource: Some(
            headers
                .get("x-ratelimit-resource")
                .and_then(|value| value.to_str().ok())
                .unwrap_or(resource.bucket())
                .into(),
        ),
    }
}

/// `Retry-After` (delta-seconds or HTTP-date) or `retry-after-ms`.
fn provider_retry_after(headers: &HeaderMap) -> Option<Duration> {
    retry_after_delay(headers, Duration::MAX, std::time::SystemTime::now())
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
    // The headers decide first: a spent bucket is that bucket's limit, even
    // under a message naming a secondary limit, which then also cools the key.
    if matches!(code, 403 | 429)
        && let Some(reset) = spent_bucket_reset(headers)
    {
        return Failure::Primary {
            reset,
            wait: config.reset_wait(reset),
            secondary: names_secondary_limit(&text),
        };
    }
    if is_secondary_rate_limit(code, remaining, retry_after, &text) {
        return Failure::Secondary {
            wait: secondary_wait(headers, config),
        };
    }
    if is_primary_rate_limit(code, remaining, &text) {
        let reset = reset.unwrap_or_else(|| {
            now_ms() / 1000 + retry_after.unwrap_or(config.secondary_default.as_secs())
        });
        return Failure::Primary {
            reset,
            wait: requested.unwrap_or_else(|| config.reset_wait(reset)),
            secondary: false,
        };
    }
    if classify_status(code).is_retryable() {
        Failure::Server
    } else {
        Failure::Final
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_retry_after_preserves_declared_delays_without_a_day_cap() {
        for seconds in [2 * 24 * 60 * 60, u64::MAX] {
            let mut headers = HeaderMap::new();
            headers.insert(
                "retry-after",
                reqwest::header::HeaderValue::from_str(&seconds.to_string()).expect("header"),
            );
            assert_eq!(
                provider_retry_after(&headers),
                Some(Duration::from_secs(seconds))
            );
        }
    }
}
