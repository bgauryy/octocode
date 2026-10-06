#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct SanitizationResult {
    pub content: String,
    pub has_secrets: bool,
    pub secrets_detected: Vec<String>,
    pub warnings: Vec<String>,
}
