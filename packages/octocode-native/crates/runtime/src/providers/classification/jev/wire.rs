use super::super::ProviderContractError;
use serde_json::{Map, Value, json};

const PROBABILITY_TOLERANCE: f64 = 0.02;
const MAX_SCORE_ROUNDING_DEVIATION: f64 = 0.25;

pub(super) fn invalid_request(message: impl Into<String>) -> ProviderContractError {
    ProviderContractError {
        request: true,
        message: message.into(),
    }
}

pub(super) fn invalid_response(message: impl Into<String>) -> ProviderContractError {
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
pub(super) fn validate_wire(
    request: &Value,
    response: &Value,
) -> Result<(), ProviderContractError> {
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
                // imposing exact equality or replacing the provider's score. Cap the
                // allowance at a quarter rubric step: the API defines score as the
                // expectation of the returned distribution, so a longer rubric must
                // not turn rounding tolerance into permission for multi-level errors.
                let index_sum = levels.len() * levels.len().saturating_sub(1) / 2;
                let tolerance =
                    (0.005 * (1 + index_sum) as f64).min(MAX_SCORE_ROUNDING_DEVIATION) + 1e-9;
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
            Some("box") => {
                let coordinates =
                    answer
                        .get("box")
                        .and_then(Value::as_object)
                        .ok_or_else(|| {
                            invalid_response(format!("answers.{id}.box must be an object"))
                        })?;
                let edge = |name: &str| {
                    probability(
                        coordinates.get(name).unwrap_or(&Value::Null),
                        &format!("answers.{id}.box.{name}"),
                    )
                };
                let (top, bottom, left, right) =
                    (edge("top")?, edge("bottom")?, edge("left")?, edge("right")?);
                if top >= bottom || left >= right {
                    return Err(invalid_response(format!(
                        "answers.{id}.box edges must be ordered top < bottom and left < right"
                    )));
                }
                match answer.get("confidence") {
                    Some(Value::Null) => {}
                    Some(value) => {
                        probability(value, &format!("answers.{id}.confidence"))?;
                    }
                    None => {
                        return Err(invalid_response(format!(
                            "answers.{id}.confidence must be present, using null when unavailable"
                        )));
                    }
                }
            }
            _ => return Err(invalid_request(format!("questions.{id}.type is invalid"))),
        }
    }
    if !answers.is_empty() {
        // The candidate is built for this check alone, so it is validated in
        // place instead of cloned again.
        crate::contracts::validate_output_in_place(
            "clasify",
            &mut json!({"queries":[{
                "id":"provider",
                "model":response["model"],
                "answers":response["answers"],
                "usage":{
                    "input_tokens":response["usage"]["input_tokens"],
                    "output_tokens":response["usage"]["output_tokens"],
                },
            }]}),
        )
        .map_err(|_| {
            invalid_response("Provider answers violate the canonical clasify output contract")
        })?;
    }
    Ok(())
}
