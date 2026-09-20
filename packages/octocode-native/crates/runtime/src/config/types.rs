use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigFieldKind {
    Boolean,
    Number,
    String,
    Url,
    Path,
    StringArray,
    Enum,
    SchemaVersion,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigNormalize {
    Trim,
    Lower,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigInvalidEnv {
    Skip,
    Default,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigEnumStyle {
    List,
    QuotedOr,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigEnvBinding {
    pub name: &'static str,
    pub normalize: Option<ConfigNormalize>,
    pub invalid: ConfigInvalidEnv,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConfigFieldSpec {
    pub path: &'static str,
    pub section: &'static str,
    pub key: &'static str,
    pub kind: ConfigFieldKind,
    pub file: bool,
    pub resolved: bool,
    pub credential: bool,
    pub env: &'static [ConfigEnvBinding],
    pub default_json: &'static str,
    pub default_from: Option<&'static str>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub values: &'static [&'static str],
    pub enum_style: ConfigEnumStyle,
    pub item_path: bool,
}

include!(concat!(env!("OUT_DIR"), "/config_contract.rs"));

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

/// A dotenv key that was found in a `.env` file but not applied, with the
/// file it came from. Key name only — never the value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EnvSkip {
    pub key: String,
    pub source_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConfigInspectorData {
    pub home: PathBuf,
    pub global_env_path: PathBuf,
    pub project_env_path: PathBuf,
    pub loaded_keys: Vec<String>,
    pub skipped_protected: Vec<EnvSkip>,
    pub skipped_existing: Vec<EnvSkip>,
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
