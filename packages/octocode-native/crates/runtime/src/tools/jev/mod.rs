//! One typed judgment over an explicit value or bounded ordinary tool result.
pub(crate) mod batch;
pub(crate) mod transport;

use self::transport::{JevProviderError, check_budget, endpoint, post};
use crate::{providers::RequestBudget, tools::id::ToolId};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};

const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;

fn request_error(message: &str) -> JevProviderError {
    JevProviderError {
        code: "invalidJevRequest".into(),
        message: message.into(),
        hints: vec!["Inspect octocode tools jev --scheme.".into()],
    }
}

fn entry(value: &Value) -> bool {
    matches!(
        value,
        Value::Null | Value::String(_) | Value::Array(_) | Value::Object(_)
    )
}

pub(crate) fn is_context_tool(tool: &str) -> bool {
    matches!(
        ToolId::from_name(tool),
        Some(
            ToolId::GhSearch
                | ToolId::GhGetFileContent
                | ToolId::GhSearchHistory
                | ToolId::GhGetHistoryItem
                | ToolId::ArtifactSearch
                | ToolId::LocalSearch
                | ToolId::LocalFetch
                | ToolId::AstSearch
                | ToolId::LspSearch
        )
    )
}

pub(crate) fn preflight(query: &Value) -> Result<(), JevProviderError> {
    if query.to_string().len() > MAX_REQUEST_BYTES {
        return Err(request_error("Jev request exceeded the 4 MiB limit."));
    }
    let query = query
        .as_object()
        .ok_or_else(|| request_error("Query must be an object."))?;
    if query.len() != 3
        || !query.contains_key("context")
        || !query.contains_key("question")
        || !query
            .get("reasoning")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(request_error(
            "Supply nonblank reasoning, context and one typed question.",
        ));
    }
    validate_question(&query["question"])?;
    let context = query["context"]
        .as_object()
        .ok_or_else(|| request_error("Context must be an object."))?;
    if context.len() == 1 && context.get("value").is_some_and(entry) {
        return Ok(());
    }
    if context.len() != 2
        || !context
            .get("tool")
            .and_then(Value::as_str)
            .is_some_and(is_context_tool)
        || !context.get("query").is_some_and(Value::is_object)
    {
        return Err(request_error(
            "Context requires value, or an allowed read tool with one ordinary query.",
        ));
    }
    Ok(())
}

fn validate_question(question: &Value) -> Result<(), JevProviderError> {
    let question = question
        .as_object()
        .ok_or_else(|| request_error("Question must be an object."))?;
    if question
        .keys()
        .any(|key| !matches!(key.as_str(), "type" | "instructions" | "criteria"))
        || !question.get("instructions").is_some_and(entry)
    {
        return Err(request_error(
            "Question requires instructions and only type, instructions, and criteria fields.",
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
    Ok(())
}

fn prepare(state: &Value, question: &Value, model: &str) -> Result<Value, JevProviderError> {
    if !entry(state) {
        return Err(request_error(
            "Context value must be a string, object, array, or null.",
        ));
    }
    validate_question(question)?;
    let request = json!({"model":model, "state":state, "questions":{"answer":question}});
    if request.to_string().len() > MAX_REQUEST_BYTES {
        return Err(request_error("Jev request exceeded the 4 MiB limit."));
    }
    Ok(request)
}

pub async fn execute(
    state: &Value,
    question: &Value,
    key: SecretString,
    base_url: &str,
    model: &str,
    budget: RequestBudget,
    retries: u32,
) -> Result<Value, JevProviderError> {
    check_budget(&budget)?;
    let request = prepare(state, question, model)?;
    if key.expose_secret().chars().any(char::is_control) {
        return Err(JevProviderError {
            code: "invalidJevConfiguration".into(),
            message: "OCTOCODE_JEV_KEY contains invalid control characters.".into(),
            hints: vec!["Replace the configured key.".into()],
        });
    }
    let response = post(&request, &key, endpoint(base_url)?, &budget, retries).await?;
    octocode_engine::jev::validate_response(&request, &response).map_err(|error| {
        JevProviderError {
            code: error.code.into(),
            message: error.message,
            hints: vec!["Inspect provider compatibility before using the answer.".into()],
        }
    })?;
    project(
        question,
        &response["answers"]["answer"],
        model,
        &response["usage"],
    )
}

fn project(
    question: &Value,
    answer: &Value,
    model: &str,
    usage: &Value,
) -> Result<Value, JevProviderError> {
    let answer = match question["type"].as_str() {
        Some("noul") => json!({"type":"noul","noul":answer["noul"]}),
        Some("choice") => {
            json!({"type":"choice","choice":answer["choice"],"confidence":answer["confidence"],"probabilities":answer["probabilities"]})
        }
        Some("score") => {
            let criteria = question["criteria"]
                .as_array()
                .ok_or_else(|| request_error("Score criteria must be an array."))?;
            let legend: serde_json::Map<String, Value> = criteria
                .iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v.clone()))
                .collect();
            json!({"type":"score","score":answer["score"],"confidence":answer["confidence"],"probabilities":answer["probabilities"],"legend":legend})
        }
        _ => return Err(request_error("Invalid question type.")),
    };
    Ok(json!({"model":model,"answer":answer,"usage":{
        "input_tokens":usage["input_tokens"],"output_tokens":usage["output_tokens"]
    }}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, method},
    };

    fn question() -> Value {
        json!({"type":"noul","instructions":"Assess only supplied context"})
    }
    fn budget() -> RequestBudget {
        super::transport::budget(
            Instant::now() + Duration::from_secs(30),
            tokio_util::sync::CancellationToken::new(),
        )
    }
    #[test]
    fn reasoning_is_required_metadata_and_never_provider_evidence() {
        let mut query = json!({"reasoning":"  Decide whether to inspect the retry branch.  ","context":{"value":{"observation":true}},"question":question()});
        preflight(&query).expect("reasoning metadata accepted");
        assert_eq!(
            prepare(&query["context"]["value"], &query["question"], "m").unwrap(),
            json!({"model":"m","state":{"observation":true},"questions":{"answer":question()}})
        );
        for invalid in [Value::Null, json!(7), json!(""), json!(" \t\n")] {
            query["reasoning"] = invalid;
            assert!(preflight(&query).is_err());
        }
        query.as_object_mut().unwrap().remove("reasoning");
        assert!(preflight(&query).is_err());
    }

    #[test]
    fn one_question_and_explicit_context_replace_all_legacy_shapes() {
        for value in [
            Value::Null,
            json!([]),
            json!({"value":"one"}),
            json!("literal"),
        ] {
            assert!(preflight(&json!({"reasoning":"Decide the next evidence read.","context":{"value":value},"question":question()})).is_ok());
        }
        for tool in [
            "localFetch",
            "localSearch",
            "astSearch",
            "lspSearch",
            "ghSearch",
            "ghGetFileContent",
            "ghSearchHistory",
            "ghGetHistoryItem",
            "artifactSearch",
        ] {
            assert!(
                preflight(&json!({"reasoning":"Decide the next evidence read.","context":{"tool":tool,"query":{}},"question":question()}))
                    .is_ok()
            );
        }
        for tool in ["jev", "astRewrite", "ghCloneRepo", "unknown"] {
            assert!(
                preflight(&json!({"reasoning":"Decide the next evidence read.","context":{"tool":tool,"query":{}},"question":question()}))
                    .is_err()
            );
        }
        for invalid in [
            json!({"state":null,"questions":{"q":question()}}),
            json!({"reasoning":"Decide the next evidence read.","context":{"value":true},"question":question()}),
            json!({"reasoning":"Decide the next evidence read.","context":{"value":null,"tool":"localFetch","query":{}},"question":question()}),
            json!({"reasoning":"Decide the next evidence read.","context":{"value":null},"questions":{"q":question()}}),
        ] {
            assert!(preflight(&invalid).is_err());
        }
        let oversized = json!({"reasoning":"Decide the next evidence read.","context":{"value":"x".repeat(MAX_REQUEST_BYTES)},"question":question()});
        assert!(preflight(&oversized).is_err());
    }
    #[test]
    fn validates_each_primitive_and_provider_shape() {
        for (count, valid) in [(1, true), (255, true), (256, false)] {
            let criteria: serde_json::Map<String, Value> =
                (0..count).map(|i| (i.to_string(), Value::Null)).collect();
            assert_eq!(
                prepare(
                    &Value::Null,
                    &json!({"type":"choice","instructions":null,"criteria":criteria}),
                    "m"
                )
                .is_ok(),
                valid
            );
        }
        for (count, valid) in [(1, false), (2, true), (10, true), (11, false)] {
            assert_eq!(
                prepare(
                    &Value::Null,
                    &json!({"type":"score","instructions":null,"criteria":vec![Value::Null;count]}),
                    "m"
                )
                .is_ok(),
                valid
            );
        }
        for invalid in [
            json!({"type":"noul"}),
            json!({"type":"noul","instructions":true}),
            json!({"type":"noul","instructions":null,"criteria":{}}),
            json!({"type":"choice","instructions":null,"criteria":{"":null}}),
        ] {
            assert!(validate_question(&invalid).is_err());
        }
        assert_eq!(
            prepare(&json!({"x":1}), &question(), "m").unwrap(),
            json!({"model":"m","state":{"x":1},"questions":{"answer":question()}})
        );
    }
    #[tokio::test]
    async fn each_primitive_is_projected_without_provider_extras() {
        for (question, answer) in [
            (question(), json!({"type":"noul","noul":0.8})),
            (
                json!({"type":"choice","instructions":"Pick","criteria":{"a":"First","b":"Second"}}),
                json!({"type":"choice","choice":"a","confidence":0.8,"probabilities":{"a":0.8,"b":0.2}}),
            ),
            (
                json!({"type":"score","instructions":"Rate","criteria":[{"path":"/literal/criterion"},"Good"]}),
                json!({"type":"score","score":0.75,"confidence":0.8,"probabilities":{"0":0.25,"1":0.75},"legend":{"0":{"path":"/literal/criterion"},"1":"Good"}}),
            ),
        ] {
            let server = MockServer::start().await;
            let state = json!({"context":"HIDDEN_BODY"});
            let mut supplied = answer.clone();
            supplied["content"] = json!("HIDDEN_BODY");
            Mock::given(method("POST")).and(body_json(prepare(&state,&question,"m").unwrap()))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({"model":"HIDDEN_BODY","answers":{"answer":supplied},"content":"HIDDEN_BODY","usage":{"input_tokens":10,"output_tokens":1,"content":"HIDDEN_BODY"}})))
                .expect(1).mount(&server).await;
            let result = execute(
                &state,
                &question,
                SecretString::from("test-key"),
                &server.uri(),
                "m",
                budget(),
                0,
            )
            .await
            .unwrap();
            assert_eq!(
                result,
                json!({"model":"m","answer":answer,"usage":{"input_tokens":10,"output_tokens":1}})
            );
            assert!(!result.to_string().contains("HIDDEN_BODY"));
        }
    }
    #[tokio::test]
    async fn invalid_and_cancelled_requests_never_reach_provider() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&server)
            .await;
        assert!(
            execute(
                &Value::Null,
                &json!({}),
                SecretString::from("test-key"),
                &server.uri(),
                "m",
                budget(),
                0
            )
            .await
            .is_err()
        );
        let cancelled = budget();
        cancelled.cancellation.cancel();
        assert_eq!(
            execute(
                &Value::Null,
                &question(),
                SecretString::from("test-key"),
                &server.uri(),
                "m",
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
    async fn provider_answer_id_or_type_drift_is_rejected() {
        for answer in [
            json!({}),
            json!({"answer":{"type":"choice"}}),
            json!({"wrong":{"type":"noul","noul":0.8}}),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"model":"m","answers":answer,"usage":{"input_tokens":1,"output_tokens":1}}))).expect(1).mount(&server).await;
            assert_eq!(
                execute(
                    &Value::Null,
                    &question(),
                    SecretString::from("test-key"),
                    &server.uri(),
                    "m",
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
