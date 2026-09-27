//! Vendor-agnostic single-question classification: preflight validation,
//! provider request building, HTTP dispatch, and answer projection.
pub(crate) mod batch;
pub(crate) mod questions;
pub(crate) mod transport;

use self::transport::{ClassificationError, check_budget, endpoint, post};
use crate::providers::classification::gate::GateLease;
use crate::{providers::RequestBudget, tools::id::ToolId};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};

const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;

fn request_error(message: &str) -> ClassificationError {
    ClassificationError {
        code: "invalidClassificationRequest".into(),
        message: message.into(),
        hints: vec!["Inspect the current clasify query schema.".into()],
        ..Default::default()
    }
}

fn nullable_entry(value: &Value) -> bool {
    matches!(
        value,
        Value::Null | Value::String(_) | Value::Array(_) | Value::Object(_)
    )
}

fn entry(value: &Value) -> bool {
    !value.is_null() && nullable_entry(value)
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
                | ToolId::StructureSearch
                | ToolId::AstSearch
                | ToolId::AstTopology
                | ToolId::LspSearch
        )
    )
}

/// Context tools whose continuations move within one document. Search and
/// discovery continuations reach new candidates instead, so clasify captures
/// only the requested page and returns the rest through `next.clasify`.
pub(crate) fn pages_within_resource(tool: &str) -> bool {
    matches!(
        ToolId::from_name(tool),
        Some(ToolId::LocalFetch | ToolId::GhGetFileContent | ToolId::GhGetHistoryItem)
    )
}

/// Validate the matrix and resolve its provider questions once, before capture.
pub(crate) fn preflight(query: &Value) -> Result<Vec<Value>, ClassificationError> {
    if query.to_string().len() > MAX_REQUEST_BYTES {
        return Err(request_error(
            "Classification request exceeded the 4 MiB limit.",
        ));
    }
    let query = query
        .as_object()
        .ok_or_else(|| request_error("Query must be an object."))?;
    // `carry` (the running locate ranking from next.clasify) is optional.
    let expected_keys = if query.contains_key("carry") { 5 } else { 4 };
    if query.len() != expected_keys
        || query.get("carry").is_some_and(|carry| !carry.is_object())
        || !query.get("id").is_some_and(valid_matrix_id)
        || !query.get("resources").is_some_and(Value::is_array)
        || !query.get("questions").is_some_and(Value::is_array)
        || query
            .get("reasoning")
            .and_then(Value::as_str)
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err(request_error(
            "Supply an id, nonblank reasoning, resources, and typed questions.",
        ));
    }
    let resources = query["resources"]
        .as_array()
        .ok_or_else(|| request_error("Resources must be an array."))?;
    let questions = query["questions"]
        .as_array()
        .ok_or_else(|| request_error("Questions must be an array."))?;
    if resources.is_empty()
        || questions.is_empty()
        || resources.len().saturating_mul(questions.len()) > 25
    {
        return Err(request_error(
            "Supply 1–25 resources/questions with at most 25 matrix cells.",
        ));
    }
    for resource in resources {
        if !resource.get("id").is_some_and(valid_matrix_id) {
            return Err(request_error(
                "Every resource requires a valid correlation id.",
            ));
        }
        let context = resource
            .get("context")
            .and_then(Value::as_object)
            .ok_or_else(|| request_error("Every resource requires context."))?;
        let value_context = context.len() == 1 && context.get("value").is_some_and(entry);
        let tool = context.get("tool").and_then(Value::as_str);
        let candidate_evidence = context.get("candidateEvidence").and_then(Value::as_str);
        let candidate_search = match tool {
            Some("localSearch") => true,
            Some("ghSearch") => {
                context
                    .get("query")
                    .and_then(|query| query.get("operation"))
                    .and_then(Value::as_str)
                    == Some("code")
            }
            _ => false,
        };
        let candidate_evidence_valid = match candidate_evidence {
            None => true,
            Some("search" | "fileChunks") => candidate_search,
            Some(_) => false,
        };
        let tool_context = context.len() == 2 + usize::from(candidate_evidence.is_some())
            && tool.is_some_and(is_context_tool)
            && context.get("query").is_some_and(Value::is_object)
            && candidate_evidence_valid;
        if !value_context && !tool_context {
            return Err(request_error(
                "Resource context requires a non-empty value, or an allowed read tool with one ordinary query.",
            ));
        }
    }
    let mut resolved = Vec::with_capacity(questions.len());
    for question in questions {
        if !question.get("id").is_some_and(valid_matrix_id) {
            return Err(request_error(
                "Every question requires a valid correlation id.",
            ));
        }
        let expanded = questions::expand(&question["question"])?;
        if !questions::is_locate(&expanded) {
            validate_question(&expanded)?;
        }
        resolved.push(json!({"id":question["id"],"question":expanded}));
    }
    Ok(resolved)
}

fn valid_matrix_id(value: &Value) -> bool {
    let Some(value) = value.as_str() else {
        return false;
    };
    let bytes = value.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn validate_question(question: &Value) -> Result<(), ClassificationError> {
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
                        && criteria.values().all(nullable_entry)
                })
        }),
        Some("choice") => criteria.and_then(Value::as_object).is_some_and(|criteria| {
            (2..=255).contains(&criteria.len())
                && criteria.keys().all(|key| !key.is_empty())
                && criteria.values().all(nullable_entry)
        }),
        Some("score") => criteria.and_then(Value::as_array).is_some_and(|criteria| {
            (2..=10).contains(&criteria.len()) && criteria.iter().all(entry)
        }),
        _ => false,
    };
    if !valid {
        return Err(request_error(
            "Use noul with optional true/false criteria, choice with 2..255 labeled criteria, or score with 2..10 ordered criteria.",
        ));
    }
    Ok(())
}

/// Build and validate the provider request for one state × question cell.
/// Delegates wire format to the vendor's [`ClassificationProvider::build_request`].
fn prepare(
    state: &Value,
    question: &Value,
    model: &str,
    provider: &dyn crate::providers::classification::ClassificationProvider,
) -> Result<Value, ClassificationError> {
    if !entry(state) {
        return Err(request_error(
            "Context value must be a non-empty string, object, or array.",
        ));
    }
    validate_question(question)?;
    let request = provider.build_request(state, question, model);
    if request.to_string().len() > MAX_REQUEST_BYTES {
        return Err(request_error(
            "Classification request exceeded the 4 MiB limit.",
        ));
    }
    Ok(request)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute(
    state: &Value,
    question: &Value,
    key: SecretString,
    base_url: &str,
    endpoint_path: &str,
    model: &str,
    provider: &dyn crate::providers::classification::ClassificationProvider,
    budget: RequestBudget,
    retries: u32,
    gate: &GateLease,
) -> Result<Value, ClassificationError> {
    check_budget(&budget)?;
    let request = prepare(state, question, model, provider)?;
    if key.expose_secret().chars().any(char::is_control) {
        return Err(ClassificationError {
            code: "invalidClassificationConfiguration".into(),
            message: "OCTOCODE_CLASSIFICATION_API contains invalid control characters.".into(),
            hints: vec!["Replace the configured key.".into()],
            ..Default::default()
        });
    }
    let response = post(
        &request,
        &key,
        endpoint(base_url, endpoint_path)?,
        &budget,
        retries,
        gate,
    )
    .await?;
    provider
        .validate_response(&request, &response)
        .map_err(|error| ClassificationError {
            code: error.code().into(),
            message: error.message,
            hints: vec!["Inspect provider compatibility before using the answer.".into()],
            ..Default::default()
        })?;
    let answer = provider
        .extract_answer(&response)
        .ok_or_else(|| ClassificationError {
            code: "invalidClassificationResponse".into(),
            message: "Classification provider response is missing the expected answer.".into(),
            hints: vec!["Inspect provider compatibility before using the response.".into()],
            ..Default::default()
        })?;
    project(
        question,
        answer,
        model,
        response["model"].as_str().unwrap_or(model),
        &response["usage"],
    )
}

fn project(
    question: &Value,
    answer: &Value,
    requested_model: &str,
    resolved_model: &str,
    usage: &Value,
) -> Result<Value, ClassificationError> {
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
    Ok(
        json!({"requestedModel":requested_model,"resolvedModel":resolved_model,"answer":answer,"usage":{
            "input_tokens":usage["input_tokens"],"output_tokens":usage["output_tokens"]
        }}),
    )
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
    fn semantic_query(context: Value, question: Value) -> Value {
        json!({
            "id":"decision",
            "reasoning":"Decide whether to inspect the retry branch.",
            "resources":[{"id":"resource-1","context":context}],
            "questions":[{"id":"relevance.v1","question":question}]
        })
    }
    #[test]
    fn preflight_accepts_a_continuation_that_carries_the_running_best() {
        let mut query = semantic_query(json!({"value":"x"}), question());
        query["carry"] = json!({"t":[{"resourceId":"resource-1","exists":0.9,"startLine":1,"endLine":8,"probability":0.5}]});
        assert!(preflight(&query).is_ok());
        query["carry"] = json!("not a map");
        assert!(preflight(&query).is_err());
        query.as_object_mut().unwrap().remove("carry");
        query["unexpected"] = json!(1);
        assert!(preflight(&query).is_err());
    }

    fn budget() -> RequestBudget {
        super::transport::budget(
            Instant::now() + Duration::from_secs(30),
            tokio_util::sync::CancellationToken::new(),
        )
    }

    fn jev_provider() -> &'static dyn crate::providers::classification::ClassificationProvider {
        &crate::providers::classification::jev::JEV
    }

    fn test_gate() -> GateLease {
        crate::providers::classification::gate::lease("test://clasify-mod", 64)
    }

    #[test]
    fn reasoning_is_required_metadata_and_never_provider_evidence() {
        let provider = jev_provider();
        let mut query = semantic_query(json!({"value":{"observation":true}}), question());
        let resolved = preflight(&query).expect("reasoning metadata accepted");
        assert_eq!(
            prepare(
                &query["resources"][0]["context"]["value"],
                &resolved[0]["question"],
                "m",
                provider,
            )
            .unwrap(),
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
    fn correlation_ids_are_validated_but_never_sent_to_the_provider() {
        let provider = jev_provider();
        let query = semantic_query(json!({"value":{"observation":true}}), question());
        let resolved = preflight(&query).expect("valid correlation IDs");
        assert_eq!(resolved[0]["id"], query["questions"][0]["id"]);
        assert_eq!(
            prepare(
                &query["resources"][0]["context"]["value"],
                &resolved[0]["question"],
                "m",
                provider,
            )
            .unwrap(),
            json!({"model":"m","state":{"observation":true},"questions":{"answer":question()}})
        );
        let mut null_id = query.clone();
        null_id["questions"][0]["id"] = Value::Null;
        let mut invalid_id = query.clone();
        invalid_id["resources"][0]["id"] = json!("bad id");
        for invalid in [null_id, invalid_id] {
            assert!(preflight(&invalid).is_err());
        }
        let mut missing = query;
        missing["questions"][0]
            .as_object_mut()
            .unwrap()
            .remove("id");
        assert!(preflight(&missing).is_err());
    }

    #[test]
    fn one_question_and_explicit_context_replace_all_legacy_shapes() {
        for value in [json!(["one"]), json!({"value":"one"}), json!("literal")] {
            assert!(preflight(&semantic_query(json!({"value":value}), question())).is_ok());
        }
        for tool in [
            "localFetch",
            "localSearch",
            "structureSearch",
            "astSearch",
            "astTopology",
            "lspSearch",
            "ghSearch",
            "ghGetFileContent",
            "ghSearchHistory",
            "ghGetHistoryItem",
            "artifactSearch",
        ] {
            assert!(
                preflight(&semantic_query(json!({"tool":tool,"query":{}}), question())).is_ok()
            );
        }
        for tool in ["jev", "clasify", "astRewrite", "ghCloneRepo", "unknown"] {
            assert!(
                preflight(&semantic_query(json!({"tool":tool,"query":{}}), question())).is_err()
            );
        }
        for context in [
            json!({"tool":"localSearch","query":{},"candidateEvidence":"search"}),
            json!({"tool":"localSearch","query":{},"candidateEvidence":"fileChunks"}),
            json!({"tool":"ghSearch","query":{"operation":"code"},"candidateEvidence":"search"}),
            json!({"tool":"ghSearch","query":{"operation":"code"},"candidateEvidence":"fileChunks"}),
        ] {
            assert!(preflight(&semantic_query(context, question())).is_ok());
        }
        for context in [
            json!({"tool":"localFetch","query":{},"candidateEvidence":"fileChunks"}),
            json!({"tool":"localFetch","query":{},"candidateEvidence":"search"}),
            json!({"tool":"ghSearch","query":{"operation":"tree"},"candidateEvidence":"search"}),
            json!({"tool":"ghSearch","query":{"operation":"tree"},"candidateEvidence":"fileChunks"}),
            json!({"tool":"localSearch","query":{},"candidateEvidence":"unknown"}),
        ] {
            assert!(preflight(&semantic_query(context, question())).is_err());
        }
        for invalid in [
            semantic_query(json!({"value":true}), question()),
            semantic_query(
                json!({"value":null,"tool":"localFetch","query":{}}),
                question(),
            ),
            semantic_query(json!({"value":null}), question()),
        ] {
            assert!(preflight(&invalid).is_err());
        }
        let oversized = semantic_query(json!({"value":"x".repeat(MAX_REQUEST_BYTES)}), question());
        assert!(preflight(&oversized).is_err());
    }

    #[test]
    fn validates_each_primitive_and_provider_shape() {
        let provider = jev_provider();
        for (count, valid) in [(1, false), (2, true), (255, true), (256, false)] {
            let criteria: serde_json::Map<String, Value> =
                (0..count).map(|i| (i.to_string(), Value::Null)).collect();
            assert_eq!(
                prepare(
                    &json!({"state":true}),
                    &json!({"type":"choice","instructions":"Pick","criteria":criteria}),
                    "m",
                    provider,
                )
                .is_ok(),
                valid
            );
        }
        for (count, valid) in [(1, false), (2, true), (10, true), (11, false)] {
            assert_eq!(
                prepare(
                    &json!({"state":true}),
                    &json!({"type":"score","instructions":"Rate","criteria":vec![json!("level");count]}),
                    "m",
                    provider,
                )
                .is_ok(),
                valid
            );
        }
        for invalid in [
            json!({"type":"noul"}),
            json!({"type":"noul","instructions":true}),
            json!({"type":"noul","instructions":null,"criteria":{}}),
            json!({"type":"choice","instructions":"Pick","criteria":{"":null,"b":null}}),
        ] {
            assert!(validate_question(&invalid).is_err());
        }
        assert_eq!(
            validate_question(&json!({
                "type":"choice",
                "instructions":"Pick",
                "criteria":{"only":null}
            }))
            .expect_err("one choice is invalid")
            .message,
            "Use noul with optional true/false criteria, choice with 2..255 labeled criteria, or score with 2..10 ordered criteria."
        );
        assert_eq!(
            prepare(&Value::Null, &question(), "m", provider)
                .expect_err("null context is invalid")
                .message,
            "Context value must be a non-empty string, object, or array."
        );
        assert_eq!(
            prepare(&json!({"x":1}), &question(), "m", provider).unwrap(),
            json!({"model":"m","state":{"x":1},"questions":{"answer":question()}})
        );
    }

    #[tokio::test]
    async fn each_primitive_is_projected_without_provider_extras() {
        let provider = jev_provider();
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
            Mock::given(method("POST")).and(body_json(prepare(&state, &question, "m", provider).unwrap()))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({"model":"provider-model","answers":{"answer":supplied},"content":"HIDDEN_BODY","usage":{"input_tokens":10,"output_tokens":1,"content":"HIDDEN_BODY"}})))
                .expect(1).mount(&server).await;
            let result = execute(
                &state,
                &question,
                SecretString::from("test-key"),
                &server.uri(),
                "v1/systemone",
                "m",
                provider,
                budget(),
                0,
                &test_gate(),
            )
            .await
            .unwrap();
            assert_eq!(
                result,
                json!({"requestedModel":"m","resolvedModel":"provider-model","answer":answer,"usage":{"input_tokens":10,"output_tokens":1}})
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
                &json!({"state":true}),
                &json!({}),
                SecretString::from("test-key"),
                &server.uri(),
                "v1/systemone",
                "m",
                jev_provider(),
                budget(),
                0,
                &test_gate(),
            )
            .await
            .is_err()
        );
        let cancelled = budget();
        cancelled.cancellation.cancel();
        assert_eq!(
            execute(
                &json!({"state":true}),
                &question(),
                SecretString::from("test-key"),
                &server.uri(),
                "v1/systemone",
                "m",
                jev_provider(),
                cancelled,
                0,
                &test_gate(),
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
                    &json!({"state":true}),
                    &question(),
                    SecretString::from("test-key"),
                    &server.uri(),
                    "v1/systemone",
                    "m",
                    jev_provider(),
                    budget(),
                    0,
                    &test_gate(),
                )
                .await
                .unwrap_err()
                .code,
                "invalidClassificationResponse"
            );
        }
    }
}
