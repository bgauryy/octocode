#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct SanitizationResult<'a> {
    pub content: std::borrow::Cow<'a, str>,
    pub has_secrets: bool,
    pub secrets_detected: Vec<String>,
    pub warnings: Vec<String>,
}

impl SanitizationResult<'_> {
    /// Own the source only at a boundary that outlives the input buffer.
    pub fn into_owned(self) -> SanitizationResult<'static> {
        SanitizationResult {
            content: std::borrow::Cow::Owned(self.content.into_owned()),
            has_secrets: self.has_secrets,
            secrets_detected: self.secrets_detected,
            warnings: self.warnings,
        }
    }
}
