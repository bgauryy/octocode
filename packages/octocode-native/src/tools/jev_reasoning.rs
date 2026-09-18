use crate::providers::RequestBudget;
use bytes::BytesMut;
use futures_util::StreamExt;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};
use std::future::Future;
use std::time::{Duration, Instant};
use url::Url;

const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JevProviderError {
    pub code: String,
    pub message: String,
    pub hints: Vec<String>,
}

impl JevProviderError {
    fn new(code: &str, message: impl Into<String>, hint: &str) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            hints: vec![hint.to_owned()],
        }
    }
}

fn route_policy_action(route: &str) -> &'static str {
    match route {
        "hunch_check" => "promote_or_drop_hunch",
        "hypothesis_triage" => "test_selected_check",
        "reflection_delta" => "update_or_reframe",
        "decision_review" => "review_before_action",
        "disputed_inference" => "verify_selected_basis",
        "hallucination_gate" => "gate_then_qualify",
        _ => "continue_host_research",
    }
}

fn deterministic_gate(query: &Value) -> Option<Value> {
    let route = query["route"].as_str().unwrap_or("unknown");
    let result = if query["willChangeAction"] == false {
        Some((
            "skipped",
            "act_without_jev",
            "Continue without a Jev judgment.",
        ))
    } else if query.pointer("/directCheck/available") == Some(&Value::Bool(true)) {
        Some((
            "skipped",
            "run_direct_check",
            query
                .pointer("/directCheck/action")
                .and_then(Value::as_str)
                .unwrap_or("Run the available deterministic check."),
        ))
    } else if query["jevCallsAtCrossroad"].as_u64().unwrap_or(0) >= 1 {
        Some((
            "skipped",
            "continue_host_research",
            "Continue host research without another Jev call at this crossroad.",
        ))
    } else if query["evidenceFresh"] == false {
        Some((
            "needsEvidence",
            "refresh_evidence",
            "Refresh the stale evidence before making a judgment.",
        ))
    } else {
        None
    };
    result.map(|(gate, policy_action, next_action)| {
        json!({
            "route": route,
            "gate": gate,
            "provisional": true,
            "policyAction": policy_action,
            "nextAction": next_action,
        })
    })
}

fn endpoint(base_url: &str) -> Result<Url, JevProviderError> {
    let base = Url::parse(base_url).map_err(|_| {
        JevProviderError::new(
            "invalidJevConfiguration",
            "OCTOCODE_JEV_BASE_URL is not a valid URL.",
            "Use an HTTPS API root without a path, query, fragment, or credentials.",
        )
    })?;
    let host = base.host_str().unwrap_or_default();
    let loopback = matches!(host, "localhost" | "127.0.0.1" | "::1");
    let valid_scheme = base.scheme() == "https" || (base.scheme() == "http" && loopback);
    if !valid_scheme
        || host.is_empty()
        || !base.username().is_empty()
        || base.password().is_some()
        || !matches!(base.path(), "" | "/")
        || base.query().is_some()
        || base.fragment().is_some()
    {
        return Err(JevProviderError::new(
            "invalidJevConfiguration",
            "OCTOCODE_JEV_BASE_URL must be an HTTPS API root; HTTP is allowed only on loopback.",
            "Remove paths, credentials, query strings, and fragments from the configured API root.",
        ));
    }
    base.join("v1/systemone").map_err(|_| {
        JevProviderError::new(
            "invalidJevConfiguration",
            "Cannot construct the Jev System One endpoint.",
            "Check OCTOCODE_JEV_BASE_URL.",
        )
    })
}

fn check_budget(budget: &RequestBudget) -> Result<(), JevProviderError> {
    if budget.cancellation.is_cancelled() {
        return Err(JevProviderError::new(
            "cancelled",
            "Jev request was cancelled.",
            "Retry only if the reasoning crossroad is still unresolved.",
        ));
    }
    if Instant::now() >= budget.deadline {
        return Err(JevProviderError::new(
            "timeout",
            "Jev request exceeded its total deadline.",
            "Retry later or increase the shared request timeout.",
        ));
    }
    Ok(())
}

async fn wait<T>(
    budget: &RequestBudget,
    future: impl Future<Output = T>,
) -> Result<T, JevProviderError> {
    check_budget(budget)?;
    let remaining = budget.deadline.saturating_duration_since(Instant::now());
    tokio::select! {
        _ = budget.cancellation.cancelled() => Err(JevProviderError::new(
            "cancelled",
            "Jev request was cancelled.",
            "Retry only if the reasoning crossroad is still unresolved.",
        )),
        value = tokio::time::timeout(remaining, future) => value.map_err(|_| JevProviderError::new(
            "timeout",
            "Jev request exceeded its total deadline.",
            "Retry later or increase the shared request timeout.",
        )),
    }
}

fn retry_delay(headers: &reqwest::header::HeaderMap, attempt: u32) -> Duration {
    if let Some(milliseconds) = headers
        .get("retry-after-ms")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        return Duration::from_millis(milliseconds);
    }
    if let Some(seconds) = headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        return Duration::from_secs(seconds);
    }
    Duration::from_millis(500_u64.saturating_mul(1_u64 << attempt.min(10)))
}

fn transport_error() -> JevProviderError {
    JevProviderError::new(
        "jevProviderError",
        "Jev HTTPS request failed or timed out.",
        "Check connectivity, certificates, proxy settings, and the configured API root.",
    )
}

async fn post(
    request: &Value,
    key: &SecretString,
    endpoint: Url,
    budget: &RequestBudget,
    retries: u32,
) -> Result<Value, JevProviderError> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| transport_error())?;
    for attempt in 0..=retries {
        check_budget(budget)?;
        let response = wait(
            budget,
            client
                .post(endpoint.clone())
                .bearer_auth(key.expose_secret())
                .header(reqwest::header::ACCEPT, "application/json")
                .json(request)
                .send(),
        )
        .await?
        .map_err(|_| transport_error())?;
        let status = response.status();
        if matches!(status.as_u16(), 429 | 503 | 529) && attempt < retries {
            let delay = retry_delay(response.headers(), attempt);
            if delay >= budget.deadline.saturating_duration_since(Instant::now()) {
                return Err(JevProviderError::new(
                    "timeout",
                    format!("HTTP {status}: retry delay exceeds the remaining Jev deadline."),
                    "Retry later.",
                ));
            }
            drop(response);
            wait(budget, tokio::time::sleep(delay)).await?;
            continue;
        }
        if !status.is_success() {
            let hint = match status.as_u16() {
                401 | 403 => "Check OCTOCODE_JEV_KEY and account access.",
                400 | 422 => "Check the selected route, bounded state, model, and request size.",
                429 | 503 | 529 => "Retry later or reduce request volume.",
                300..=399 => "Redirects are disabled; check OCTOCODE_JEV_BASE_URL.",
                _ => "Check provider availability and OCTOCODE_JEV_BASE_URL.",
            };
            return Err(JevProviderError::new(
                "jevProviderError",
                format!("Jev provider returned HTTP {status}; response body omitted."),
                hint,
            ));
        }
        let mut body = BytesMut::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = wait(budget, stream.next()).await? {
            let chunk = chunk.map_err(|_| transport_error())?;
            if body.len().saturating_add(chunk.len()) > budget.max_body_bytes {
                return Err(JevProviderError::new(
                    "invalidJevResponse",
                    "Jev response exceeded the 4 MiB limit.",
                    "Reduce the bounded state or selected route breadth.",
                ));
            }
            body.extend_from_slice(&chunk);
        }
        return serde_json::from_slice(&body).map_err(|_| {
            JevProviderError::new(
                "invalidJevResponse",
                "Jev provider returned invalid JSON.",
                "Keep downstream actions stopped and inspect provider compatibility.",
            )
        });
    }
    Err(transport_error())
}

pub async fn execute(
    query: &Value,
    key: SecretString,
    base_url: &str,
    default_model: &str,
    budget: RequestBudget,
    retries: u32,
) -> Result<Value, JevProviderError> {
    if let Some(result) = deterministic_gate(query) {
        return Ok(result);
    }
    if key.expose_secret().chars().any(char::is_control) {
        return Err(JevProviderError::new(
            "invalidJevConfiguration",
            "OCTOCODE_JEV_KEY contains invalid control characters.",
            "Replace the configured key.",
        ));
    }
    let route = query["route"].as_str().unwrap_or("unknown");
    let request = octocode_engine::jev::build_request(query, default_model).map_err(|error| {
        JevProviderError::new(
            error.code,
            error.message,
            "Inspect the jevReasoning schema.",
        )
    })?;
    let response = post(&request, &key, endpoint(base_url)?, &budget, retries).await?;
    octocode_engine::jev::validate_response(&request, &response).map_err(|error| {
        JevProviderError::new(
            error.code,
            error.message,
            "Keep downstream actions stopped and inspect provider compatibility.",
        )
    })?;
    let applied =
        octocode_engine::jev::apply_response(route, &request, &response).map_err(|error| {
            JevProviderError::new(
                error.code,
                error.message,
                "Keep the judgment provisional and continue host-owned evidence work.",
            )
        })?;
    Ok(json!({
        "route": route,
        "gate": "judgment",
        "provisional": true,
        "policyAction": route_policy_action(route),
        "nextAction": applied["nextAction"],
        "model": response["model"],
        "answers": response["answers"],
        "usage": response["usage"],
        "applied": applied,
    }))
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
