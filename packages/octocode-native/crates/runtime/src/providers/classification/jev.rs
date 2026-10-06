//! The `jev` vendor (TypeSafe System One) — the first registered classification
//! provider.
//!
//! Wire format: `POST /v1/systemone` with a JSON body
//! `{"model": "...", "state": <evidence>, "questions": {"answer": <question>}}`
//! for single-question requests, and
//! `{"model": "...", "state": <evidence>, "questions": {"answer_0": q0, ...}}`
//! for batches. Response: `{"model": "...", "answers": {"answer": {...}}, "usage": {...}}`.
use super::{ClassificationProvider, ProviderContractError};
use serde_json::{Map, Value, json};

pub struct Jev;
pub static JEV: Jev = Jev;

impl ClassificationProvider for Jev {
    fn id(&self) -> &'static str {
        "jev"
    }

    fn key_env(&self) -> &'static str {
        "OCTOCODE_JEV_KEY"
    }

    fn default_host(&self) -> &'static str {
        "https://api.typesafe.ai"
    }

    fn default_model(&self) -> &'static str {
        "jev-latest"
    }

    fn endpoint_path(&self) -> &'static str {
        "v1/systemone"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.typesafe.ai/introduction"
    }

    fn build_request(&self, state: &Value, question: &Value, model: &str) -> Value {
        json!({"model": model, "state": state, "questions": {"answer": question}})
    }

    fn build_batch_request(
        &self,
        state: &Value,
        questions: &[(usize, &Value)],
        model: &str,
    ) -> Value {
        let qs: Map<String, Value> = questions
            .iter()
            .map(|(i, q)| (format!("answer_{i}"), (*q).clone()))
            .collect();
        json!({"model": model, "state": state, "questions": qs})
    }

    fn extract_answer<'a>(&self, response: &'a Value) -> Option<&'a Value> {
        response.get("answers")?.get("answer")
    }

    fn batch_answer_key(&self, index: usize) -> String {
        format!("answer_{index}")
    }

    fn validate_response(
        &self,
        request: &Value,
        response: &Value,
    ) -> Result<(), ProviderContractError> {
        validate_wire(request, response)
    }
}

const PROBABILITY_TOLERANCE: f64 = 0.02;

fn invalid_request(message: impl Into<String>) -> ProviderContractError {
    ProviderContractError {
        request: true,
        message: message.into(),
    }
}

fn invalid_response(message: impl Into<String>) -> ProviderContractError {
    ProviderContractError {
        request: false,
        message: message.into(),
    }
}

fn object<'a>(
    value: &'a Value,
    field: &str,
) -> Result<&'a Map<String, Value>, ProviderContractError> {
    value
        .as_object()
        .ok_or_else(|| invalid_request(format!("{field} must be an object")))
}

fn probability(value: &Value, field: &str) -> Result<f64, ProviderContractError> {
    value
        .as_f64()
        .filter(|number| number.is_finite() && (0.0..=1.0).contains(number))
        .ok_or_else(|| invalid_response(format!("{field} must be between 0 and 1")))
}

fn validate_distribution(
    answer: &Map<String, Value>,
    criteria: &Map<String, Value>,
    field: &str,
) -> Result<(), ProviderContractError> {
    let probabilities = answer
        .get("probabilities")
        .unwrap_or(&Value::Null)
        .as_object()
        .ok_or_else(|| invalid_response(format!("{field}.probabilities must be an object")))?;
    if probabilities.len() != criteria.len()
        || !criteria
            .keys()
            .all(|label| probabilities.contains_key(label))
    {
        return Err(invalid_response(format!(
            "{field} probability labels differ from the request"
        )));
    }
    let mut sum = 0.0;
    for (label, value) in probabilities {
        sum += probability(value, &format!("{field}.probabilities.{label}"))?;
    }
    if (sum - 1.0).abs() > PROBABILITY_TOLERANCE {
        return Err(invalid_response(format!(
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
fn validate_wire(request: &Value, response: &Value) -> Result<(), ProviderContractError> {
    let request = object(request, "request")?;
    if !response.is_object() {
        return Err(invalid_response("response must be an object"));
    }
    response["model"]
        .as_str()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .ok_or_else(|| invalid_response("response.model must be non-blank"))?;
    let usage = response["usage"]
        .as_object()
        .ok_or_else(|| invalid_response("response.usage must be an object"))?;
    for field in ["input_tokens", "output_tokens"] {
        if usage.get(field).and_then(Value::as_u64).is_none() {
            return Err(invalid_response(format!(
                "response.usage.{field} must be a non-negative integer"
            )));
        }
    }
    let questions = request
        .get("questions")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid_request("request.questions must be an object"))?;
    let answers = response["answers"]
        .as_object()
        .ok_or_else(|| invalid_response("response.answers must be an object"))?;
    if answers.len() != questions.len()
        || !questions
            .keys()
            .all(|question| answers.contains_key(question))
    {
        return Err(invalid_response(
            "response answer IDs differ from request question IDs",
        ));
    }
    for (id, question) in questions {
        let question = object(question, &format!("questions.{id}"))?;
        let answer = answers[id]
            .as_object()
            .ok_or_else(|| invalid_response(format!("answers.{id} must be an object")))?;
        if answer.get("type") != question.get("type") {
            return Err(invalid_response(format!(
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
                        invalid_request(format!("questions.{id}.criteria must be an object"))
                    })?;
                validate_distribution(answer, criteria, &format!("answers.{id}"))?;
                let selected = answer
                    .get("choice")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        invalid_response(format!("answers.{id}.choice must be a string"))
                    })?;
                if !criteria.contains_key(selected) {
                    return Err(invalid_response(format!(
                        "answers.{id}.choice is not a requested label"
                    )));
                }
                let selected_probability = answer["probabilities"][selected]
                    .as_f64()
                    .ok_or_else(|| invalid_response("selected probability is missing"))?;
                if answer["probabilities"].as_object().is_some_and(|values| {
                    values
                        .values()
                        .filter_map(Value::as_f64)
                        .any(|value| value > selected_probability + 1e-6)
                }) {
                    return Err(invalid_response(format!(
                        "answers.{id}.choice is not a highest-probability label"
                    )));
                }
            }
            Some("score") => {
                let levels = question
                    .get("criteria")
                    .and_then(Value::as_array)
                    .ok_or_else(|| invalid_request("score criteria must be an array"))?;
                let criteria: Map<String, Value> = levels
                    .iter()
                    .enumerate()
                    .map(|(index, level)| (index.to_string(), level.clone()))
                    .collect();
                validate_distribution(answer, &criteria, &format!("answers.{id}"))?;
                let score = answer
                    .get("score")
                    .and_then(Value::as_f64)
                    .filter(|score| {
                        score.is_finite()
                            && *score >= 0.0
                            && *score <= levels.len().saturating_sub(1) as f64
                    })
                    .ok_or_else(|| {
                        invalid_response(format!("answers.{id}.score is outside the rubric"))
                    })?;
                let expected = (0..levels.len()).try_fold(0.0, |sum, index| {
                    probability(
                        &answer["probabilities"][index.to_string()],
                        &format!("answers.{id}.probabilities.{index}"),
                    )
                    .map(|value| sum + index as f64 * value)
                })?;
                // Compatibility allowance for the two-decimal values in provider examples:
                // each rounded probability contributes at most index * 0.005 error,
                // plus 0.005 for the rounded score. This rejects contradictions without
                // imposing exact equality or replacing the provider's score.
                let index_sum = levels.len() * levels.len().saturating_sub(1) / 2;
                let tolerance = 0.005 * (1 + index_sum) as f64 + 1e-9;
                if (score - expected).abs() > tolerance {
                    return Err(invalid_response(format!(
                        "answers.{id}.score disagrees with its probabilities"
                    )));
                }
                if answer.get("legend").and_then(Value::as_object) != Some(&criteria) {
                    return Err(invalid_response(format!(
                        "answers.{id}.legend differs from requested criteria"
                    )));
                }
            }
            _ => return Err(invalid_request(format!("questions.{id}.type is invalid"))),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_wire;
    use serde_json::{Value, json};

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
        validate_wire(&request(), &response()).unwrap();
    }

    #[test]
    fn score_agrees_with_distribution_allowing_two_decimal_rounding() {
        for (levels, probabilities, score, valid) in [
            (3, vec![0.21, 0.66, 0.13], 0.92, true),
            (3, vec![0.21, 0.66, 0.13], 0.93, true),
            (3, vec![0.21, 0.66, 0.13], 0.96, false),
            (3, vec![1.0, 0.0, 0.0], 2.0, false),
            (2, vec![0.25, 0.75], 0.76, true),
            (2, vec![0.25, 0.75], 0.77, false),
            (10, vec![0.10; 10], 4.725, true),
            (10, vec![0.10; 10], 4.74, false),
        ] {
            let criteria: Vec<_> = (0..levels).map(|i| json!(format!("Level {i}"))).collect();
            let legend: serde_json::Map<String, Value> = criteria
                .iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v.clone()))
                .collect();
            let probabilities: serde_json::Map<String, Value> = probabilities
                .iter()
                .enumerate()
                .map(|(i, p)| (i.to_string(), json!(p)))
                .collect();
            let request = json!({"questions":{"q":{"type":"score","criteria":criteria}}});
            let response = json!({"model":"m","usage":{"input_tokens":1,"output_tokens":1},
                "answers":{"q":{"type":"score","score":score,"legend":legend,
                    "confidence":0.4,"probabilities":probabilities}}});
            assert_eq!(
                validate_wire(&request, &response).is_ok(),
                valid,
                "levels={levels}, score={score}"
            );
        }
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
                validate_wire(&request(), &invalid).is_err(),
                "accepted {pointer}"
            );
        }
        let mut missing = response();
        missing["answers"].as_object_mut().unwrap().remove("truth");
        assert!(validate_wire(&request(), &missing).is_err());
        let mut extra = response();
        extra["answers"]["extra"] = json!({"type":"noul","noul":0.9});
        assert!(validate_wire(&request(), &extra).is_err());
        let mut missing_usage = response();
        missing_usage["usage"] = json!({});
        assert!(validate_wire(&request(), &missing_usage).is_err());
        assert!(validate_wire(&json!({}), &response()).is_err());
    }
}
