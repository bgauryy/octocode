mod fields;
mod inspector;
mod layers;
use super::dotenv::{apply_env, merged_env};
use super::types::*;
use super::validation::get_path;
pub(super) use fields::parse_candidate;
use fields::resolve_fields;
#[cfg(test)]
pub(crate) use fields::resolve_sections;
pub(crate) use inspector::inspector_data;
use layers::{EnvFiles, LayerState, config_source, drop_removed_env, load_layer};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

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

#[cfg(test)]
mod tests;
