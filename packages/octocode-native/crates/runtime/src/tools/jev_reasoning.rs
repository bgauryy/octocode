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

pub(crate) fn endpoint(base_url: &str) -> Result<Url, JevProviderError> {
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

pub(super) fn check_budget(budget: &RequestBudget) -> Result<(), JevProviderError> {
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

pub(crate) async fn post(
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
    sources: super::jev_source_questions::SourceAccess<'_>,
) -> Result<Value, JevProviderError> {
    if query["route"] == "source_questions" {
        return super::jev_source_questions::execute(
            query,
            key,
            base_url,
            default_model,
            budget,
            retries,
            sources,
        )
        .await;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};

    #[tokio::test]
    async fn source_questions_read_both_files_in_one_provider_call() {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("request.rs");
        let second = dir.path().join("cache.rs");
        std::fs::write(&first, "if cancelled { return; }\ncache.write(result);").unwrap();
        std::fs::write(&second, "fn write(result: Result) { save(result); }").unwrap();
        let server = MockServer::start().await;
        let paths = crate::policy::path::PathPolicy::new(crate::policy::path::PathPolicyConfig {
            workspace_root: Some(dir.path().to_owned()),
            ..Default::default()
        })
        .unwrap();
        let security = crate::security::ContentSecurity::new(std::sync::Arc::new(
            crate::security::SecurityRegistry::default(),
        ));
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "model": "jev-test", "usage": {"input_tokens": 10, "output_tokens": 1},
                "answers": {"guarded": {"type": "choice", "choice": "insufficient", "confidence": 1.0,
                    "probabilities": {"supported": 0.0, "contradicted": 0.0, "insufficient": 1.0, "conflicting": 0.0}}}
            })))
            .expect(1)
            .mount(&server).await;
        let result = execute(
            &json!({"route": "source_questions", "sources": [{"path": first}, {"path": second}],
                "questions": {"guarded": "Cancellation prevents late cache writes."}}),
            SecretString::from("test-key".to_owned()),
            &server.uri(),
            "jev-test",
            budget(
                Instant::now() + Duration::from_secs(30),
                tokio_util::sync::CancellationToken::new(),
            ),
            0,
            super::super::jev_source_questions::SourceAccess {
                paths: &paths,
                security: &security,
                local_enabled: true,
            },
        )
        .await
        .expect("source references must be assembled before the provider call");
        assert_eq!(result["answers"]["guarded"]["choice"], "insufficient");
        assert!(result.get("nextAction").is_none());
        assert!(result.get("applied").is_none());
        assert_eq!(result["sources"].as_array().unwrap().len(), 2);
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(body["state"]["sources"].as_array().unwrap().len(), 2);
        assert!(
            body["state"]["sources"][0]["content"]
                .as_str()
                .unwrap()
                .contains("cancelled")
        );
        assert!(
            body["state"]["sources"][1]["content"]
                .as_str()
                .unwrap()
                .contains("save(result)")
        );
        assert!(!result.to_string().contains("save(result)"));
    }

    #[test]
    fn route_policy_action_maps_every_route_and_falls_back() {
        assert_eq!(route_policy_action("hunch_check"), "promote_or_drop_hunch");
        assert_eq!(
            route_policy_action("hypothesis_triage"),
            "test_selected_check"
        );
        assert_eq!(route_policy_action("reflection_delta"), "update_or_reframe");
        assert_eq!(
            route_policy_action("decision_review"),
            "review_before_action"
        );
        assert_eq!(
            route_policy_action("disputed_inference"),
            "verify_selected_basis"
        );
        assert_eq!(
            route_policy_action("hallucination_gate"),
            "gate_then_qualify"
        );
        assert_eq!(
            route_policy_action("anything_else"),
            "continue_host_research"
        );
    }

    #[test]
    fn deterministic_gate_skips_when_action_will_not_change() {
        let gate = deterministic_gate(&json!({"route": "hunch_check", "willChangeAction": false}))
            .expect("gate fires");
        assert_eq!(gate["gate"], "skipped");
        assert_eq!(gate["policyAction"], "act_without_jev");
        assert_eq!(gate["route"], "hunch_check");
        assert_eq!(gate["provisional"], true);
    }

    #[test]
    fn deterministic_gate_prefers_an_available_direct_check() {
        let gate = deterministic_gate(&json!({
            "route": "disputed_inference",
            "willChangeAction": true,
            "directCheck": {"available": true, "action": "Run the compiler."}
        }))
        .expect("gate fires");
        assert_eq!(gate["gate"], "skipped");
        assert_eq!(gate["policyAction"], "run_direct_check");
        assert_eq!(gate["nextAction"], "Run the compiler.");
    }

    #[test]
    fn deterministic_gate_avoids_a_repeat_call_at_the_same_crossroad() {
        let gate = deterministic_gate(&json!({
            "route": "decision_review",
            "willChangeAction": true,
            "jevCallsAtCrossroad": 1
        }))
        .expect("gate fires");
        assert_eq!(gate["gate"], "skipped");
        assert_eq!(gate["policyAction"], "continue_host_research");
    }

    #[test]
    fn deterministic_gate_demands_fresh_evidence() {
        let gate = deterministic_gate(&json!({
            "route": "hunch_check",
            "willChangeAction": true,
            "evidenceFresh": false
        }))
        .expect("gate fires");
        assert_eq!(gate["gate"], "needsEvidence");
        assert_eq!(gate["policyAction"], "refresh_evidence");
    }

    #[test]
    fn deterministic_gate_lets_a_clear_crossroad_reach_the_provider() {
        // willChangeAction true, no direct check, first call, fresh evidence => no gate.
        assert!(
            deterministic_gate(&json!({
                "route": "hunch_check",
                "willChangeAction": true,
                "jevCallsAtCrossroad": 0,
                "evidenceFresh": true
            }))
            .is_none()
        );
    }

    #[test]
    fn endpoint_accepts_an_https_root_and_appends_the_system_one_path() {
        let url = endpoint("https://api.typesafe.ai").expect("valid root");
        assert_eq!(url.as_str(), "https://api.typesafe.ai/v1/systemone");
        // A trailing slash is also a bare root.
        assert!(endpoint("https://api.typesafe.ai/").is_ok());
        // HTTP is allowed only on loopback.
        assert!(endpoint("http://127.0.0.1").is_ok());
    }

    #[test]
    fn endpoint_rejects_non_root_or_insecure_bases() {
        for bad in [
            "http://api.typesafe.ai",            // http, non-loopback
            "https://api.typesafe.ai/v1",        // has a path
            "https://api.typesafe.ai/?x=1",      // has a query
            "https://api.typesafe.ai/#frag",     // has a fragment
            "https://user:pass@api.typesafe.ai", // has credentials
            "not a url",
        ] {
            assert!(endpoint(bad).is_err(), "should reject {bad}");
        }
    }

    #[test]
    fn retry_delay_honours_headers_then_falls_back_to_backoff() {
        let mut ms = HeaderMap::new();
        ms.insert("retry-after-ms", HeaderValue::from_static("1500"));
        assert_eq!(retry_delay(&ms, 3), Duration::from_millis(1500));

        let mut secs = HeaderMap::new();
        secs.insert(RETRY_AFTER, HeaderValue::from_static("2"));
        assert_eq!(retry_delay(&secs, 3), Duration::from_secs(2));

        // No headers: 500ms * 2^attempt.
        let empty = HeaderMap::new();
        assert_eq!(retry_delay(&empty, 0), Duration::from_millis(500));
        assert_eq!(retry_delay(&empty, 2), Duration::from_millis(2000));
    }
}
