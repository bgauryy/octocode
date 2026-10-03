#[cfg(feature = "napi-addon")]
use napi_derive::napi;

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[cfg_attr(feature = "napi-addon", napi(object))]
pub struct SanitizationResult {
    pub content: String,
    pub has_secrets: bool,
    pub secrets_detected: Vec<String>,
    pub warnings: Vec<String>,
}
