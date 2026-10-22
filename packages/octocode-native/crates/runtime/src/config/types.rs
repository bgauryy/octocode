use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigFieldKind {
    Boolean,
    String,
    Url,
    Path,
    StringArray,
    Enum,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigNormalize {
    Trim,
    Lower,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConfigEnumStyle {
    List,
    QuotedOr,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigEnvBinding {
    pub name: &'static str,
    pub normalize: Option<ConfigNormalize>,
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
    /// The one environment variable that sets this field, if any.
    pub env: Option<ConfigEnvBinding>,
    pub default_json: &'static str,
    pub values: &'static [&'static str],
    pub(crate) enum_style: ConfigEnumStyle,
    pub(crate) item_path: bool,
}

include!(concat!(env!("OUT_DIR"), "/config_contract.rs"));

impl ConfigFieldSpec {
    /// The environment variable that sets this field, if any.
    #[must_use]
    pub fn env_name(&self) -> Option<&'static str> {
        self.env.map(|binding| binding.name)
    }

    /// `path (ENV)`, or the bare path for a field no variable sets: how a
    /// message names the setting to change.
    #[must_use]
    pub(crate) fn display_name(&self) -> String {
        match self.env_name() {
            Some(env) => format!("{} ({env})", self.path),
            None => self.path.to_owned(),
        }
    }
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
    pub global_env: FileInput,
    pub project_env: FileInput,
    /// Global `<octocode-home>/.octocoderc`.
    pub config_file: FileInput,
    /// Workspace `<cwd>/.octocode/.octocoderc`; per field it overrides the
    /// global file, and both rank below every environment source.
    pub project_config_file: FileInput,
    pub runtime_surface: RuntimeSurface,
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
pub(crate) enum Severity {
    Warning,
    Error,
}
/// Diagnostic code of a configuration key no setting reads.
pub(crate) const UNKNOWN_CONFIG_CODE: &str = "unknown_or_future_config";
/// Diagnostic code of a setting or variable the contract removed.
pub(crate) const REMOVED_SETTING_CODE: &str = "removed_setting";
/// Diagnostic code of a tool-list entry no tool has.
pub(crate) const UNKNOWN_TOOL_NAME_CODE: &str = "unknown_tool_name";

/// Why the contract removed `name`: a removed variable, or a removed setting
/// path or a path inside a removed section (`output.pagination.x`).
#[must_use]
pub(crate) fn removed_reason(name: &str) -> Option<&'static str> {
    REMOVED_ENV_KEYS
        .iter()
        .chain(REMOVED_SETTINGS)
        .find(|(removed, _)| {
            name == *removed
                || name
                    .strip_prefix(removed)
                    .is_some_and(|rest| rest.starts_with('.'))
        })
        .map(|(_, reason)| *reason)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConfigDiagnostic {
    pub(crate) severity: Severity,
    pub code: String,
    pub field_path: Option<String>,
    pub message: String,
    pub source_path: Option<PathBuf>,
}
impl fmt::Display for ConfigDiagnostic {
    /// One self-contained line: where the problem is, then what and the effect.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity {
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        write!(f, "octocode: config {severity}")?;
        if let Some(path) = &self.source_path {
            write!(f, ": {}", path.display())?;
        }
        write!(f, ": {} [{}]", self.message, self.code)
    }
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct EnvApplyReport {
    pub applied: Vec<String>,
    pub skipped_protected: Vec<String>,
    pub skipped_existing: Vec<String>,
    /// Workspace `.env` keys dropped because they are protected and the home
    /// `.env` sets them (the home value stands).
    pub(crate) skipped_workspace_protected: Vec<String>,
    pub sources: BTreeMap<String, String>,
    pub keys: Vec<String>,
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

/// Which configuration scope a file belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigScope {
    Home,
    Workspace,
}
impl ConfigScope {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Workspace => "workspace",
        }
    }
}

/// The file an effective environment value came from: a `.env` file, or a
/// `.octocoderc` credential setting.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EnvOrigin {
    pub scope: ConfigScope,
    pub path: PathBuf,
}

/// Whether a variable holds a secret: a GitHub token, a credential setting's
/// variable, or a name with a credential word as one of its parts
/// (`SERVICE_API_KEY`, `DB_PASSWORD`). Secret values are never printed.
#[must_use]
pub fn is_secret_key(key: &str) -> bool {
    const CREDENTIAL_WORDS: &[&str] = &[
        "secret",
        "secrets",
        "password",
        "passwd",
        "passphrase",
        "key",
        "keys",
        "token",
        "tokens",
        "credential",
        "credentials",
    ];
    ENV_TOKEN_VARS.contains(&key)
        || CONFIG_FIELDS
            .iter()
            .any(|field| field.credential && field.env_name() == Some(key))
        || key
            .split(['_', '.', '-'])
            .any(|part| CREDENTIAL_WORDS.contains(&part.to_ascii_lowercase().as_str()))
}

/// The layer a resolved setting came from; a setting no layer sets is
/// `Default`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SettingSource {
    Process,
    WorkspaceEnv,
    HomeEnv,
    WorkspaceFile,
    HomeFile,
    Default,
}

#[derive(Clone)]
pub struct ConfigOutput {
    pub home: PathBuf,
    pub resolved: ResolvedConfig,
    pub(crate) effective_env: BTreeMap<String, String>,
    /// The file each effective variable a file supplied came from; a
    /// variable not listed came from the process environment.
    pub env_origins: BTreeMap<String, EnvOrigin>,
    /// The layer each resolved setting came from, by contract path; absent
    /// means the generated default.
    pub(crate) setting_sources: BTreeMap<&'static str, SettingSource>,
    pub dotenv: EnvApplyReport,
    pub diagnostics: Vec<ConfigDiagnostic>,
    pub token: Option<PrivateTokenSelection>,
    pub source: ConfigSource,
    /// Global `.octocoderc` path when that file exists (valid or not).
    pub config_path: Option<PathBuf>,
    /// Workspace `.octocoderc` path when that file exists (valid or not).
    pub project_config_path: Option<PathBuf>,
}
impl ConfigOutput {
    pub(crate) fn effective_env(&self) -> impl Iterator<Item = (&str, &str)> {
        self.effective_env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
    }
    pub fn env_value(&self, key: &str) -> Option<&str> {
        self.effective_env.get(key).map(String::as_str)
    }
    /// The layer the resolved setting at `path` came from.
    #[must_use]
    pub(crate) fn setting_source(&self, path: &str) -> SettingSource {
        self.setting_sources
            .get(path)
            .copied()
            .unwrap_or(SettingSource::Default)
    }
    /// A digest of what this configuration makes a runtime do: its home, the
    /// resolved settings, the selected credential (by digest), and the
    /// effective value of every variable the config contract declares. Two
    /// configurations with equal digests run tools alike, so a warm server
    /// keyed on it never answers under stale settings.
    #[must_use]
    pub fn identity_digest(&self) -> String {
        use std::fmt::Write as _;
        let mut identity = format!("{}\n{:?}\n", self.home.display(), self.resolved);
        if let Some(token) = &self.token {
            let _ = writeln!(
                identity,
                "token={}:{}",
                token.source(),
                octocode_engine::digest::sha256(token.token().as_bytes())
            );
        }
        for name in DECLARED_ENV_KEYS {
            if let Some(value) = self.effective_env.get(*name) {
                let _ = writeln!(identity, "{name}={value}");
            }
        }
        octocode_engine::digest::sha256(identity.as_bytes())
    }
    /// The effective environment without the values a workspace `.env`
    /// supplied: a repository-controlled file never steers credential
    /// discovery (which credential file is read, or what it expands to).
    pub(crate) fn credential_env(&self) -> BTreeMap<String, String> {
        let from_workspace = |key: &str| {
            self.dotenv.applied.iter().any(|applied| applied == key)
                && self.dotenv.sources.get(key).map(String::as_str) == Some("project")
        };
        self.effective_env
            .iter()
            .filter(|(key, _)| !from_workspace(key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
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
            .field("source", &self.source)
            .field("config_path", &self.config_path)
            .field("project_config_path", &self.project_config_path)
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
    /// Whether a workspace `.env` exists as its own layer: false when it is
    /// missing or is the home `.env` itself (cwd = the OS home).
    pub project_env_exists: bool,
    pub loaded_keys: Vec<String>,
    pub skipped_protected: Vec<EnvSkip>,
    pub skipped_existing: Vec<EnvSkip>,
    pub(crate) global_key_count: usize,
    pub(crate) project_key_count: usize,
    pub storage_mode: String,
    pub config_keys: Vec<String>,
    pub source: ConfigSource,
    pub config_path: Option<PathBuf>,
    /// Canonical workspace `.octocoderc` location, whether or not it exists.
    pub project_config_file: PathBuf,
    pub project_config_path: Option<PathBuf>,
    pub project_config_keys: Vec<String>,
    pub diagnostics: Vec<ConfigDiagnostic>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LoadConfigResult {
    pub success: bool,
    pub config: Option<Value>,
    pub error: Option<String>,
    pub path: PathBuf,
}
