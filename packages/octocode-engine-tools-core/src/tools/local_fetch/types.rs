use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChunkType {
    #[default]
    Lines,
    Bytes,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MinifyMode {
    #[default]
    None,
    Standard,
    Symbols,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalFetchRequest {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_content: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_string: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_string_is_regex: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_string_case_sensitive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_lines: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_type: Option<ChunkType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minify: Option<MinifyMode>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    pub chunk_type: ChunkType,
    pub offset: usize,
    pub length: usize,
    pub limit: usize,
    pub total_lines: usize,
    pub total_bytes: usize,
    pub has_more: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Continuation {
    pub tool: String,
    pub query: LocalFetchRequest,
    pub confidence: String,
    #[serde(rename = "why", skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NextCalls {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#continue: Option<Continuation>,
    #[serde(rename = "readBoundedLines", skip_serializing_if = "Option::is_none")]
    pub read_bounded_lines: Option<Continuation>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinifyFallback {
    pub requested: MinifyMode,
    pub applied: MinifyMode,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PartialReason {
    #[serde(rename = "full-content-size-limit")]
    FullContentLimit,
    #[serde(rename = "full-content-source-size-limit")]
    FullContentSourceSizeLimit,
    #[serde(rename = "security-selected-view-size-limit")]
    SecuritySelectedViewSizeLimit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalFetchResult {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(skip)]
    pub status: String,
    #[serde(skip)]
    pub resource_missing: bool,
    #[serde(skip)]
    pub source_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_view: Option<MinifyMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minify_fallback: Option<MinifyFallback>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub hints: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_lines: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub source_line_ranges: Vec<LineRange>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub match_ranges: Vec<LineRange>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub matched_lines: Vec<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_match_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_chars: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returned_chars: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returned_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returned_lines: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<Pagination>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_partial: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub partial_reasons: Vec<PartialReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_limit: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub metadata_unavailable: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<NextCalls>,
}
impl LocalFetchResult {
    pub fn error(_path: String, code: &str, message: String) -> Self {
        Self {
            path: String::new(),
            status: "error".into(),
            resource_missing: false,
            source_sha256: None,
            content: None,
            content_view: None,
            minify_fallback: None,
            error_code: Some(code.into()),
            error: Some(message),
            resolved_path: None,
            warnings: vec![],
            hints: vec![],
            total_lines: None,
            start_line: None,
            end_line: None,
            source_line_ranges: vec![],
            match_ranges: vec![],
            matched_lines: vec![],
            selected_match_count: None,
            modified: None,
            source_chars: None,
            source_bytes: None,
            returned_chars: None,
            returned_bytes: None,
            returned_lines: None,
            pagination: None,
            is_partial: None,
            partial_reasons: vec![],
            terminal_limit: None,
            metadata_unavailable: vec![],
            next: None,
        }
    }
}

pub trait CancellationCheck {
    fn check(&self) -> Result<(), String>;
}
pub struct NeverCancel;
impl CancellationCheck for NeverCancel {
    fn check(&self) -> Result<(), String> {
        Ok(())
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRead {
    pub canonical: PathBuf,
    pub display: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathFailure {
    pub code: String,
    pub message: String,
    pub safe_path: Option<String>,
    pub resource_missing: bool,
}
pub trait PathAccess {
    fn validate_read(&self, path: &Path) -> Result<ValidatedRead, PathFailure>;
}
pub trait ContentScan {
    fn sanitize(&self, text: &str, path: &Path) -> Result<(String, Vec<String>), (String, String)>;
}
pub trait RegexMatch {
    fn matching_ranges(
        &self,
        pattern: &str,
        case_sensitive: bool,
        input: &str,
    ) -> Result<Vec<(usize, usize)>, String>;
}

#[derive(Clone, Default)]
pub struct LocalFetchRegex {
    isolated: Option<Arc<crate::regex::IsolatedRegexEngine>>,
}
impl LocalFetchRegex {
    pub fn new(isolated: Option<Arc<crate::regex::IsolatedRegexEngine>>) -> Self {
        Self { isolated }
    }
}
impl RegexMatch for LocalFetchRegex {
    fn matching_ranges(
        &self,
        source: &str,
        case_sensitive: bool,
        input: &str,
    ) -> Result<Vec<(usize, usize)>, String> {
        use crate::regex::{EcmaPattern, RegexExecutionClass, RegexLimits};
        let flags = if case_sensitive { "g" } else { "gi" };
        let limits = RegexLimits {
            max_pattern_bytes: 4_096,
            max_input_bytes: 10 * 1024 * 1024,
            max_matches: 10_000,
        };
        let pattern = EcmaPattern::compile(source, flags, limits)
            .map_err(|error| format!("Invalid regex pattern: {}", error.message))?;
        let ranges = match pattern.execution_class() {
            RegexExecutionClass::LinearInProcess => pattern.find_ranges(input),
            RegexExecutionClass::RequiresIsolatedEngine => self
                .isolated
                .as_ref()
                .ok_or_else(|| {
                    "Regex execution unavailable: pattern requires the configured isolated ECMAScript worker".to_owned()
                })?
                .find_ranges(source, flags, input),
        }
        .map_err(|error| format!("Invalid regex pattern: {}", error.message))?;
        Ok(ranges
            .into_iter()
            .map(|range| (range.start, range.end))
            .collect())
    }
}

impl PathAccess for crate::policy::path::PathPolicy {
    fn validate_read(&self, path: &Path) -> Result<ValidatedRead, PathFailure> {
        crate::policy::path::PathPolicy::validate_read(self, path)
            .map(|validated| ValidatedRead {
                canonical: validated.canonical,
                display: validated.display,
            })
            .map_err(|error| {
                let missing = error.code == crate::policy::PolicyErrorCode::NotFound;
                PathFailure {
                    code: "pathValidationFailed".into(),
                    message: error.message,
                    safe_path: error.safe_path,
                    resource_missing: missing,
                }
            })
    }
}

impl ContentScan for crate::security::ContentSecurity {
    fn sanitize(&self, text: &str, path: &Path) -> Result<(String, Vec<String>), (String, String)> {
        let result = self.sanitize_text(text, Some(path));
        if result
            .secrets_detected
            .iter()
            .any(|name| name == "content-size-exceeded")
        {
            return Err((
                "contentSecurityLimit".into(),
                "The selected content view exceeds the secret scanner size limit.".into(),
            ));
        }
        Ok((result.content, Vec::new()))
    }
}
