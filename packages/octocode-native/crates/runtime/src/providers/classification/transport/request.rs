use super::{
    BACKOFF, ClassificationError, MAX_ERROR_BODY_BYTES, MAX_RETRY_AFTER, budget_error,
    check_budget, denied, is_overloaded, overloaded_error, rate_limited, send_error, shared_client,
    status_error, transport_error, wait,
};
use crate::providers::classification::gate::{GateDenied, GateLease, GatePermit};
use octocode_contract_types::ErrorCode;
use octocode_io::budget::RequestBudget;
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use std::time::{Duration, Instant, SystemTime};
use url::Url;

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

pub(super) struct Request {
    pub(super) body: Option<bytes::Bytes>,
    pub(super) headers: reqwest::header::HeaderMap,
}

pub(super) async fn request(
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
