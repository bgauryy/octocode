//! Provider batching for independently captured identical states in one request.
use super::{
    project, request_error,
    transport::{JevProviderError, check_budget, endpoint, post},
};
use crate::providers::RequestBudget;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Map, Value, json};

// Conservative UTF-8 headroom, not a model-token admission guarantee.
const MAX_STATE_AND_QUESTION_BYTES: usize = 24 * 1024;
const MAX_GROUP_BYTES: usize = 48 * 1024;

fn request(state: &Value, questions: &[(usize, &Value)], model: &str) -> Value {
    let questions: Map<String, Value> = questions
        .iter()
        .map(|(index, question)| (format!("answer_{index}"), (*question).clone()))
        .collect();
    json!({"model":model,"state":state,"questions":questions})
}

pub(crate) fn fits(state: &Value, questions: &[(usize, &Value)], model: &str) -> bool {
    questions.iter().all(|question| {
        request(state, std::slice::from_ref(question), model)
            .to_string()
            .len()
            <= MAX_STATE_AND_QUESTION_BYTES
    }) && request(state, questions, model).to_string().len() <= MAX_GROUP_BYTES
}

pub(crate) struct GroupResponse {
    pub answers: Vec<Result<Value, JevProviderError>>,
    pub usage: Value,
}

fn response_error(message: impl Into<String>) -> JevProviderError {
    JevProviderError {
        code: "invalidJevResponse".into(),
        message: message.into(),
        hints: vec!["Inspect provider compatibility before using the answer.".into()],
    }
}

pub(crate) async fn execute(
    state: &Value,
    questions: &[(usize, &Value)],
    key: &SecretString,
    base_url: &str,
    model: &str,
    budget: &RequestBudget,
    retries: u32,
) -> Result<GroupResponse, JevProviderError> {
    check_budget(budget)?;
    if !fits(state, questions, model) {
        return Err(request_error(
            "Shared Jev request exceeds the batching headroom policy.",
        ));
    }
    if key.expose_secret().chars().any(char::is_control) {
        return Err(JevProviderError {
            code: "invalidJevConfiguration".into(),
            message: "OCTOCODE_JEV_KEY contains invalid control characters.".into(),
            hints: vec!["Replace the configured key.".into()],
        });
    }
    let request = request(state, questions, model);
    let response = post(&request, key, endpoint(base_url)?, budget, retries).await?;
    project_response(&request, &response, questions, model)
}

fn project_response(
    request: &Value,
    response: &Value,
    questions: &[(usize, &Value)],
    model: &str,
) -> Result<GroupResponse, JevProviderError> {
    let answers = response["answers"]
        .as_object()
        .ok_or_else(|| response_error("response.answers must be an object"))?;
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
    octocode_engine::jev::validate_response(&metadata_request, &metadata_response)
        .map_err(|error| response_error(error.message))?;
    let projected = questions
        .iter()
        .map(|(index, question)| {
            let id = format!("answer_{index}");
            let mut row_request = request.clone();
            row_request["questions"] = json!({&id:*question});
            let mut row_response = metadata_response.clone();
            row_response["answers"] = answers
                .get(&id)
                .map_or_else(|| json!({}), |answer| json!({&id:answer}));
            octocode_engine::jev::validate_response(&row_request, &row_response)
                .map_err(|error| response_error(error.message))?;
            project(question, &answers[&id], model, &response["usage"])
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

    #[test]
    fn batching_headroom_falls_back_before_existing_singleton_limit() {
        let question = json!({"type":"noul","instructions":"Assess"});
        let questions = [(0, &question), (1, &question)];
        assert!(fits(&json!({"x":1}), &questions, "m"));
        assert!(!fits(
            &json!("x".repeat(MAX_STATE_AND_QUESTION_BYTES)),
            &questions,
            "m"
        ));
        let wide = json!({"type":"noul","instructions":"x".repeat(12*1024)});
        assert!(!fits(
            &Value::Null,
            &[(0, &wide), (1, &wide), (2, &wide), (3, &wide), (4, &wide)],
            "m"
        ));
    }

    #[test]
    fn malformed_or_missing_answer_is_isolated_but_unknown_id_fails_group() {
        let question = json!({"type":"noul","instructions":null});
        let questions = [(0, &question), (2, &question), (4, &question)];
        let request = request(&Value::Null, &questions, "m");
        let mut response = json!({"model":"provider-model","usage":{"input_tokens":9,"output_tokens":3},
            "answers":{"answer_0":{"type":"noul","noul":0.8},"answer_2":{"type":"noul","noul":2}}});
        let result = project_response(&request, &response, &questions, "m").unwrap();
        assert!(result.answers[0].is_ok());
        assert!(result.answers[1].is_err());
        assert!(result.answers[2].is_err());
        assert_eq!(result.usage["input_tokens"], 9);
        response["answers"]["unexpected"] = json!({"type":"noul","noul":0.5});
        assert!(project_response(&request, &response, &questions, "m").is_err());
    }
}
