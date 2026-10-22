use super::dotenv::{MergedEnv, apply_env, merged_env, parse_boolean_env, parse_string_array_env};
use super::loader::load_config;
use super::types::*;
use super::validation::{config_issues, get_path, is_http_url, is_local_path};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn normalize_string(value: &str, normalize: Option<ConfigNormalize>) -> String {
    let trimmed = value.trim();
    match normalize {
        Some(ConfigNormalize::Lower) => trimmed.to_ascii_lowercase(),
        Some(ConfigNormalize::Trim) | None => trimmed.to_owned(),
    }
}

/// A setting's value from one raw source, or `None` when the source does not
/// hold a valid value for it (`null` included: it means "use the next source").
pub(super) fn parse_candidate(
    field: &ConfigFieldSpec,
    raw: &Value,
    from_environment: bool,
    normalize: Option<ConfigNormalize>,
) -> Option<Value> {
    match field.kind {
        ConfigFieldKind::Boolean => {
            let value = if from_environment {
                parse_boolean_env(raw.as_str())
            } else {
                raw.as_bool()
            }?;
            Some(Value::Bool(value))
        }
        ConfigFieldKind::StringArray => {
            let value = if from_environment {
                parse_string_array_env(raw.as_str())
                    .map(|values| Value::Array(values.into_iter().map(Value::String).collect()))
            } else if raw
                .as_array()
                .is_some_and(|values| values.iter().all(Value::is_string))
            {
                Some(raw.clone())
            } else {
                None
            }?;
            if field.item_path
                && value.as_array().is_some_and(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .any(|item| !is_local_path(item))
                })
            {
                return None;
            }
            Some(value)
        }
        ConfigFieldKind::Enum => {
            let value = normalize_string(raw.as_str()?, normalize);
            field
                .values
                .contains(&value.as_str())
                .then(|| Value::String(value))
        }
        ConfigFieldKind::String | ConfigFieldKind::Url | ConfigFieldKind::Path => {
            let value = if from_environment {
                normalize_string(raw.as_str()?, normalize)
            } else {
                raw.as_str()?.to_owned()
            };
            if value.trim().is_empty()
                || (field.kind == ConfigFieldKind::Url && !is_http_url(&value))
                || (field.kind == ConfigFieldKind::Path && !is_local_path(&value))
            {
                return None;
            }
            Some(Value::String(value))
        }
    }
}

/// `~` and `~/rest` (or `~\rest`) against the OS home; other paths unchanged.
fn expand_home_path(path: &str, os_home: &Path) -> String {
    let rest = if path == "~" {
        Some("")
    } else {
        path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\"))
    };
    match rest {
        Some("") => os_home.to_string_lossy().into_owned(),
        Some(rest) => os_home.join(rest).to_string_lossy().into_owned(),
        None => path.to_owned(),
    }
}

/// Path settings expand `~` once, here, with the home the path policy uses;
/// no reader sees a home-relative path.
fn expand_home(field: &ConfigFieldSpec, value: Value, os_home: &Path) -> Value {
    match (field.kind, value) {
        (ConfigFieldKind::Path, Value::String(path)) => {
            Value::String(expand_home_path(&path, os_home))
        }
        (ConfigFieldKind::StringArray, Value::Array(items)) if field.item_path => Value::Array(
            items
                .into_iter()
                .map(|item| match item {
                    Value::String(path) => Value::String(expand_home_path(&path, os_home)),
                    other => other,
                })
                .collect(),
        ),
        (_, value) => value,
    }
}

/// Resolve every contract field. `files` are `.octocoderc` layers in priority
/// order (workspace before global); for each field the environment wins, then
/// the first layer holding a valid value, then the generated default.
#[cfg(test)]
pub(crate) fn resolve_sections(
    files: &[&Value],
    environment: &BTreeMap<String, String>,
) -> Result<ResolvedConfig, String> {
    let mut errors = vec![];
    let (resolved, _) = resolve_fields(
        files,
        environment,
        Path::new("/synthetic-home"),
        &mut |_, _| {},
        &mut |error| errors.push(error),
    );
    if errors.is_empty() {
        Ok(resolved)
    } else {
        Err(errors.join("; "))
    }
}

/// `on_invalid_env(field, variable)` fires for a nonblank environment value
/// that the field rejects; the value is skipped, never fatal. `on_type_error`
/// reports a selected value the generated type rejects: that field keeps its
/// contract default and every other field resolves as usual.
fn resolve_fields(
    files: &[&Value],
    environment: &BTreeMap<String, String>,
    os_home: &Path,
    on_invalid_env: &mut dyn FnMut(&ConfigFieldSpec, &str),
    on_type_error: &mut dyn FnMut(String),
) -> (ResolvedConfig, BTreeMap<&'static str, Pick>) {
    let mut resolved = ResolvedConfig::default();
    let mut picks = BTreeMap::new();
    for field in CONFIG_FIELDS.iter().filter(|field| field.resolved) {
        let mut selected = None;
        if let Some(binding) = field.env
            && let Some(raw) = environment.get(binding.name)
        {
            let raw = Value::String(raw.clone());
            if let Some(value) = parse_candidate(field, &raw, true, binding.normalize) {
                selected = Some((value, Pick::Env));
            } else if raw.as_str().is_some_and(|raw| !raw.trim().is_empty()) {
                on_invalid_env(field, binding.name);
            }
        }

        if selected.is_none() && field.file {
            selected = files.iter().enumerate().find_map(|(index, config)| {
                get_path(config, field.path)
                    .and_then(|raw| parse_candidate(field, raw, false, None))
                    .map(|value| (value, Pick::File(index)))
            });
        }

        if let Some((value, pick)) = selected {
            match resolved.set_json(field.path, expand_home(field, value, os_home)) {
                Ok(()) => {
                    picks.insert(field.path, pick);
                }
                Err(error) => on_type_error(error),
            }
        }
    }
    (resolved, picks)
}

/// Where `resolve_fields` took a setting from: its variable, or the file
/// layer at this index.
#[derive(Clone, Copy)]
enum Pick {
    Env,
    File(usize),
}
pub fn resolve_env_token(e: &BTreeMap<String, String>) -> Option<PrivateTokenSelection> {
    for k in ENV_TOKEN_VARS.iter().copied() {
        if let Some(v) = e.get(k).map(|s| s.trim()).filter(|s| !s.is_empty()) {
            return Some(PrivateTokenSelection::new(v.into(), format!("env:{k}")));
        }
    }
    None
}
/// Apply trusted file fallbacks for fields explicitly marked as credentials.
/// They are written only to the effective child environment and never enter
/// `ResolvedConfig`, so inspection and `config get` cannot print their values.
/// An explicit blank classification key is an opt-out, not a missing value.
fn effective_disables_classification(effective: &BTreeMap<String, String>, key: &str) -> bool {
    super::dotenv::is_classification_kill_switch(key)
        && effective
            .get(key)
            .is_some_and(|value| value.trim().is_empty())
}

fn apply_credential_file_fallbacks(
    layers: &[(&Value, &PathBuf, ConfigScope)],
    effective: &mut BTreeMap<String, String>,
    origins: &mut BTreeMap<String, EnvOrigin>,
) {
    for field in CONFIG_FIELDS.iter().filter(|field| field.credential) {
        let Some(binding) = field.env else {
            continue;
        };
        if effective
            .get(binding.name)
            .is_some_and(|value| !value.trim().is_empty())
            || effective_disables_classification(effective, binding.name)
        {
            continue;
        }
        if let Some((value, path, scope)) = layers.iter().find_map(|(file, path, scope)| {
            get_path(file, field.path)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| (value, *path, *scope))
        }) {
            effective.insert(binding.name.to_owned(), value.to_owned());
            origins.insert(
                binding.name.to_owned(),
                EnvOrigin {
                    scope,
                    path: path.clone(),
                },
            );
        }
    }
}

/// A workspace `.octocoderc` may set a field only when its environment
/// bindings may come from a workspace `.env` — the same trust boundary, so a
/// checked-out repository gains no power through one file it lacks via the
/// other.
pub(super) fn workspace_file_allowed(field: &ConfigFieldSpec) -> bool {
    !field
        .env
        .is_some_and(|binding| PROTECTED_KEYS.contains(&binding.name))
}

/// A workspace file may still set a protected field to a value its
/// environment binding may narrow to (`storage.mode: memory`): protection
/// stops it widening, not opting out.
pub(super) fn workspace_setting_allowed(field: &ConfigFieldSpec, value: &Value) -> bool {
    let scalar = match value {
        Value::String(value) => Some(value.as_str()),
        Value::Bool(true) => Some("true"),
        Value::Bool(false) => Some("false"),
        _ => None,
    };
    workspace_file_allowed(field)
        || scalar.is_some_and(|value| {
            field
                .env
                .is_some_and(|binding| super::dotenv::workspace_may_narrow(binding.name, value))
        })
}

pub(super) fn remove_path(root: &mut Value, field_path: &str) -> bool {
    let (parents, key) = field_path
        .rsplit_once('.')
        .map_or(("", field_path), |(parents, key)| (parents, key));
    let mut current = root;
    for part in parents.split('.').filter(|part| !part.is_empty()) {
        let Some(next) = current.as_object_mut().and_then(|o| o.get_mut(part)) else {
            return false;
        };
        current = next;
    }
    current
        .as_object_mut()
        .is_some_and(|object| object.remove(key).is_some())
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum LayerState {
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
fn load_layer(
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
fn drop_removed_env(
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

/// The tool name `name` most likely meant, when one is close
/// (case-insensitive edit distance).
pub(crate) fn nearest_tool_name(name: &str) -> Option<&'static str> {
    let name = name.to_ascii_lowercase();
    crate::id::ToolId::ALL
        .into_iter()
        .map(|id| {
            let candidate = id.as_str();
            let distance = crate::contracts::levenshtein(&name, &candidate.to_ascii_lowercase());
            (distance, candidate)
        })
        .filter(|(distance, candidate)| *distance <= 3 && *distance * 3 <= candidate.len())
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate)
}

/// The tool-list settings: each names tools by their wire name.
const TOOL_LIST_FIELDS: [&ConfigFieldSpec; 3] = [
    field::TOOLS_ENABLED,
    field::TOOLS_DISABLED,
    field::MCP_DEFERRED,
];

/// A warning per tool-list setting that names a tool no tool has. Non-fatal:
/// the name is ignored, but a typo in an allowlist would otherwise leave no
/// tools without a word.
fn unknown_tool_names(
    resolved: &ResolvedConfig,
    source_of: &dyn Fn(&ConfigFieldSpec) -> Option<PathBuf>,
) -> Vec<ConfigDiagnostic> {
    TOOL_LIST_FIELDS
        .iter()
        .filter_map(|field| {
            let names = get_config_value(resolved, field.path)?;
            let unknown: Vec<String> = names
                .as_array()?
                .iter()
                .filter_map(Value::as_str)
                .filter(|name| crate::id::ToolId::from_name(name).is_none())
                .map(|name| match nearest_tool_name(name) {
                    Some(nearest) => format!("\"{name}\" (did you mean \"{nearest}\"?)"),
                    None => format!("\"{name}\""),
                })
                .collect();
            (!unknown.is_empty()).then(|| ConfigDiagnostic {
                severity: Severity::Warning,
                code: UNKNOWN_TOOL_NAME_CODE.into(),
                field_path: Some(field.path.into()),
                message: format!(
                    "{} names no tool: {}; the name is ignored",
                    field.display_name(),
                    unknown.join(", ")
                ),
                source_path: source_of(field),
            })
        })
        .collect()
}

/// The `.env` file that set each applied variable; `None` is the process
/// environment.
struct EnvFiles<'a> {
    dotenv: &'a EnvApplyReport,
    input: &'a ConfigInput,
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

    fn origins(&self) -> BTreeMap<String, EnvOrigin> {
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

    fn invalid(&self, field: &ConfigFieldSpec, name: &str) -> ConfigDiagnostic {
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
    fn setting_sources(
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
fn config_source(states: [LayerState; 2], has_env: bool) -> ConfigSource {
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

/// Credential-bound settings are forwarded through the environment, not
/// resolved; their environment values are still validated here, so an
/// invalid one is reported and skipped (a file value may then apply).
fn drop_invalid_credential_env(
    files: &EnvFiles<'_>,
    effective: &mut BTreeMap<String, String>,
    env_origins: &mut BTreeMap<String, EnvOrigin>,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) {
    for field in CONFIG_FIELDS.iter().filter(|field| field.credential) {
        let Some(binding) = field.env else { continue };
        let Some(raw) = effective
            .get(binding.name)
            .filter(|value| !value.trim().is_empty())
        else {
            continue;
        };
        if parse_candidate(field, &Value::String(raw.clone()), true, binding.normalize).is_none() {
            diagnostics.push(files.invalid(field, binding.name));
            effective.remove(binding.name);
            env_origins.remove(binding.name);
        }
    }
}

pub fn resolve_config(input: &ConfigInput) -> ConfigOutput {
    fn text(file: &FileInput) -> Option<&str> {
        match file {
            FileInput::Read { text, .. } => Some(text),
            _ => None,
        }
    }
    let mut merged = merged_env(text(&input.global_env), text(&input.project_env));
    let mut effective = input.env.clone();
    let removed_env = drop_removed_env(&mut merged, &mut effective, input);
    let dotenv = apply_env(merged, &mut effective);
    let mut diagnostics = vec![];
    let (project_file, project_state) =
        load_layer(&input.project_config_file, true, &mut diagnostics);
    let (global_file, global_state) = load_layer(&input.config_file, false, &mut diagnostics);
    let layers: Vec<(&Value, &PathBuf, ConfigScope)> = project_file
        .iter()
        .map(|file| {
            (
                file,
                input.project_config_file.path(),
                ConfigScope::Workspace,
            )
        })
        .chain(
            global_file
                .iter()
                .map(|file| (file, input.config_file.path(), ConfigScope::Home)),
        )
        .collect();
    let files: Vec<&Value> = layers.iter().map(|(file, _, _)| *file).collect();
    let env_files = EnvFiles {
        dotenv: &dotenv,
        input,
    };
    let mut env_origins = env_files.origins();
    let has_env = CONFIG_SOURCE_ENV_KEYS
        .iter()
        .any(|key| effective.contains_key(*key));
    let source = config_source([project_state, global_state], has_env);
    diagnostics.extend(removed_env);
    drop_invalid_credential_env(
        &env_files,
        &mut effective,
        &mut env_origins,
        &mut diagnostics,
    );
    apply_credential_file_fallbacks(&layers, &mut effective, &mut env_origins);
    let mut env_warnings = vec![];
    let mut type_errors = vec![];
    let (resolved, picks) = resolve_fields(
        &files,
        &effective,
        &input.os_home,
        &mut |field, name| env_warnings.push(env_files.invalid(field, name)),
        &mut |message| {
            type_errors.push(ConfigDiagnostic {
                severity: Severity::Error,
                code: "generated_config_contract_error".into(),
                field_path: None,
                message,
                source_path: None,
            });
        },
    );
    diagnostics.extend(type_errors);
    diagnostics.extend(env_warnings);
    let setting_sources = env_files.setting_sources(picks, &layers);
    let source_path = |field: &ConfigFieldSpec| -> Option<PathBuf> {
        match setting_sources.get(field.path)? {
            SettingSource::Process | SettingSource::Default => None,
            SettingSource::HomeEnv => Some(input.global_env.path().clone()),
            SettingSource::WorkspaceEnv => Some(input.project_env.path().clone()),
            SettingSource::HomeFile => Some(input.config_file.path().clone()),
            SettingSource::WorkspaceFile => Some(input.project_config_file.path().clone()),
        }
    };
    diagnostics.extend(unknown_tool_names(&resolved, &source_path));
    let token = resolve_env_token(&effective);
    let config_path =
        (global_state != LayerState::Absent).then(|| input.config_file.path().clone());
    let project_config_path =
        (project_state != LayerState::Absent).then(|| input.project_config_file.path().clone());
    ConfigOutput {
        home: super::octocode_home(&input.env, &input.cwd, &input.os_home),
        resolved,
        effective_env: effective,
        env_origins,
        setting_sources,
        dotenv,
        diagnostics,
        token,
        source,
        config_path,
        project_config_path,
    }
}
pub(crate) fn get_config_value(resolved: &ResolvedConfig, path: &str) -> Option<Value> {
    if path.is_empty() {
        return None;
    }
    get_path(&serde_json::to_value(resolved).ok()?, path).cloned()
}
pub(crate) fn is_stats_enabled(resolved: &ResolvedConfig) -> bool {
    is_persistent_storage_enabled(resolved) && resolved.storage.stats
}
pub fn is_persistent_storage_enabled(resolved: &ResolvedConfig) -> bool {
    resolved.storage.mode == StorageMode::Persistent
}
pub(crate) fn inspector_data(input: &ConfigInput, output: &ConfigOutput) -> ConfigInspectorData {
    let home = output.home.clone();
    let top_level_keys = |file: &FileInput| -> Vec<String> {
        let mut keys: Vec<String> = load_config(file)
            .config
            .and_then(|v| v.as_object().map(|o| o.keys().cloned().collect()))
            .unwrap_or_default();
        keys.sort();
        keys
    };
    let skip_source = |key: &String| EnvSkip {
        key: key.clone(),
        source_path: match output.dotenv.sources.get(key).map(String::as_str) {
            Some("project") => input.project_env.path().clone(),
            _ => input.global_env.path().clone(),
        },
    };
    ConfigInspectorData {
        home,
        global_env_path: input.global_env.path().clone(),
        project_env_path: input.project_env.path().clone(),
        project_env_exists: !matches!(input.project_env, FileInput::Missing { .. }),
        loaded_keys: output.dotenv.keys.clone(),
        skipped_protected: output
            .dotenv
            .skipped_protected
            .iter()
            .map(skip_source)
            .chain(
                output
                    .dotenv
                    .skipped_workspace_protected
                    .iter()
                    .map(|key| EnvSkip {
                        key: key.clone(),
                        source_path: input.project_env.path().clone(),
                    }),
            )
            .collect(),
        skipped_existing: output
            .dotenv
            .skipped_existing
            .iter()
            .map(skip_source)
            .collect(),
        global_key_count: output
            .dotenv
            .sources
            .values()
            .filter(|s| s.as_str() == "global")
            .count(),
        project_key_count: output
            .dotenv
            .sources
            .values()
            .filter(|s| s.as_str() == "project")
            .count(),
        storage_mode: output.resolved.storage.mode.as_str().to_owned(),
        config_keys: top_level_keys(&input.config_file),
        source: output.source.clone(),
        config_path: output.config_path.clone(),
        project_config_file: input.project_config_file.path().clone(),
        project_config_path: output.project_config_path.clone(),
        project_config_keys: top_level_keys(&input.project_config_file),
        diagnostics: output.diagnostics.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::{is_http_url, resolve_sections};
    use std::collections::BTreeMap;

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn blank_classification_key_in_env_is_an_opt_out_not_a_missing_value() {
        use super::{ConfigInput, FileInput, RuntimeSurface, resolve_config};
        let home = std::path::PathBuf::from("/synthetic/home");
        let input = |env: BTreeMap<String, String>| ConfigInput {
            env,
            cwd: "/synthetic/cwd".into(),
            os_home: "/synthetic".into(),
            global_env: FileInput::Read {
                path: home.join(".env"),
                text: "OCTOCODE_CLASSIFICATION_API=from-home-env".into(),
            },
            project_env: FileInput::Missing {
                path: "/synthetic/cwd/.octocode/.env".into(),
            },
            config_file: FileInput::Read {
                path: home.join(".octocoderc"),
                text: "{\"classification\":{\"api\":\"from-config-file\"}}".into(),
            },
            project_config_file: FileInput::Missing {
                path: "/synthetic/cwd/.octocode/.octocoderc".into(),
            },
            runtime_surface: RuntimeSurface::Mcp,
        };
        assert_eq!(
            resolve_config(&input(BTreeMap::new())).env_value("OCTOCODE_CLASSIFICATION_API"),
            Some("from-home-env")
        );
        for blank in ["", "  "] {
            let out = resolve_config(&input(env(&[("OCTOCODE_CLASSIFICATION_API", blank)])));
            assert_eq!(
                out.env_value("OCTOCODE_CLASSIFICATION_API").map(str::trim),
                Some(""),
                "blank {blank:?} must not be refilled from .env or .octocoderc"
            );
        }
    }

    #[test]
    fn is_http_url_accepts_http_and_https_only() {
        assert!(is_http_url("https://ghe.internal/api/v3"));
        assert!(is_http_url("http://127.0.0.1:8080/api/v3"));
        assert!(!is_http_url("ftp://evil.example"));
        assert!(!is_http_url("file:///etc/passwd"));
        assert!(!is_http_url("not a url"));
    }

    #[test]
    fn non_http_env_api_url_falls_back_to_default() {
        let resolved = resolve_sections(&[], &env(&[("GITHUB_API_URL", "ftp://evil.example")]));
        assert!(resolved.is_ok());
        assert_eq!(
            resolved.unwrap_or_default().github.api_url,
            "https://api.github.com"
        );
    }

    #[test]
    fn https_env_api_url_is_honored() {
        let resolved = resolve_sections(
            &[],
            &env(&[("GITHUB_API_URL", "https://ghe.internal/api/v3")]),
        );
        assert!(resolved.is_ok());
        assert_eq!(
            resolved.unwrap_or_default().github.api_url,
            "https://ghe.internal/api/v3"
        );
    }
}
