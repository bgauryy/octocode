//! The `jev` vendor (TypeSafe System One) — the first registered classification
//! provider. Its response validation delegates to the shared `octocode_engine`
//! System One contract, which stays vendor-agnostic until a second vendor with a
//! different response schema requires `type`-dispatched validation.
use super::ClassificationProvider;
use octocode_engine::jev::JevError;
use serde_json::Value;

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
    fn validate_response(&self, request: &Value, response: &Value) -> Result<(), JevError> {
        octocode_engine::jev::validate_response(request, response)
    }
}
