//! The `jev` vendor (TypeSafe System One) — the first registered classification
//! provider.
//!
//! Wire format: `POST /v1/systemone` with a JSON body
//! `{"model": "...", "state": <evidence>, "questions": {"answer": <question>}}`
//! for single-question requests, and
//! `{"model": "...", "state": <evidence>, "questions": {"answer_0": q0, ...}}`
//! for batches. Response: `{"model": "...", "answers": {"answer": {...}}, "usage": {...}}`.
mod wire;
use super::{
    ClassificationProvider, ProviderContractError, ProviderDefaults, ProviderResponse,
    ProviderUsage,
};
use serde_json::{Map, Value, json};
use wire::{invalid_request, invalid_response, validate_wire};

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

#[cfg(test)]
mod tests;
