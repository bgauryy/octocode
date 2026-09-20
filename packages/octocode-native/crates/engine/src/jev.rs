//! Transport-neutral validation of pure TypeSafe System One responses.

use serde_json::{Map, Value};
use std::fmt::{Display, Formatter};

const PROBABILITY_TOLERANCE: f64 = 0.02;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JevError {
    pub code: &'static str,
    pub message: String,
}

impl JevError {
    fn request(message: impl Into<String>) -> Self {
        Self {
            code: "invalidJevRequest",
            message: message.into(),
        }
    }

    fn response(message: impl Into<String>) -> Self {
        Self {
            code: "invalidJevResponse",
            message: message.into(),
        }
    }
}

impl Display for JevError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for JevError {}

fn object<'a>(value: &'a Value, field: &str) -> Result<&'a Map<String, Value>, JevError> {
    value
        .as_object()
        .ok_or_else(|| JevError::request(format!("{field} must be an object")))
}

fn probability(value: &Value, field: &str) -> Result<f64, JevError> {
    value
        .as_f64()
        .filter(|number| number.is_finite() && (0.0..=1.0).contains(number))
        .ok_or_else(|| JevError::response(format!("{field} must be between 0 and 1")))
}

fn validate_distribution(
    answer: &Map<String, Value>,
    criteria: &Map<String, Value>,
    field: &str,
) -> Result<(), JevError> {
    let probabilities = answer
        .get("probabilities")
        .unwrap_or(&Value::Null)
        .as_object()
        .ok_or_else(|| JevError::response(format!("{field}.probabilities must be an object")))?;
    if probabilities.len() != criteria.len()
        || !criteria
            .keys()
            .all(|label| probabilities.contains_key(label))
    {
        return Err(JevError::response(format!(
            "{field} probability labels differ from the request"
        )));
    }
    let mut sum = 0.0;
    for (label, value) in probabilities {
        sum += probability(value, &format!("{field}.probabilities.{label}"))?;
    }
    if (sum - 1.0).abs() > PROBABILITY_TOLERANCE {
        return Err(JevError::response(format!(
            "{field} probabilities do not sum to 1"
        )));
    }
    probability(
        answer.get("confidence").unwrap_or(&Value::Null),
        &format!("{field}.confidence"),
    )?;
    Ok(())
}

/// Rejects malformed or request-incompatible System One responses.
pub fn validate_response(request: &Value, response: &Value) -> Result<(), JevError> {
    let request = object(request, "request")?;
    if !response.is_object() {
        return Err(JevError::response("response must be an object"));
    }
    response["model"]
        .as_str()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .ok_or_else(|| JevError::response("response.model must be non-blank"))?;
    let usage = response["usage"]
        .as_object()
        .ok_or_else(|| JevError::response("response.usage must be an object"))?;
    for field in ["input_tokens", "output_tokens"] {
        if usage.get(field).and_then(Value::as_u64).is_none() {
            return Err(JevError::response(format!(
                "response.usage.{field} must be a non-negative integer"
            )));
        }
    }
    let questions = request
        .get("questions")
        .and_then(Value::as_object)
        .ok_or_else(|| JevError::request("request.questions must be an object"))?;
    let answers = response["answers"]
        .as_object()
        .ok_or_else(|| JevError::response("response.answers must be an object"))?;
    if answers.len() != questions.len()
        || !questions
            .keys()
            .all(|question| answers.contains_key(question))
    {
        return Err(JevError::response(
            "response answer IDs differ from request question IDs",
        ));
    }
    for (id, question) in questions {
        let question = object(question, &format!("questions.{id}"))?;
        let answer = answers[id]
            .as_object()
            .ok_or_else(|| JevError::response(format!("answers.{id} must be an object")))?;
        if answer.get("type") != question.get("type") {
            return Err(JevError::response(format!(
                "answers.{id}.type differs from the request"
            )));
        }
        match question.get("type").and_then(Value::as_str) {
            Some("noul") => {
                probability(
                    answer.get("noul").unwrap_or(&Value::Null),
                    &format!("answers.{id}.noul"),
                )?;
            }
            Some("choice") => {
                let criteria = question
                    .get("criteria")
                    .and_then(Value::as_object)
                    .ok_or_else(|| {
                        JevError::request(format!("questions.{id}.criteria must be an object"))
                    })?;
                validate_distribution(answer, criteria, &format!("answers.{id}"))?;
                let selected = answer
                    .get("choice")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        JevError::response(format!("answers.{id}.choice must be a string"))
                    })?;
                if !criteria.contains_key(selected) {
                    return Err(JevError::response(format!(
                        "answers.{id}.choice is not a requested label"
                    )));
                }
                let selected_probability = answer["probabilities"][selected]
                    .as_f64()
                    .ok_or_else(|| JevError::response("selected probability is missing"))?;
                if answer["probabilities"].as_object().is_some_and(|values| {
                    values
                        .values()
                        .filter_map(Value::as_f64)
                        .any(|value| value > selected_probability + 1e-6)
                }) {
                    return Err(JevError::response(format!(
                        "answers.{id}.choice is not a highest-probability label"
                    )));
                }
            }
            Some("score") => {
                let levels = question
                    .get("criteria")
                    .and_then(Value::as_array)
                    .ok_or_else(|| JevError::request("score criteria must be an array"))?;
                let criteria: Map<String, Value> = levels
                    .iter()
                    .enumerate()
                    .map(|(index, level)| (index.to_string(), level.clone()))
                    .collect();
                validate_distribution(answer, &criteria, &format!("answers.{id}"))?;
                answer
                    .get("score")
                    .and_then(Value::as_f64)
                    .filter(|score| {
                        score.is_finite()
                            && *score >= 0.0
                            && *score <= levels.len().saturating_sub(1) as f64
                    })
                    .ok_or_else(|| {
                        JevError::response(format!("answers.{id}.score is outside the rubric"))
                    })?;
                // Rounded provider values need not satisfy an exact arithmetic identity.
                if answer.get("legend").and_then(Value::as_object) != Some(&criteria) {
                    return Err(JevError::response(format!(
                        "answers.{id}.legend differs from requested criteria"
                    )));
                }
            }
            _ => return Err(JevError::request(format!("questions.{id}.type is invalid"))),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_response;
    use serde_json::{json, Value};

    fn request() -> Value {
        json!({"model":"jev-test","state":null,"questions":{
            "truth":{"type":"noul","instructions":null},
            "pick":{"type":"choice","instructions":null,"criteria":{"a":null,"b":{"meaning":"other"}}},
            "quality":{"type":"score","instructions":null,"criteria":[null,{"meaning":"good"}]}
        }})
    }

    fn response() -> Value {
        json!({"model":"jev-test","usage":{"input_tokens":12,"output_tokens":3},"answers":{
            "truth":{"type":"noul","noul":0.5},
            "pick":{"type":"choice","choice":"a","confidence":0.8,"probabilities":{"a":0.8,"b":0.2}},
            "quality":{"type":"score","score":0.75,"confidence":0.7,"probabilities":{"0":0.25,"1":0.75},"legend":{"0":null,"1":{"meaning":"good"}}}
        }})
    }

    #[test]
    fn validates_all_primitives_without_interpreting_ambiguous_judgments() {
        validate_response(&request(), &response()).unwrap();
    }

    #[test]
    fn rejects_incompatible_answers_and_malformed_metadata() {
        for (pointer, value) in [
            ("/model", json!("")),
            ("/usage/input_tokens", json!(-1)),
            ("/answers/truth/type", json!("score")),
            ("/answers/truth/noul", json!(1.1)),
            ("/answers/pick/choice", json!("b")),
            ("/answers/pick/confidence", json!(1.1)),
            ("/answers/pick/probabilities", json!({"a":0.8,"c":0.2})),
            ("/answers/pick/probabilities", json!({"a":0.8,"b":0.4})),
            ("/answers/quality/score", json!(2)),
            ("/answers/quality/legend", json!({"0":null,"1":"different"})),
        ] {
            let mut invalid = response();
            *invalid.pointer_mut(pointer).unwrap() = value;
            assert!(
                validate_response(&request(), &invalid).is_err(),
                "accepted {pointer}"
            );
        }
        let mut missing = response();
        missing["answers"].as_object_mut().unwrap().remove("truth");
        assert!(validate_response(&request(), &missing).is_err());
        let mut extra = response();
        extra["answers"]["extra"] = json!({"type":"noul","noul":0.9});
        assert!(validate_response(&request(), &extra).is_err());
        let mut missing_usage = response();
        missing_usage["usage"] = json!({});
        assert!(validate_response(&request(), &missing_usage).is_err());
        assert!(validate_response(&json!({}), &response()).is_err());
    }
}
