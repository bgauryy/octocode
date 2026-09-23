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
        octocode_engine::jev::validate_response(request, response).map_err(|error| {
            ProviderContractError {
                request: error.code == "invalidJevRequest",
                message: error.message,
            }
        })
    }
}
