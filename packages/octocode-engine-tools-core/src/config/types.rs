use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

pub const CONFIG_SCHEMA_VERSION: i64 = 1;
pub const CONFIG_FILE_NAME: &str = ".octocoderc";
pub const ENV_TOKEN_VARS: [&str; 4] = [
    "OCTOCODE_TOKEN",
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GITHUB_PERSONAL_ACCESS_TOKEN",
];
pub const PROTECTED_KEYS: [&str; 13] = [
    "PATH",
    "HOME",
    "SHELL",
    "USER",
    "LOGNAME",
    "PWD",
    "TMPDIR",
    "NODE_OPTIONS",
    "OCTOCODE_TOKEN",
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GITHUB_PERSONAL_ACCESS_TOKEN",
    "PYTHON",
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeSurface {
    Cli,
    #[default]
    Mcp,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileInput {
    Missing { path: PathBuf },
    Read { path: PathBuf, text: String },
    Unreadable { path: PathBuf, kind: String },
}
impl FileInput {
    pub fn path(&self) -> &PathBuf {
        match self {
            Self::Missing { path } | Self::Read { path, .. } | Self::Unreadable { path, .. } => {
                path
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConfigInput {
    pub env: BTreeMap<String, String>,
    pub cwd: PathBuf,
    pub os_home: PathBuf,
    pub trusted_project: bool,
    pub global_env: FileInput,
    pub project_env: FileInput,
    pub config_file: FileInput,
    pub runtime_surface: RuntimeSurface,
    pub revision: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RawConfig {
    #[serde(rename = "$schema", default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub version: Option<Value>,
    #[serde(default)]
    pub github: Option<Value>,
    #[serde(default)]
    pub local: Option<Value>,
    #[serde(default)]
    pub tools: Option<Value>,
    #[serde(default)]
    pub network: Option<Value>,
    #[serde(default)]
    pub lsp: Option<Value>,
    #[serde(default)]
    pub output: Option<Value>,
    #[serde(default)]
    pub storage: Option<Value>,
    #[serde(default)]
    pub extension: Option<Value>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedConfig {
    pub version: Value,
    pub github: GitHubConfig,
    pub local: LocalConfig,
    pub tools: ToolsConfig,
    pub network: NetworkConfig,
    pub lsp: LspConfig,
    pub output: OutputConfig,
    pub session: SessionConfig,
    pub storage: StorageConfig,
    pub extension: ExtensionConfig,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GitHubConfig {
    #[serde(rename = "apiUrl")]
    pub api_url: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LocalConfig {
    pub enabled: bool,
    #[serde(rename = "enableClone")]
    pub enable_clone: bool,
    #[serde(rename = "enableAstRewriteApply")]
    pub enable_ast_rewrite_apply: bool,
    #[serde(rename = "allowedPaths")]
    pub allowed_paths: Vec<String>,
    #[serde(rename = "workspaceRoot")]
    pub workspace_root: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolsConfig {
    pub enabled: Option<Vec<String>>,
    pub disabled: Option<Vec<String>>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub timeout: f64,
    #[serde(rename = "maxRetries")]
    pub max_retries: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LspConfig {
    #[serde(rename = "configPath")]
    pub config_path: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputConfig {
    pub format: String,
    pub pagination: PaginationConfig,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaginationConfig {
    #[serde(rename = "defaultCharLength")]
    pub default_char_length: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionConfig {
    #[serde(rename = "enableStats")]
    pub enable_stats: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StorageConfig {
    pub mode: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtensionConfig {
    pub storage: StorageConfig,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigSource {
    File,
    Defaults,
    Mixed,
    Env,
    Invalid,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warning,
    Error,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConfigDiagnostic {
    pub severity: Severity,
    pub code: String,
    pub field_path: Option<String>,
    pub message: String,
    pub source_path: Option<PathBuf>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct EnvApplyReport {
    pub applied: Vec<String>,
    pub skipped_protected: Vec<String>,
    pub skipped_existing: Vec<String>,
    pub sources: BTreeMap<String, String>,
    pub keys: Vec<String>,
}
#[derive(Clone, Default, Eq, PartialEq)]
pub struct ChildEnvPlan {
    pub(crate) set: BTreeMap<String, String>,
}
impl ChildEnvPlan {
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.set.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}
impl fmt::Debug for ChildEnvPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChildEnvPlan")
            .field("keys", &self.set.keys().collect::<Vec<_>>())
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct PrivateTokenSelection {
    token: String,
    source: String,
}
impl PrivateTokenSelection {
    pub fn token(&self) -> &str {
        &self.token
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub(crate) fn new(token: String, source: String) -> Self {
        Self { token, source }
    }
}
impl fmt::Debug for PrivateTokenSelection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PrivateTokenSelection")
            .field("token", &"[REDACTED]")
            .field("source", &self.source)
            .finish()
    }
}

#[derive(Clone)]
pub struct ConfigOutput {
    pub resolved: ResolvedConfig,
    pub(crate) effective_env: BTreeMap<String, String>,
    pub dotenv: EnvApplyReport,
    pub diagnostics: Vec<ConfigDiagnostic>,
    pub token: Option<PrivateTokenSelection>,
    pub child_env: ChildEnvPlan,
    pub source: ConfigSource,
    pub config_path: Option<PathBuf>,
    pub revision: u64,
}
impl ConfigOutput {
    pub fn effective_env(&self) -> impl Iterator<Item = (&str, &str)> {
        self.effective_env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
    }
    pub fn env_value(&self, key: &str) -> Option<&str> {
        self.effective_env.get(key).map(String::as_str)
    }
}
impl fmt::Debug for ConfigOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfigOutput")
            .field("resolved", &self.resolved)
            .field(
                "effective_env_keys",
                &self.effective_env.keys().collect::<Vec<_>>(),
            )
            .field("dotenv", &self.dotenv)
            .field("diagnostics", &self.diagnostics)
            .field("token", &self.token)
            .field("child_env", &self.child_env)
            .field("source", &self.source)
            .field("config_path", &self.config_path)
            .field("revision", &self.revision)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConfigInspectorData {
    pub home: PathBuf,
    pub global_env_path: PathBuf,
    pub project_env_path: PathBuf,
    pub loaded_keys: Vec<String>,
    pub global_key_count: usize,
    pub project_key_count: usize,
    pub storage_mode: String,
    pub config_keys: Vec<String>,
    pub source: ConfigSource,
    pub config_path: Option<PathBuf>,
    pub diagnostics: Vec<ConfigDiagnostic>,
    pub revision: u64,
}
impl ConfigInspectorData {
    pub fn is_set(&self, key: &str, effective_env: &BTreeMap<String, String>) -> bool {
        effective_env
            .get(key)
            .is_some_and(|value| !value.is_empty())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub config: Option<Value>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct LoadConfigResult {
    pub success: bool,
    pub config: Option<Value>,
    pub error: Option<String>,
    pub path: PathBuf,
}
