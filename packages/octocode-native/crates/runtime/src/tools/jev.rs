//! Pure System One evaluation: the host supplies state and typed questions.
use super::jev_transport::{JevProviderError, check_budget, endpoint, post};
use crate::providers::RequestBudget;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};

const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;

fn request_error(message: &str) -> JevProviderError {
    JevProviderError {
        code: "invalidJevRequest".to_owned(),
        message: message.to_owned(),
        hints: vec!["Inspect octocode tools jev --scheme.".to_owned()],
    }
}

fn entry(value: &Value) -> bool {
    matches!(
        value,
        Value::Null | Value::String(_) | Value::Array(_) | Value::Object(_)
    )
}

pub(crate) fn preflight(query: &Value, model: &str) -> Result<(), JevProviderError> {
    if query.to_string().len() > MAX_REQUEST_BYTES {
        return Err(request_error("Jev request exceeded the 4 MiB limit."));
    }
    let mut pure = query.clone();
    if let Some(object) = pure.as_object_mut() {
        object.remove("sources");
    }
    prepare(&pure, model).map(|_| ())
}

fn prepare(query: &Value, model: &str) -> Result<Value, JevProviderError> {
    let query = query
        .as_object()
        .ok_or_else(|| request_error("Query must be an object."))?;
    if query.len() != 2 || !query.contains_key("state") || !query.contains_key("questions") {
        return Err(request_error(
            "Supply only state and questions; model is configured internally.",
        ));
    }
    if !entry(&query["state"]) {
        return Err(request_error(
            "State must be a string, object, array, or null.",
        ));
    }
    let questions = query["questions"]
        .as_object()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| request_error("Questions must be a nonempty object."))?;
    if questions.keys().any(String::is_empty) {
        return Err(request_error("Question IDs must be nonempty."));
    }
    for question in questions.values() {
        let question = question
            .as_object()
            .ok_or_else(|| request_error("Each question must be an object."))?;
        if question
            .keys()
            .any(|key| !matches!(key.as_str(), "type" | "instructions" | "criteria"))
            || !question.get("instructions").is_some_and(entry)
        {
            return Err(request_error(
                "Each question requires explicit instructions and only type, instructions, and criteria fields.",
            ));
        }
        let criteria = question.get("criteria");
        let valid = match question.get("type").and_then(Value::as_str) {
            Some("noul") => criteria.is_none_or(|value| {
                value.is_null()
                    || value.as_object().is_some_and(|criteria| {
                        criteria.len() == 2
                            && criteria.contains_key("true")
                            && criteria.contains_key("false")
                            && criteria.values().all(entry)
                    })
            }),
            Some("choice") => criteria.and_then(Value::as_object).is_some_and(|criteria| {
                (1..=255).contains(&criteria.len())
                    && criteria.keys().all(|key| !key.is_empty())
                    && criteria.values().all(entry)
            }),
            Some("score") => criteria.and_then(Value::as_array).is_some_and(|criteria| {
                (2..=10).contains(&criteria.len()) && criteria.iter().all(entry)
            }),
            _ => false,
        };
        if !valid {
            return Err(request_error(
                "Use noul with optional true/false criteria, choice with 1..255 labeled criteria, or score with 2..10 ordered criteria.",
            ));
        }
    }
    let request = json!({"model": model, "state": query["state"], "questions": query["questions"]});
    if request.to_string().len() > MAX_REQUEST_BYTES {
        return Err(request_error("Jev request exceeded the 4 MiB limit."));
    }
    Ok(request)
}

pub async fn execute(
    query: &Value,
    key: SecretString,
    base_url: &str,
    model: &str,
    budget: RequestBudget,
    retries: u32,
) -> Result<Value, JevProviderError> {
    check_budget(&budget)?;
    let request = prepare(query, model)?;
    if key.expose_secret().chars().any(char::is_control) {
        return Err(JevProviderError {
            code: "invalidJevConfiguration".to_owned(),
            message: "OCTOCODE_JEV_KEY contains invalid control characters.".to_owned(),
            hints: vec!["Replace the configured key.".to_owned()],
        });
    }
    let response = post(&request, &key, endpoint(base_url)?, &budget, retries).await?;
    octocode_engine::jev::validate_response(&request, &response).map_err(|error| {
        JevProviderError {
            code: error.code.to_owned(),
            message: error.message,
            hints: vec!["Inspect provider compatibility before using the answer.".to_owned()],
        }
    })?;
    let answers: serde_json::Map<String, Value> = request["questions"]
        .as_object()
        .expect("request questions validated")
        .iter()
        .map(|(id, question)| {
            let answer = &response["answers"][id];
            let projected = match question["type"].as_str() {
                Some("noul") => json!({"type":"noul", "noul":answer["noul"]}),
                Some("choice") => json!({"type":"choice", "choice":answer["choice"], "confidence":answer["confidence"], "probabilities":answer["probabilities"]}),
                Some("score") => {
                    let legend: serde_json::Map<String, Value> = question["criteria"].as_array().expect("score criteria validated").iter().enumerate().map(|(index, value)| (index.to_string(), value.clone())).collect();
                    json!({"type":"score", "score":answer["score"], "confidence":answer["confidence"], "probabilities":answer["probabilities"], "legend":legend})
                }
                _ => unreachable!("question type validated"),
            };
            (id.clone(), projected)
        })
        .collect();
    Ok(
        json!({"model":request["model"], "answers":answers, "usage":{
            "input_tokens":response["usage"]["input_tokens"], "output_tokens":response["usage"]["output_tokens"]
        }}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, method, path},
    };

    fn query() -> Value {
        json!({"state": {"observations": ["one", "two"], "flag": true}, "questions": {
            "truth": {"type": "noul", "instructions": {"task": "Assess support"}, "criteria": {"true": ["supported"], "false": null}},
            "pick": {"type": "choice", "instructions": "Select", "criteria": {"a": {"meaning": "First"}, "b": ["Second"]}},
            "quality": {"type": "score", "instructions": ["Assess quality"], "criteria": [null, {"meaning": "Good"}]}
        }})
    }

    fn response() -> Value {
        json!({"model": "configured-model", "usage": {"input_tokens": 10, "output_tokens": 3}, "answers": {
            "truth": {"type": "noul", "noul": 0.4},
            "pick": {"type": "choice", "choice": "a", "confidence": 0.8, "probabilities": {"a": 0.8, "b": 0.2}},
            "quality": {"type": "score", "score": 0.75, "confidence": 0.7, "probabilities": {"0": 0.25, "1": 0.75}, "legend": {"0": null, "1": {"meaning": "Good"}}}
        }})
    }

    fn budget() -> RequestBudget {
        super::super::jev_transport::budget(
            Instant::now() + Duration::from_secs(30),
            tokio_util::sync::CancellationToken::new(),
        )
    }

    #[test]
    fn preparation_preserves_supplied_values_and_injects_only_model() {
        let query = query();
        let request = prepare(&query, "configured-model").unwrap();
        assert_eq!(request["state"], query["state"]);
        assert_eq!(request["questions"], query["questions"]);
        assert_eq!(request["model"], "configured-model");
        assert_eq!(request.as_object().unwrap().len(), 3);
        for state in [Value::Null, json!([]), json!("{\"literal\":true}")] {
            let mut value = query.clone();
            value["state"] = state.clone();
            assert_eq!(prepare(&value, "configured-model").unwrap()["state"], state);
        }
    }

    #[test]
    fn rejects_workflow_fields_missing_instructions_and_invalid_primitives() {
        for field in ["model", "route", "reasoning", "goal", "debug", "sources"] {
            let mut value = query();
            value[field] = json!("unwanted");
            assert!(prepare(&value, "configured-model").is_err());
        }
        for question in [
            json!({"type": "noul"}),
            json!({"type": "noul", "instructions": true}),
            json!({"type": "noul", "instructions": null, "criteria": {"maybe": "unknown"}}),
            json!({"type": "noul", "instructions": null, "criteria": {}}),
            json!({"type": "noul", "instructions": null, "criteria": {"true": null}}),
            json!({"type": "choice", "instructions": null, "criteria": {}}),
            json!({"type": "choice", "instructions": null, "criteria": {"": null}}),
            json!({"type": "score", "instructions": null, "criteria": ["one"]}),
            json!({"type": "score", "instructions": null, "criteria": vec![Value::Null; 11]}),
        ] {
            assert!(
                prepare(
                    &json!({"state": null, "questions": {"q": question}}),
                    "configured-model"
                )
                .is_err()
            );
        }
        assert!(
            prepare(
                &json!({"state": false, "questions": {}}),
                "configured-model"
            )
            .is_err()
        );
        let mut oversized = query();
        oversized["state"] = json!("x".repeat(MAX_REQUEST_BYTES));
        assert!(prepare(&oversized, "configured-model").is_err());
    }

    #[test]
    fn primitive_bounds_and_empty_question_ids_match_the_public_contract() {
        for (count, valid) in [(1, true), (255, true), (256, false)] {
            let criteria: serde_json::Map<String, Value> = (0..count)
                .map(|index| (index.to_string(), Value::Null))
                .collect();
            let value = json!({"state": null, "questions": {"q": {
                "type": "choice", "instructions": null, "criteria": criteria
            }}});
            assert_eq!(prepare(&value, "configured-model").is_ok(), valid);
        }
        for count in [2, 10] {
            let value = json!({"state": null, "questions": {"q": {
                "type": "score", "instructions": null, "criteria": vec![Value::Null; count]
            }}});
            assert!(prepare(&value, "configured-model").is_ok());
        }
        assert!(
            prepare(
                &json!({"state": null, "questions": {
                    "": {"type": "noul", "instructions": null}
                }}),
                "configured-model"
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn shared_transport_preserves_all_primitive_answers_without_workflow_policy() {
        let server = MockServer::start().await;
        let query = query();
        let response = response();
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .and(body_json(prepare(&query, "configured-model").unwrap()))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response))
            .expect(1)
            .mount(&server)
            .await;
        let actual = execute(
            &query,
            SecretString::from("test-key".to_owned()),
            &server.uri(),
            "configured-model",
            budget(),
            0,
        )
        .await
        .unwrap();
        assert_eq!(actual, response);
    }

    #[tokio::test]
    async fn provider_extras_cannot_echo_hidden_source_bodies() {
        let server = MockServer::start().await;
        let mut supplied = response();
        supplied["answers"]["truth"]["content"] = json!("hidden-source-body");
        supplied["usage"]["content"] = json!("hidden-source-body");
        supplied["content"] = json!("hidden-source-body");
        supplied["model"] = json!("hidden-source-body");
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(supplied))
            .mount(&server)
            .await;
        let result = execute(
            &query(),
            SecretString::from("test-key".to_owned()),
            &server.uri(),
            "configured-model",
            budget(),
            0,
        )
        .await
        .unwrap();
        assert_eq!(result, response());
        assert!(!result.to_string().contains("hidden-source-body"));
    }

    #[tokio::test]
    async fn invalid_input_and_cancelled_requests_never_reach_provider() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        let mut invalid = query();
        invalid["model"] = json!("agent-selected");
        assert_eq!(
            execute(
                &invalid,
                SecretString::from("test-key".to_owned()),
                &server.uri(),
                "configured-model",
                budget(),
                0
            )
            .await
            .unwrap_err()
            .code,
            "invalidJevRequest"
        );
        let cancelled = budget();
        cancelled.cancellation.cancel();
        assert_eq!(
            execute(
                &query(),
                SecretString::from("test-key".to_owned()),
                &server.uri(),
                "configured-model",
                cancelled,
                0
            )
            .await
            .unwrap_err()
            .code,
            "cancelled"
        );
    }

    #[tokio::test]
    async fn shared_validator_rejects_answer_id_and_type_drift() {
        for invalid in [
            json!({}),
            json!({"truth": {"type": "choice"}, "pick": {}, "quality": {}}),
        ] {
            let server = MockServer::start().await;
            let mut response = response();
            response["answers"] = invalid;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(200).set_body_json(response))
                .expect(1)
                .mount(&server)
                .await;
            assert_eq!(
                execute(
                    &query(),
                    SecretString::from("test-key".to_owned()),
                    &server.uri(),
                    "configured-model",
                    budget(),
                    0
                )
                .await
                .unwrap_err()
                .code,
                "invalidJevResponse"
            );
        }
    }
}
