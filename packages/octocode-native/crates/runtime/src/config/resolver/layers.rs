use super::super::dotenv::MergedEnv;
use super::super::loader::load_config;
use super::super::types::*;
use super::super::validation::{config_issues, get_path};
use super::{Pick, remove_path, workspace_setting_allowed};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum LayerState {
    Absent,
    Valid,
    Invalid,
}

fn expected_kind(field: &ConfigFieldSpec) -> String {
    match field.kind {
        ConfigFieldKind::Boolean => "boolean (true/false/1/0)".into(),
        ConfigFieldKind::String => "nonblank string".into(),
        ConfigFieldKind::Url => "http(s) URL".into(),
        ConfigFieldKind::Path => "absolute or ~ path without ..".into(),
        ConfigFieldKind::StringArray if field.item_path => {
            "comma-separated list of absolute or ~ paths".into()
        }
        ConfigFieldKind::StringArray => "comma-separated list".into(),
        ConfigFieldKind::Enum => format!("value ({})", field.values.join(", ")),
    }
}

fn warning(
    code: &str,
    field_path: Option<&str>,
    message: String,
    source_path: &std::path::Path,
) -> ConfigDiagnostic {
    ConfigDiagnostic {
        severity: Severity::Warning,
        code: code.into(),
        field_path: field_path.map(str::to_owned),
        message,
        source_path: Some(source_path.to_path_buf()),
    }
}

/// Load and validate one `.octocoderc` layer. Misconfiguration never fails
/// the runtime: an unreadable or unparseable file is skipped, and each invalid
/// value is dropped on its own so the rest of the file still applies. Every
/// skip is reported as a warning naming the file and the offending path.
pub(super) fn load_layer(
    input: &FileInput,
    workspace: bool,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) -> (Option<Value>, LayerState) {
    let load = load_config(input);
    if !load.success {
        if load.error.as_deref() == Some("Config file does not exist") {
            return (None, LayerState::Absent);
        }
        diagnostics.push(warning(
            "config_load_error",
            None,
            format!(
                "{}; the whole file is ignored",
                load.error.unwrap_or_else(|| "Invalid configuration".into())
            ),
            &load.path,
        ));
        return (None, LayerState::Invalid);
    }
    let mut value = load.config.unwrap_or_else(|| json!({}));
    for (path, reason) in REMOVED_SETTINGS {
        if remove_path(&mut value, path) {
            diagnostics.push(warning(
                REMOVED_SETTING_CODE,
                Some(path),
                format!("{path} was removed and is ignored: {reason}"),
                &load.path,
            ));
        }
    }
    let (issues, warnings) = config_issues(&value);
    for m in warnings {
        diagnostics.push(warning(UNKNOWN_CONFIG_CODE, None, m, &load.path));
    }
    for issue in issues {
        remove_path(&mut value, &issue.path);
        diagnostics.push(warning(
            "invalid_config",
            Some(&issue.path),
            format!("{}; value ignored", issue.message),
            &load.path,
        ));
    }
    if workspace {
        for field in CONFIG_FIELDS.iter() {
            if get_path(&value, field.path).is_none_or(|v| workspace_setting_allowed(field, v)) {
                continue;
            }
            if remove_path(&mut value, field.path) {
                diagnostics.push(warning(
                    "workspace_config_protected",
                    Some(field.path),
                    format!(
                        "{} is protected and ignored in a workspace config file; set it in the global config file or the process environment",
                        field.path
                    ),
                    &load.path,
                ));
            }
        }
    }
    (Some(value), LayerState::Valid)
}

/// Removed variables are reported once each, by the layer that names them
/// (the process environment first), and dropped from every layer, so no
/// reader, child process or `config get` sees their values.
pub(super) fn drop_removed_env(
    merged: &mut MergedEnv,
    effective: &mut BTreeMap<String, String>,
    input: &ConfigInput,
) -> Vec<ConfigDiagnostic> {
    REMOVED_ENV_KEYS
        .iter()
        .filter_map(|(name, reason)| {
            let process = effective.remove(*name).is_some();
            let file = merged.map.remove(*name).is_some();
            let source = merged.sources.remove(*name);
            (process || file).then(|| ConfigDiagnostic {
                severity: Severity::Warning,
                code: REMOVED_SETTING_CODE.into(),
                field_path: None,
                message: format!("{name} was removed and is ignored: {reason}"),
                source_path: (!process).then(|| match source.as_deref() {
                    Some("project") => input.project_env.path().clone(),
                    _ => input.global_env.path().clone(),
                }),
            })
        })
        .collect()
}

/// The `.env` file that set each applied variable; `None` is the process
/// environment.
pub(super) struct EnvFiles<'a> {
    pub(super) dotenv: &'a EnvApplyReport,
    pub(super) input: &'a ConfigInput,
}

impl EnvFiles<'_> {
    fn source(&self, name: &str) -> Option<PathBuf> {
        if !self.dotenv.applied.iter().any(|key| key == name) {
            return None;
        }
        match self.dotenv.sources.get(name).map(String::as_str) {
            Some("project") => Some(self.input.project_env.path().clone()),
            _ => Some(self.input.global_env.path().clone()),
        }
    }

    pub(super) fn origins(&self) -> BTreeMap<String, EnvOrigin> {
        self.dotenv
            .applied
            .iter()
            .map(|key| {
                let origin = match self.dotenv.sources.get(key).map(String::as_str) {
                    Some("project") => EnvOrigin {
                        scope: ConfigScope::Workspace,
                        path: self.input.project_env.path().clone(),
                    },
                    _ => EnvOrigin {
                        scope: ConfigScope::Home,
                        path: self.input.global_env.path().clone(),
                    },
                };
                (key.clone(), origin)
            })
            .collect()
    }

    pub(super) fn invalid(&self, field: &ConfigFieldSpec, name: &str) -> ConfigDiagnostic {
        let source_path = self.source(name);
        ConfigDiagnostic {
            severity: Severity::Warning,
            code: "invalid_env_value".into(),
            field_path: Some(field.path.into()),
            message: format!(
                "{name}{} is not a valid {} for {}; value ignored",
                if source_path.is_none() {
                    " (process environment)"
                } else {
                    ""
                },
                expected_kind(field),
                field.path
            ),
            source_path,
        }
    }

    /// Where each resolved setting came from.
    pub(super) fn setting_sources(
        &self,
        picks: BTreeMap<&'static str, Pick>,
        layers: &[(&Value, &PathBuf, ConfigScope)],
    ) -> BTreeMap<&'static str, SettingSource> {
        picks
            .into_iter()
            .map(|(path, pick)| {
                let source = match pick {
                    Pick::Env => {
                        let name = CONFIG_FIELDS
                            .iter()
                            .find(|field| field.path == path)
                            .and_then(ConfigFieldSpec::env_name)
                            .unwrap_or_default();
                        match self.source(name) {
                            None => SettingSource::Process,
                            Some(file) if &file == self.input.project_env.path() => {
                                SettingSource::WorkspaceEnv
                            }
                            Some(_) => SettingSource::HomeEnv,
                        }
                    }
                    Pick::File(index) => match layers[index].2 {
                        ConfigScope::Workspace => SettingSource::WorkspaceFile,
                        ConfigScope::Home => SettingSource::HomeFile,
                    },
                };
                (path, source)
            })
            .collect()
    }
}

/// The overall source: an invalid file, else files and/or environment, else
/// the generated defaults.
pub(super) fn config_source(states: [LayerState; 2], has_env: bool) -> ConfigSource {
    if states.contains(&LayerState::Invalid) {
        ConfigSource::Invalid
    } else if states.contains(&LayerState::Valid) {
        if has_env {
            ConfigSource::Mixed
        } else {
            ConfigSource::File
        }
    } else if has_env {
        ConfigSource::Env
    } else {
        ConfigSource::Defaults
    }
}
