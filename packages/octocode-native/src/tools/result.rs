//! Tool-owned diagnostics remain separate from public domain data.
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Default, Serialize)]
pub struct ToolDiagnostics {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub codes: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
    pub partial: bool,
}
impl ToolDiagnostics {
    pub fn add(&mut self, code: &str, hint: &str, partial: bool) {
        if !self.codes.iter().any(|value| value == code) {
            self.codes.push(code.into());
        }
        if !self.hints.iter().any(|value| value == hint) {
            self.hints.push(hint.into());
        }
        self.partial |= partial;
    }
}

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct ToolData {
    pub data: Value,
    #[serde(skip)]
    pub diagnostics: ToolDiagnostics,
    #[serde(skip)]
    pub status: Option<&'static str>,
}
impl From<Value> for ToolData {
    fn from(data: Value) -> Self {
        Self {
            data,
            diagnostics: ToolDiagnostics::default(),
            status: None,
        }
    }
}
