//! What one setting is right now: the value that applies, the layer holding
//! it and the default it falls back to. A credential's value is never included.
use super::dotenv::workspace_may_narrow;
use super::manage::{shown, shown_url};
use super::resolver::workspace_file_allowed;
use super::{
    CONFIG_FIELDS, ConfigFieldKind, ConfigFieldSpec, ConfigOutput, KNOWN_ENV, KnownEnv,
    PROTECTED_KEYS, SettingSource, get_config_value,
};
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;

impl ConfigFieldSpec {
    /// The default as a reader sees it: what unset means, or the literal
    /// default value. `null` only for a setting with neither.
    #[must_use]
    pub fn default_value(&self) -> serde_json::Value {
        self.display_default.map_or_else(
            || serde_json::from_str(self.default_json).unwrap_or(serde_json::Value::Null),
            serde_json::Value::from,
        )
    }
}

/// One setting's effective state, for `config get` and `config list`.
#[derive(Debug, Serialize)]
pub struct SettingReport {
    /// The `.octocoderc` path; `None` for an entry only the environment sets.
    pub setting: Option<&'static str>,
    /// The environment variable that sets it, the name `config get` reports.
    pub key: Option<&'static str>,
    /// The value that applies; for a credential only a masked placeholder.
    /// `None` when nothing sets it and its default has no literal value.
    pub value: Option<Value>,
    /// Whether a layer sets it: `false` means the default applies.
    pub set: bool,
    /// `env`, `home`, `workspace` or `default`.
    pub source: &'static str,
    /// The file holding the value; `None` for the process or the default.
    pub path: Option<PathBuf>,
    pub default: Value,
    /// Whether a workspace may set it: `yes`, `no`, or `only <value>`.
    pub workspace: String,
    pub credential: bool,
}

/// Whether a workspace `.env` or `.octocoderc` may set `field`: fully, not at
/// all, or only to the one value that narrows it.
fn workspace_policy(field: &ConfigFieldSpec) -> String {
    if workspace_file_allowed(field) {
        return "yes".into();
    }
    let candidates: &[&str] = match field.kind {
        ConfigFieldKind::Boolean => &["true", "false"],
        _ => field.values,
    };
    field
        .env_name()
        .and_then(|name| {
            candidates
                .iter()
                .find(|value| workspace_may_narrow(name, value))
        })
        .map_or_else(|| "no".into(), |value| format!("only {value}"))
}

fn credential_report(config: &ConfigOutput, field: &'static ConfigFieldSpec) -> SettingReport {
    let name = field.env_name();
    let set = name
        .and_then(|name| config.env_value(name))
        .is_some_and(|value| !value.trim().is_empty());
    let origin = name.and_then(|name| config.env_origins.get(name));
    let source = match (set, origin) {
        (false, _) => "default",
        (true, Some(origin)) => origin.scope.as_str(),
        (true, None) => "env",
    };
    SettingReport {
        setting: Some(field.path),
        key: name,
        value: set.then(|| {
            Value::from(octocode_engine::security::redaction_placeholder(
                name.unwrap_or(field.path),
            ))
        }),
        set,
        source,
        path: origin.filter(|_| set).map(|origin| origin.path.clone()),
        default: field.default_value(),
        workspace: workspace_policy(field),
        credential: true,
    }
}

/// The report for `field` under `config`.
#[must_use]
pub fn setting_report(config: &ConfigOutput, field: &'static ConfigFieldSpec) -> SettingReport {
    if field.credential {
        return credential_report(config, field);
    }
    let name = field.env_name();
    let env_path = || {
        name.and_then(|name| config.env_origins.get(name))
            .map(|origin| origin.path.clone())
    };
    let (source, path) = match config.setting_source(field.path) {
        SettingSource::Default => ("default", None),
        SettingSource::Process => ("env", None),
        SettingSource::HomeEnv => ("home", env_path()),
        SettingSource::WorkspaceEnv => ("workspace", env_path()),
        SettingSource::HomeFile => ("home", config.config_path.clone()),
        SettingSource::WorkspaceFile => ("workspace", config.project_config_path.clone()),
    };
    SettingReport {
        setting: Some(field.path),
        key: name,
        value: shown(field, get_config_value(&config.resolved, field.path))
            .filter(|value| !value.is_null()),
        set: source != "default",
        source,
        path,
        default: field.default_value(),
        workspace: workspace_policy(field),
        credential: false,
    }
}

/// The report for an entry only the environment sets.
#[must_use]
pub fn known_env_report(config: &ConfigOutput, known: &'static KnownEnv) -> SettingReport {
    let raw = config
        .env_value(known.name)
        .filter(|value| !value.trim().is_empty());
    let origin = raw.and(config.env_origins.get(known.name));
    SettingReport {
        setting: None,
        key: Some(known.name),
        value: raw.map(|value| {
            if known.credential {
                Value::from(octocode_engine::security::redaction_placeholder(known.name))
            } else if known.kind == "url" {
                shown_url(Value::from(value))
            } else {
                Value::from(value)
            }
        }),
        set: raw.is_some(),
        source: match (raw, origin) {
            (None, _) => "default",
            (Some(_), Some(origin)) => origin.scope.as_str(),
            (Some(_), None) => "env",
        },
        path: origin.map(|origin| origin.path.clone()),
        default: Value::from(known.default),
        workspace: if PROTECTED_KEYS.contains(&known.name) {
            "no".into()
        } else {
            "yes".into()
        },
        credential: known.credential,
    }
}

/// A report for everything a reader can configure, settings first.
#[must_use]
pub fn setting_reports(config: &ConfigOutput) -> Vec<SettingReport> {
    CONFIG_FIELDS
        .iter()
        .map(|field| setting_report(config, field))
        .chain(
            KNOWN_ENV
                .iter()
                .map(|known| known_env_report(config, known)),
        )
        .collect()
}
