//! Vendor-agnostic batch scheduling: group N questions over the same state
//! into one provider request when size limits allow, reducing round-trips.
//! The per-vendor wire format is delegated to [`ClassificationProvider`].
use super::{
    project, request_error,
    transport::{ClassificationError, check_budget, endpoint, post},
};
use crate::providers::RequestBudget;
use crate::providers::classification::gate::GateLease;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};

// Conservative serialized-byte headroom for shared-state requests. This is
// not a token estimate or a guarantee that a provider accepts the request.
const MAX_STATE_AND_QUESTION_BYTES: usize = 72 * 1024;
const MAX_GROUP_BYTES: usize = 120 * 1024;

fn request(
    state: &Value,
    questions: &[(usize, &Value)],
    model: &str,
    provider: &dyn crate::providers::classification::ClassificationProvider,
) -> Value {
    provider.build_batch_request(state, questions, model)
}

pub(crate) fn fits(
    state: &Value,
    questions: &[(usize, &Value)],
    model: &str,
    provider: &dyn crate::providers::classification::ClassificationProvider,
) -> bool {
    questions.iter().all(|question| {
        request(state, std::slice::from_ref(question), model, provider)
            .to_string()
            .len()
            <= MAX_STATE_AND_QUESTION_BYTES
    }) && request(state, questions, model, provider).to_string().len() <= MAX_GROUP_BYTES
}

pub(crate) struct GroupResponse {
    pub answers: Vec<Result<Value, ClassificationError>>,
    pub usage: Value,
}

fn response_error(message: impl Into<String>) -> ClassificationError {
    ClassificationError {
        code: "invalidClassificationResponse".into(),
        message: message.into(),
        hints: vec!["Inspect provider compatibility before using the answer.".into()],
        ..Default::default()
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute(
    state: &Value,
    questions: &[(usize, &Value)],
    key: &SecretString,
    base_url: &str,
    endpoint_path: &str,
    model: &str,
    provider: &dyn crate::providers::classification::ClassificationProvider,
    budget: &RequestBudget,
    retries: u32,
    gate: &GateLease,
) -> Result<GroupResponse, ClassificationError> {
    check_budget(budget)?;
    if !fits(state, questions, model, provider) {
        return Err(request_error(
            "Shared classification request exceeds the batching headroom policy.",
        ));
    }
    if key.expose_secret().chars().any(char::is_control) {
        return Err(ClassificationError {
            code: "invalidClassificationConfiguration".into(),
            message: "OCTOCODE_CLASSIFICATION_API contains invalid control characters.".into(),
            hints: vec!["Replace the configured key.".into()],
            ..Default::default()
        });
    }
    let req = request(state, questions, model, provider);
    let (response, provider_calls) = post(
        &req,
        key,
        endpoint(base_url, endpoint_path)?,
        budget,
        retries,
        gate,
    )
    .await?;
    let mut group =
        project_response(&req, &response, questions, model, provider).map_err(|mut error| {
            error.provider_calls = provider_calls;
            error
        })?;
    group.usage["provider_calls"] = json!(provider_calls);
    Ok(group)
}

fn project_response(
    request: &Value,
    response: &Value,
    questions: &[(usize, &Value)],
    model: &str,
    provider: &dyn crate::providers::classification::ClassificationProvider,
) -> Result<GroupResponse, ClassificationError> {
    let answers = response["answers"]
        .as_object()
        .ok_or_else(|| response_error("response.answers must be an object"))?;
    // All answer IDs in the response must correspond to requested questions.
    if answers
        .keys()
        .any(|id| request["questions"].get(id).is_none())
    {
        return Err(response_error("Response contains unexpected answer IDs."));
    }
    // Validate shared metadata without letting one malformed answer poison siblings.
    let metadata_request = json!({"questions":{}});
    let mut metadata_response = response.clone();
    metadata_response["answers"] = json!({});
    provider
        .validate_response(&metadata_request, &metadata_response)
        .map_err(|error| response_error(error.message))?;
    let projected = questions
        .iter()
        .map(|(index, question)| {
            // Use the vendor's key naming convention (e.g. "answer_0" for Jev).
            let id = provider.batch_answer_key(*index);
            let mut row_request = request.clone();
            row_request["questions"] = json!({&id:*question});
            let mut row_response = metadata_response.clone();
            row_response["answers"] = answers
                .get(&id)
                .map_or_else(|| json!({}), |answer| json!({&id:answer}));
            provider
                .validate_response(&row_request, &row_response)
                .map_err(|error| response_error(error.message))?;
            project(
                question,
                &answers[&id],
                model,
                response["model"].as_str().unwrap_or(model),
                &response["usage"],
            )
        })
        .collect();
    Ok(GroupResponse {
        answers: projected,
        usage: response["usage"].clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jev_provider() -> &'static dyn crate::providers::classification::ClassificationProvider {
        &crate::providers::classification::jev::JEV
    }

    #[test]
    fn batching_headroom_falls_back_before_existing_singleton_limit() {
        let provider = jev_provider();
        let question = json!({"type":"noul","instructions":"Assess"});
        let questions = [(0, &question), (1, &question)];
        assert!(fits(&json!({"x":1}), &questions, "m", provider));
        assert!(!fits(
            &json!("x".repeat(MAX_STATE_AND_QUESTION_BYTES)),
            &questions,
            "m",
            provider,
        ));
        let wide = json!({"type":"noul","instructions":"x".repeat(30*1024)});
        assert!(!fits(
            &Value::Null,
            &[(0, &wide), (1, &wide), (2, &wide), (3, &wide), (4, &wide)],
            "m",
            provider,
        ));
    }

    #[test]
    fn malformed_or_missing_answer_is_isolated_but_unknown_id_fails_group() {
        let provider = jev_provider();
        let question = json!({"type":"noul","instructions":null});
        let questions = [(0, &question), (2, &question), (4, &question)];
        let req = request(&Value::Null, &questions, "m", provider);
        let mut response = json!({"model":"provider-model","usage":{"input_tokens":9,"output_tokens":3},
            "answers":{"answer_0":{"type":"noul","noul":0.8},"answer_2":{"type":"noul","noul":2}}});
        let result = project_response(&req, &response, &questions, "m", provider).unwrap();
        assert!(result.answers[0].is_ok());
        assert!(result.answers[1].is_err());
        assert!(result.answers[2].is_err());
        assert_eq!(result.usage["input_tokens"], 9);
        response["answers"]["unexpected"] = json!({"type":"noul","noul":0.5});
        assert!(project_response(&req, &response, &questions, "m", provider).is_err());
    }
}
