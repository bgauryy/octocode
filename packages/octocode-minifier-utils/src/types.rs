use napi_derive::napi;

#[napi(object)]
#[derive(Debug, Clone, Default)]
pub struct GetExtensionOptions {
    pub lowercase: Option<bool>,
    pub fallback:  Option<String>,
}

#[napi(object)]
#[derive(Debug, Clone)]
pub struct MinifyResult {
    pub content: String,
    pub failed:  bool,
    /// Strategy name or "failed"
    pub r#type:  String,
    pub reason:  Option<String>,
}

impl MinifyResult {
    pub fn ok(content: String, strategy: &str) -> Self {
        MinifyResult { content, failed: false, r#type: strategy.to_owned(), reason: None }
    }
    pub fn fail(content: String, reason: impl Into<String>) -> Self {
        MinifyResult { content, failed: true, r#type: "failed".to_owned(), reason: Some(reason.into()) }
    }
}

#[napi(object)]
#[derive(Debug, Clone)]
pub struct FileTypeMinifyConfig {
    pub strategy: String,
    /// CommentPatternGroup | CommentPatternGroup[]
    pub comments: Option<serde_json::Value>,
}

#[napi(object)]
#[derive(Debug, Clone, Default)]
pub struct YamlConversionConfig {
    pub sort_keys:      Option<bool>,
    pub keys_priority:  Option<Vec<String>>,
}
