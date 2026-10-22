//! The `jev` vendor (TypeSafe System One) — the first registered classification
//! provider.
//!
//! Wire format: `POST /v1/systemone` with a JSON body
//! `{"model": "...", "state": <evidence>, "questions": {"answer": <question>}}`
//! for single-question requests, and
//! `{"model": "...", "state": <evidence>, "questions": {"answer_0": q0, ...}}`
//! for batches. Response: `{"model": "...", "answers": {"answer": {...}}, "usage": {...}}`.
use super::{
    ClassificationProvider, ProviderContractError, ProviderDefaults, ProviderResponse,
    ProviderUsage,
};
use serde_json::{Map, Value, json};

pub struct Jev;
pub static JEV: Jev = Jev;

static DEFAULTS: ProviderDefaults = ProviderDefaults {
    host: "https://api.typesafe.ai",
    model: "jev-latest",
    endpoint_path: "v1/systemone",
    models_path: "v1/models",
    docs_url: "https://docs.typesafe.ai/introduction",
};

/// The question key of a single-question request and its response.
const SINGLE_ANSWER_KEY: &str = "answer";

impl ClassificationProvider for Jev {
    fn defaults(&self) -> &'static ProviderDefaults {
        &DEFAULTS
    }

    fn headers(&self, images: bool, long_context: bool) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        for (enabled, name) in [
            (images, "typesafe-danger-zone-images"),
            (long_context, "typesafe-danger-zone-long-context"),
        ] {
            if enabled {
                headers.insert(name, reqwest::header::HeaderValue::from_static("true"));
            }
        }
        headers
    }

    fn build(
        &self,
        state: &Value,
        questions: &[&Value],
        model: &str,
        assets: Option<&Value>,
    ) -> Value {
        let keyed: Map<String, Value> = match questions {
            [question] => [(SINGLE_ANSWER_KEY.to_owned(), (*question).clone())]
                .into_iter()
                .collect(),
            _ => questions
                .iter()
                .enumerate()
                .map(|(index, question)| (batch_answer_key(index), (*question).clone()))
                .collect(),
        };
        let mut request = json!({"model": model, "state": state, "questions": keyed});
        if let Some(assets) = assets {
            request["assets"] = assets.clone();
        }
        request
    }

    fn read(&self, request: &Value, response: &Value) -> ProviderResponse {
        let usage = || {
            let tokens = |key: &str| response["usage"][key].as_u64();
            Some(ProviderUsage {
                input_tokens: tokens("input_tokens")?,
                output_tokens: tokens("output_tokens")?,
            })
        };
        let single = request["questions"]
            .as_object()
            .is_some_and(|questions| questions.contains_key(SINGLE_ANSWER_KEY));
        ProviderResponse {
            answers: if single {
                validate_wire(request, response)
                    .map(|()| vec![Ok(response["answers"][SINGLE_ANSWER_KEY].clone())])
            } else {
                batch_answers(request, response)
            },
            model: response["model"].as_str().map(str::to_owned),
            usage: usage(),
        }
    }

    fn models(&self, response: &Value) -> Result<Value, ProviderContractError> {
        let models = response
            .get("models")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid_response("response.models must be an array"))?;
        let models = models
            .iter()
            .map(|model| {
                let mut listed = Map::new();
                for field in ["name", "description", "release_date"] {
                    let value = model.get(field).and_then(Value::as_str).ok_or_else(|| {
                        invalid_response(format!("model.{field} must be a string"))
                    })?;
                    if field == "name" && value.trim().is_empty() {
                        return Err(invalid_response("model.name must be non-blank"));
                    }
                    listed.insert(field.to_owned(), json!(value));
                }
                Ok(Value::Object(listed))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({ "models": models }))
    }
}

/// A batch response's answers, each validated on its own: global metadata
/// and unexpected answer keys fail the response; a missing or malformed
/// expected answer fails only its own question.
fn batch_answers(
    request: &Value,
    response: &Value,
) -> Result<Vec<Result<Value, ProviderContractError>>, ProviderContractError> {
    let questions = request
        .get("questions")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid_request("request.questions must be an object"))?;
    let answers = response
        .get("answers")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid_response("response.answers must be an object"))?;
    if answers.keys().any(|key| !questions.contains_key(key)) {
        return Err(invalid_response(
            "response contains an unexpected answer ID",
        ));
    }
    // The same validator checks global metadata once, then each keyed
    // answer. Projection omits state/assets, which response validation
    // does not read, so large input bodies are never cloned here.
    let mut projected = json!({"model":response["model"],"usage":response["usage"],"answers":{}});
    validate_wire(&json!({"questions":{}}), &projected)?;
    (0..questions.len())
        .map(|index| {
            let key = batch_answer_key(index);
            let question = questions
                .get(&key)
                .ok_or_else(|| invalid_request("batch question indexes must be contiguous"))?;
            projected["answers"] =
                json!({key.clone():answers.get(&key).cloned().unwrap_or(Value::Null)});
            let result = validate_wire(&json!({"questions":{key.clone():question}}), &projected)
                .map(|()| projected["answers"][&key].clone());
            Ok(result)
        })
        .collect()
}

/// The key of the question at `index` in a batch request and its response.
fn batch_answer_key(index: usize) -> String {
    format!("answer_{index}")
}

const PROBABILITY_TOLERANCE: f64 = 0.02;
const MAX_SCORE_ROUNDING_DEVIATION: f64 = 0.25;

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

#[cfg(test)]
mod tests {
    use super::validate_wire;
    use crate::providers::classification::ClassificationProvider;
    use serde_json::{Value, json};

    fn request() -> Value {
        json!({"model":"jev-test","state":null,"questions":{
            "truth":{"type":"noul","instructions":null},
            "pick":{"type":"choice","instructions":null,"criteria":{"a":null,"b":{"meaning":"other"}}},
            "quality":{"type":"score","instructions":null,"criteria":["missing",{"meaning":"good"}]},
            "region":{"type":"box"}
        }})
    }

    fn response() -> Value {
        json!({"model":"jev-test","usage":{"input_tokens":12,"output_tokens":3},"answers":{
            "truth":{"type":"noul","noul":0.5},
            "pick":{"type":"choice","choice":"a","confidence":0.8,"probabilities":{"a":0.8,"b":0.2}},
            "quality":{"type":"score","score":0.75,"confidence":0.7,"probabilities":{"0":0.25,"1":0.75},"legend":{"0":"missing","1":{"meaning":"good"}}},
            "region":{"type":"box","box":{"top":0.1,"bottom":0.9,"left":0.0,"right":1.0},"confidence":null}
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
            (100, vec![0.01; 100], 49.5, true),
            (100, vec![0.01; 100], 49.74, true),
            (100, vec![0.01; 100], 49.76, false),
            (100, vec![0.01; 100], 74.0, false),
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

    #[test]
    fn box_requires_finite_normalized_ordered_edges_and_present_nullable_confidence() {
        let request = json!({"questions":{"region":{"type":"box"}}});
        let valid = json!({"model":"vision","usage":{"input_tokens":10,"output_tokens":4},
            "answers":{"region":{"type":"box","box":{"top":0.1,"bottom":0.9,"left":0.2,"right":0.7},"confidence":null}}});
        validate_wire(&request, &valid).unwrap();
        let mut numeric_confidence = valid.clone();
        numeric_confidence["answers"]["region"]["confidence"] = json!(0.7);
        validate_wire(&request, &numeric_confidence).unwrap();
        for (pointer, value) in [
            ("/answers/region/box/top", json!(-0.1)),
            ("/answers/region/box/bottom", json!(1.1)),
            ("/answers/region/box/top", json!(0.95)),
            ("/answers/region/box/top", json!(0.9)),
            ("/answers/region/box/right", json!(0.2)),
            ("/answers/region/box/right", json!(0.1)),
            ("/answers/region/box/left", Value::Null),
            ("/answers/region/box/left", json!("NaN")),
            ("/answers/region/confidence", json!(1.1)),
        ] {
            let mut invalid = valid.clone();
            *invalid.pointer_mut(pointer).unwrap() = value;
            assert!(
                validate_wire(&request, &invalid).is_err(),
                "accepted {pointer}"
            );
        }
        let mut missing = valid;
        missing["answers"]["region"]
            .as_object_mut()
            .unwrap()
            .remove("confidence");
        assert!(validate_wire(&request, &missing).is_err());
    }

    #[test]
    fn rejects_undeclared_fields_in_each_answer_shape() {
        for pointer in [
            "/answers/truth",
            "/answers/pick",
            "/answers/quality",
            "/answers/region",
            "/answers/region/box",
        ] {
            let mut invalid = response();
            invalid
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unexpected".into(), json!("extension"));
            assert!(
                validate_wire(&request(), &invalid).is_err(),
                "accepted {pointer}"
            );
        }
    }

    #[test]
    fn request_builder_preserves_structured_optional_and_null_question_fields() {
        let questions = [
            json!({"type":"noul"}),
            json!({"type":"noul","instructions":null,"criteria":{"true":null}}),
            json!({"type":"choice","criteria":{"only":null}}),
            json!({"type":"score","instructions":{"task":["grade"]},"criteria":[{"level":"present"}]}),
            json!({"type":"box","instructions":["locate","opaque"]}),
        ];
        let assets = json!({"opaque":{"type":"image","format":"png","data_base64":"bytes"}});
        let request = super::JEV.build(
            &json!(["opaque"]),
            &questions.iter().collect::<Vec<_>>(),
            "vision",
            Some(&assets),
        );
        assert_eq!(request["assets"], assets);
        for (index, question) in questions.iter().enumerate() {
            assert_eq!(&request["questions"][format!("answer_{index}")], question);
        }
        let plain = super::JEV.build(&json!("text"), &[&questions[0]], "m", None);
        assert_eq!(plain["questions"], json!({"answer": questions[0]}));
        assert!(plain.get("assets").is_none());
        assert!(super::JEV.headers(false, false).is_empty());
        let headers = super::JEV.headers(true, true);
        assert_eq!(headers["TypeSafe-Danger-Zone-Images"], "true");
        assert_eq!(headers["TypeSafe-Danger-Zone-Long-Context"], "true");
    }

    /// One question is one whole response; the model and usage are read
    /// even from a response that fails validation.
    #[test]
    fn a_single_question_response_reads_its_answer_model_and_usage() {
        let question = json!({"type":"noul","instructions":"q"});
        let request = super::JEV.build(&json!("s"), &[&question], "m", None);
        let response = json!({"model":"jev-1","usage":{"input_tokens":7,"output_tokens":2},
            "answers":{"answer":{"type":"noul","noul":0.4}}});
        let read = super::JEV.read(&request, &response);
        assert_eq!(
            read.answers.expect("valid response"),
            vec![Ok(json!({"type":"noul","noul":0.4}))]
        );
        assert_eq!(read.model.as_deref(), Some("jev-1"));
        assert_eq!(
            read.usage,
            Some(super::ProviderUsage {
                input_tokens: 7,
                output_tokens: 2
            })
        );
        let mut invalid = response;
        invalid["answers"]["answer"]["noul"] = json!(2);
        let read = super::JEV.read(&request, &invalid);
        assert!(read.answers.is_err());
        assert_eq!(read.usage.map(|usage| usage.input_tokens), Some(7));
        invalid["usage"] = json!({"input_tokens": 1});
        assert_eq!(super::JEV.read(&request, &invalid).usage, None);
    }

    #[test]
    fn model_discovery_accepts_live_timestamp_strings_and_rejects_missing_fields() {
        let response = json!({"object":"list","models":[{"name":"jev-preview","description":"Preview",
            "release_date":"2026-09-10T18:39:06.057655+00:00","owner":"typesafe"}]});
        // Only the clasify output fields leave the vendor wire.
        assert_eq!(
            super::JEV.models(&response).unwrap(),
            json!({"models":[{"name":"jev-preview","description":"Preview",
                "release_date":"2026-09-10T18:39:06.057655+00:00"}]})
        );
        assert_eq!(
            super::JEV.models(&json!({"models":[]})).unwrap(),
            json!({"models":[]})
        );
        for invalid in [
            json!({}),
            json!({"models":null}),
            json!({"models":[{"name":""}]}),
            json!({"models":[{"name":"m","description":"d","release_date":null}]}),
        ] {
            assert!(super::JEV.models(&invalid).is_err());
        }
    }
}
