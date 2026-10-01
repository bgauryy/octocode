//! Vendor-agnostic single-question classification: preflight validation,
//! provider request building, HTTP dispatch, and answer projection.
pub(crate) mod batch;
pub(crate) mod cache;
pub(crate) mod questions;
pub(crate) mod transport;

use self::transport::{ClassificationError, check_budget, endpoint, post};
use crate::providers::classification::gate::GateLease;
use crate::{
    providers::RequestBudget,
    tools::id::{ToolId, clasify_policy},
};
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

fn in_policy(tool: &str, set: &[ToolId]) -> bool {
    ToolId::from_name(tool).is_some_and(|id| set.contains(&id))
}

/// Read tools a clasify resource may delegate to (contract `scoutTools`).
pub(crate) fn is_context_tool(tool: &str) -> bool {
    in_policy(tool, clasify_policy::SCOUT_TOOLS)
}

/// Search tools that accept `candidateEvidence` (contract `candidateSearchTools`).
pub(crate) fn is_candidate_search_tool(tool: &str) -> bool {
    in_policy(tool, clasify_policy::CANDIDATE_SEARCH_TOOLS)
}

/// File reads that accept `prefilter` (contract `fileReadTools`).
pub(crate) fn is_file_read_tool(tool: &str) -> bool {
    in_policy(tool, clasify_policy::FILE_READ_TOOLS)
}

/// Contract `candidateEvidence` enum (generated wire type).
pub(crate) type CandidateEvidence =
    crate::contracts::tool_types::ClasifyInputVariant0ResourcesItemContextVariant1CandidateEvidence;

/// A resource context's parsed `candidateEvidence`, when present and known.
pub(crate) fn candidate_evidence(context: &Value) -> Option<CandidateEvidence> {
    context
        .get("candidateEvidence")
        .and_then(Value::as_str)
        .and_then(|value| value.parse().ok())
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

/// Whether a tool resource can yield the contiguous original-source page that
/// `locate` tags: an untransformed file read, or search hydrated with
/// `fileChunks` (a page that is still gapped fails per page at runtime).
/// Supplied values are left to the runtime page check.
fn locate_capable(context: &serde_json::Map<String, Value>) -> bool {
    let Some(tool) = context.get("tool").and_then(Value::as_str) else {
        return true;
    };
    if is_file_read_tool(tool) {
        return context
            .get("query")
            .and_then(|query| query.get("minify"))
            .and_then(Value::as_str)
            .is_none_or(|minify| minify == "none");
    }
    is_candidate_search_tool(tool)
        && context
            .get("candidateEvidence")
            .and_then(Value::as_str)
            .and_then(|value| value.parse().ok())
            == Some(CandidateEvidence::FileChunks)
}

/// Resolve a matrix's provider questions once, before capture, and reject
/// what the contract schema cannot express: `locate` over resources without
/// contiguous source lines.
///
/// Precondition: `query` passed `contracts::prepare_many_and_validate` (the
/// engine is clasify's only entry and validates every row; clasify never
/// mints cursors) and then `normalize_ids`. Shape, brief, id, context,
/// prefilter-tool, and cell-limit rules are therefore contract-owned and not
/// re-checked here.
pub(crate) fn preflight(query: &Value) -> Result<Vec<Value>, ClassificationError> {
    let resources = query["resources"]
        .as_array()
        .ok_or_else(|| request_error("Resources must be an array."))?;
    let questions = query["questions"]
        .as_array()
        .ok_or_else(|| request_error("Questions must be an array."))?;
    let mut resolved = Vec::with_capacity(questions.len());
    for question in questions {
        let expanded = questions::expand(&question["question"])?;
        if !questions::is_locate(&expanded) {
            validate_question(&expanded)?;
        }
        resolved.push(json!({"id":question["id"],"question":expanded}));
    }
    if resolved
        .iter()
        .any(|question| questions::is_locate(&question["question"]))
    {
        let blocked = resources
            .iter()
            .filter(|resource| {
                resource["context"]
                    .as_object()
                    .is_some_and(|context| !locate_capable(context))
            })
            .filter_map(|resource| resource["id"].as_str())
            .collect::<Vec<_>>();
        if !blocked.is_empty() {
            return Err(ClassificationError {
                code: "classificationLocateUnsupported".into(),
                message: format!(
                    "locate needs contiguous original source lines; resources {} cannot supply them.",
                    blocked.join(", ")
                ),
                hints: vec![
                    "For locate, use localFetch or ghGetFileContent without minify, or localSearch/ghSearchCode with candidateEvidence:\"fileChunks\".".into(),
                    "To screen search, structure, AST, LSP, history, or package results, ask noul, choice, score, or contribution questions in a separate matrix.".into(),
                ],
                ..Default::default()
            });
        }
    }
    Ok(resolved)
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
            "goal":"Searching for retry handling. Need files that decide a retry.",
            "resources":[{"id":"resource-1","context":context}],
            "questions":[{"id":"relevance.v1","question":question}]
        })
    }
    /// Clasify's engine path: contract validation of the public shape (flat
    /// questions), `normalize_ids`' internal `{id, question}` rows, then preflight.
    fn admitted(query: &Value) -> Result<Vec<Value>, ClassificationError> {
        let mut raw = query.clone();
        for row in raw["questions"].as_array_mut().expect("questions") {
            let mut flat = row["question"].clone();
            if let Some(id) = row.get("id") {
                flat["id"] = id.clone();
            }
            *row = flat;
        }
        let mut validated = crate::contracts::prepare_and_validate(
            "clasify",
            raw,
            crate::contracts::PrepareOptions::default(),
        )
        .map_err(|error| request_error(&format!("{error:?}")))?;
        for (index, row) in validated["questions"]
            .as_array_mut()
            .expect("questions")
            .iter_mut()
            .enumerate()
        {
            let mut question = row.clone();
            let id = question
                .as_object_mut()
                .and_then(|question| question.remove("id"))
                .unwrap_or_else(|| json!(format!("question-{}", index + 1)));
            *row = json!({"id":id,"question":question});
        }
        preflight(&validated)
    }

    #[test]
    fn preflight_accepts_a_continuation_that_carries_the_running_best() {
        let mut query = semantic_query(json!({"value":"x"}), question());
        query["carry"] = json!({"t":[{"resourceId":"resource-1","exists":0.9,"startLine":1,"endLine":8,"probability":0.5}]});
        assert!(admitted(&query).is_ok());
        query["carry"] = json!("not a map");
        assert!(admitted(&query).is_err());
        query.as_object_mut().unwrap().remove("carry");
        query["unexpected"] = json!(1);
        assert!(admitted(&query).is_err());
    }

    #[test]
    fn preflight_rejects_prefilter_outside_file_reads() {
        let search = json!({"tool":"localSearch","query":{"path":"/repo","searchText":"retry"}});
        let mut query = semantic_query(search, question());
        query["resources"][0]["prefilter"] = json!(["retry"]);
        // Contract validation owns the rule (same stage and wording as core).
        let error = admitted(&query).expect_err("search resources cannot prefilter");
        assert!(
            error
                .message
                .contains("prefilter applies only to localFetch or ghGetFileContent"),
            "{}",
            error.message
        );
        let read = json!({"tool":"localFetch","query":{"path":"/repo/a.rs"}});
        let mut query = semantic_query(read, question());
        query["resources"][0]["prefilter"] = json!(["retry"]);
        assert!(admitted(&query).is_ok());
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
    fn reasoning_and_goal_are_required_briefs_and_stay_off_the_expanded_question() {
        let provider = jev_provider();
        let mut query = semantic_query(json!({"value":{"observation":true}}), question());
        let resolved = admitted(&query).expect("required briefs accepted");
        assert!(resolved[0]["question"].get("goal").is_none());
        assert!(resolved[0]["question"].get("reasoning").is_none());
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
        query["carry"] = json!({"t":[{"resourceId":"resource-1","exists":0.9,"startLine":1,"endLine":8,"probability":0.5}]});
        assert!(admitted(&query).is_ok());
        for field in ["reasoning", "goal"] {
            let mut missing = query.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(admitted(&missing).is_err(), "missing {field}");
            for invalid in [
                Value::Null,
                json!(7),
                json!(""),
                json!(" \t\n"),
                json!("x".repeat(501)),
            ] {
                let mut bad = query.clone();
                bad[field] = invalid.clone();
                assert!(admitted(&bad).is_err(), "{field} {invalid}");
            }
        }
    }

    #[test]
    fn correlation_ids_are_validated_but_never_sent_to_the_provider() {
        let provider = jev_provider();
        let query = semantic_query(json!({"value":{"observation":true}}), question());
        let resolved = admitted(&query).expect("valid correlation IDs");
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
            assert!(admitted(&invalid).is_err());
        }
        let mut missing = query;
        missing["questions"][0]
            .as_object_mut()
            .unwrap()
            .remove("id");
        let derived = admitted(&missing).expect("omitted ids are derived after validation");
        assert_eq!(derived[0]["id"], "question-1");
    }

    #[test]
    fn one_question_and_explicit_context_replace_all_legacy_shapes() {
        for value in [json!(["one"]), json!({"value":"one"}), json!("literal")] {
            assert!(admitted(&semantic_query(json!({"value":value}), question())).is_ok());
        }
        for tool in [
            "localFetch",
            "localSearch",
            "structureSearch",
            "astSearch",
            "astTopology",
            "lspSearch",
            "ghSearchRepo",
            "ghSearchCode",
            "ghStructure",
            "ghGetFileContent",
            "ghSearchHistory",
            "ghGetHistoryItem",
            "artifactSearch",
        ] {
            assert!(admitted(&semantic_query(json!({"tool":tool,"query":{}}), question())).is_ok());
        }
        for tool in ["jev", "clasify", "astRewrite", "ghCloneRepo", "unknown"] {
            assert!(
                admitted(&semantic_query(json!({"tool":tool,"query":{}}), question())).is_err()
            );
        }
        for context in [
            json!({"tool":"localSearch","query":{},"candidateEvidence":"search"}),
            json!({"tool":"localSearch","query":{},"candidateEvidence":"fileChunks"}),
            json!({"tool":"ghSearchCode","query":{},"candidateEvidence":"search"}),
            json!({"tool":"ghSearchCode","query":{},"candidateEvidence":"fileChunks"}),
        ] {
            assert!(admitted(&semantic_query(context, question())).is_ok());
        }
        for context in [
            json!({"tool":"localFetch","query":{},"candidateEvidence":"fileChunks"}),
            json!({"tool":"localFetch","query":{},"candidateEvidence":"search"}),
            json!({"tool":"ghStructure","query":{},"candidateEvidence":"search"}),
            json!({"tool":"ghSearchRepo","query":{},"candidateEvidence":"fileChunks"}),
            json!({"tool":"localSearch","query":{},"candidateEvidence":"unknown"}),
        ] {
            assert!(admitted(&semantic_query(context, question())).is_err());
        }
        for invalid in [
            semantic_query(json!({"value":true}), question()),
            semantic_query(
                json!({"value":null,"tool":"localFetch","query":{}}),
                question(),
            ),
            semantic_query(json!({"value":null}), question()),
        ] {
            assert!(admitted(&invalid).is_err());
        }
    }

    #[test]
    fn locate_is_rejected_before_capture_for_resources_without_source_lines() {
        let locate = json!({"questionType":"locate","target":"retry condition"});
        for context in [
            json!({"tool":"localFetch","query":{}}),
            json!({"tool":"localFetch","query":{"minify":"none"}}),
            json!({"tool":"ghGetFileContent","query":{}}),
            json!({"tool":"localSearch","query":{},"candidateEvidence":"fileChunks"}),
            json!({"tool":"ghSearchCode","query":{},"candidateEvidence":"fileChunks"}),
            json!({"value":"supplied"}),
        ] {
            assert!(admitted(&semantic_query(context, locate.clone())).is_ok());
        }
        for context in [
            json!({"tool":"localFetch","query":{"minify":"standard"}}),
            json!({"tool":"ghGetFileContent","query":{"minify":"symbols"}}),
            json!({"tool":"localSearch","query":{}}),
            json!({"tool":"ghSearchCode","query":{},"candidateEvidence":"search"}),
            json!({"tool":"structureSearch","query":{}}),
            json!({"tool":"astSearch","query":{}}),
            json!({"tool":"astTopology","query":{}}),
            json!({"tool":"lspSearch","query":{}}),
            json!({"tool":"ghSearchRepo","query":{}}),
            json!({"tool":"ghStructure","query":{}}),
            json!({"tool":"ghSearchHistory","query":{}}),
            json!({"tool":"ghGetHistoryItem","query":{}}),
            json!({"tool":"artifactSearch","query":{}}),
        ] {
            let error = admitted(&semantic_query(context.clone(), locate.clone()))
                .expect_err("locate needs source lines");
            assert_eq!(error.code, "classificationLocateUnsupported", "{context}");
            assert!(error.message.contains("resource-1"));
            assert!(admitted(&semantic_query(context, question())).is_ok());
        }
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
