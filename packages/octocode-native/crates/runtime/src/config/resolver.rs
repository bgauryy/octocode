use super::dotenv::{
    apply_env, merged_env, parse_boolean_env, parse_int_env, parse_string_array_env,
};
use super::loader::load_config;
use super::types::*;
use super::validation::config_issues;
use serde_json::{Number, Value, json};
use std::collections::BTreeMap;
use std::path::Path;

fn get_path<'a>(root: &'a Value, field_path: &str) -> Option<&'a Value> {
    let mut current = root;
    for part in field_path.split('.') {
        current = current.as_object()?.get(part)?;
    }
    Some(current)
}

fn set_path(root: &mut Value, field_path: &str, value: Value) -> Result<(), String> {
    let parts = field_path.split('.').collect::<Vec<_>>();
    let mut current = root;
    for part in &parts[..parts.len().saturating_sub(1)] {
        let parent = current
            .as_object_mut()
            .ok_or_else(|| format!("generated default parent for {field_path} is not an object"))?;
        current = parent
            .get_mut(*part)
            .ok_or_else(|| format!("generated default is missing section {part}"))?;
    }
    let key = parts
        .last()
        .ok_or_else(|| "generated field path is empty".to_owned())?;
    current
        .as_object_mut()
        .ok_or_else(|| format!("generated default parent for {field_path} is not an object"))?
        .insert((*key).to_owned(), value);
    Ok(())
}

fn is_http_url(value: &str) -> bool {
    matches!(url::Url::parse(value), Ok(url) if matches!(url.scheme(), "http" | "https"))
}

fn is_local_path(value: &str) -> bool {
    let windows_absolute = value.as_bytes().get(1) == Some(&b':')
        && value
            .as_bytes()
            .get(2)
            .is_some_and(|separator| matches!(separator, b'/' | b'\\'));
    let absolute_or_home = Path::new(value).is_absolute()
        || value == "~"
        || value.starts_with("~/")
        || value.starts_with("~\\")
        || windows_absolute;
    absolute_or_home && !value.split(['/', '\\']).any(|part| part == "..")
}

fn normalize_string(value: &str, normalize: Option<ConfigNormalize>) -> String {
    let trimmed = value.trim();
    match normalize {
        Some(ConfigNormalize::Lower) => trimmed.to_ascii_lowercase(),
        Some(ConfigNormalize::Trim) | None => trimmed.to_owned(),
    }
}

fn parse_candidate(
    field: &ConfigFieldSpec,
    raw: &Value,
    from_environment: bool,
    normalize: Option<ConfigNormalize>,
) -> Option<Value> {
    match field.kind {
        ConfigFieldKind::SchemaVersion => raw
            .as_i64()
            .or_else(|| raw.as_u64().and_then(|value| i64::try_from(value).ok()))
            .map(Value::from),
        ConfigFieldKind::Boolean => {
            let value = if from_environment {
                parse_boolean_env(raw.as_str())
            } else {
                raw.as_bool()
            }?;
            Some(Value::Bool(value))
        }
        ConfigFieldKind::Number => {
            let value = if from_environment {
                parse_int_env(raw.as_str()).map(|value| value as f64)
            } else {
                // Truncate file-sourced numbers toward zero to match the JS
                // resolver's `Math.trunc`; config number fields are integers, so
                // `.octocoderc` `maxRetries: 2.5` must resolve to 2 on both
                // engines, not 2 (JS) vs 2.5 (Rust).
                raw.as_f64().map(f64::trunc)
            }?;
            let clamped = value.max(field.minimum?).min(field.maximum?);
            Number::from_f64(clamped).map(Value::Number)
        }
        ConfigFieldKind::StringArray => {
            let value = if from_environment {
                parse_string_array_env(raw.as_str())
                    .map(|values| Value::Array(values.into_iter().map(Value::String).collect()))
            } else if raw.is_null()
                || raw
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
/// Resolve every contract field. `files` are `.octocoderc` layers in priority
/// order (workspace before global); for each field the environment wins, then
/// the first layer holding a valid value, then the generated default.
pub fn resolve_sections(
    files: &[&Value],
    environment: &BTreeMap<String, String>,
) -> Result<ResolvedConfig, String> {
    resolve_fields(files, environment, &mut |_, _| {})
}

/// `on_invalid_env(field, variable)` fires for a nonblank environment value
/// that the field rejects; the value is skipped (or reset to the default when
/// the binding says so), never fatal.
fn resolve_fields(
    files: &[&Value],
    environment: &BTreeMap<String, String>,
    on_invalid_env: &mut dyn FnMut(&ConfigFieldSpec, &str),
) -> Result<ResolvedConfig, String> {
    let mut resolved: Value = serde_json::from_str(DEFAULT_RESOLVED_CONFIG_JSON)
        .map_err(|error| format!("generated config defaults are invalid: {error}"))?;

    for field in CONFIG_FIELDS.iter().filter(|field| field.resolved) {
        let default: Value = serde_json::from_str(field.default_json)
            .map_err(|error| format!("generated default for {} is invalid: {error}", field.path))?;
        let mut selected = None;
        for binding in field.env {
            let Some(raw) = environment.get(binding.name) else {
                continue;
            };
            let raw = Value::String(raw.clone());
            if let Some(value) = parse_candidate(field, &raw, true, binding.normalize) {
                selected = Some(value);
                break;
            }
            if raw.as_str().is_some_and(|raw| !raw.trim().is_empty()) {
                on_invalid_env(field, binding.name);
            }
            if binding.invalid == ConfigInvalidEnv::Default {
                selected = Some(default.clone());
                break;
            }
        }

        if selected.is_none() && field.file {
            selected = files
                .iter()
                .filter_map(|config| get_path(config, field.path))
                .find_map(|raw| parse_candidate(field, raw, false, None));
        }

        if selected.is_none() {
            selected = if let Some(source) = field.default_from {
                get_path(&resolved, source).cloned()
            } else {
                Some(default)
            };
        }
        let value = selected.ok_or_else(|| {
            format!(
                "generated default dependency for {} could not be resolved",
                field.path
            )
        })?;
        set_path(&mut resolved, field.path, value)?;
    }

    serde_json::from_value(resolved)
        .map_err(|error| format!("resolved config does not match generated types: {error}"))
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
    key == super::dotenv::CLASSIFICATION_KILL_SWITCH
        && effective
            .get(key)
            .is_some_and(|value| value.trim().is_empty())
}

fn apply_credential_file_fallbacks(files: &[&Value], effective: &mut BTreeMap<String, String>) {
    for field in CONFIG_FIELDS.iter().filter(|field| field.credential) {
        let Some(binding) = field.env.first() else {
            continue;
        };
        if field.env.iter().any(|alias| {
            effective
                .get(alias.name)
                .is_some_and(|value| !value.trim().is_empty())
        }) || effective_disables_classification(effective, binding.name)
        {
            continue;
        }
        if let Some(value) = files.iter().find_map(|file| {
            get_path(file, field.path)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        }) {
            effective.insert(binding.name.to_owned(), value.to_owned());
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
        .iter()
        .any(|binding| PROTECTED_KEYS.contains(&binding.name))
}

#[cfg(test)]
pub(super) fn insert_path(root: &mut Value, field_path: &str, value: Value) {
    let mut current = root;
    let mut parts = field_path.split('.').peekable();
    while let Some(part) = parts.next() {
        let object = current.as_object_mut().expect("object path");
        if parts.peek().is_none() {
            object.insert(part.to_owned(), value);
            return;
        }
        current = object.entry(part).or_insert_with(|| json!({}));
    }
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
        ConfigFieldKind::Number => "integer".into(),
        ConfigFieldKind::String => "nonblank string".into(),
        ConfigFieldKind::Url => "http(s) URL".into(),
        ConfigFieldKind::Path => "absolute or ~ path without ..".into(),
        ConfigFieldKind::StringArray if field.item_path => {
            "comma-separated list of absolute or ~ paths".into()
        }
        ConfigFieldKind::StringArray => "comma-separated list".into(),
        ConfigFieldKind::Enum => format!("value ({})", field.values.join(", ")),
        ConfigFieldKind::SchemaVersion => "schema version".into(),
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
    let (issues, warnings) = config_issues(&value);
    for m in warnings {
        diagnostics.push(warning("unknown_or_future_config", None, m, &load.path));
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
        for field in CONFIG_FIELDS
            .iter()
            .filter(|field| !workspace_file_allowed(field))
        {
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

pub fn resolve_config(input: &ConfigInput) -> ConfigOutput {
    let gt = match &input.global_env {
        FileInput::Read { text, .. } => Some(text.as_str()),
        _ => None,
    };
    let pt = match &input.project_env {
        FileInput::Read { text, .. } => Some(text.as_str()),
        _ => None,
    };
    // Workspace dotenv is configuration, independent of permission to execute
    // project-local language-server definitions (trusted_project).
    let (map, sources) = merged_env(gt, pt, true);
    let mut effective = input.env.clone();
    let dotenv = apply_env(&map, sources, &mut effective);
    let mut diagnostics = vec![];
    let (project_file, project_state) =
        load_layer(&input.project_config_file, true, &mut diagnostics);
    let (global_file, global_state) = load_layer(&input.config_file, false, &mut diagnostics);
    let files: Vec<&Value> = project_file.iter().chain(global_file.iter()).collect();
    let has_env = CONFIG_SOURCE_ENV_KEYS
        .iter()
        .any(|key| effective.contains_key(*key));
    let states = [project_state, global_state];
    let source = if states.contains(&LayerState::Invalid) {
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
    };
    apply_credential_file_fallbacks(&files, &mut effective);
    // Which file supplied an environment value; None = the process env.
    let env_source = |name: &str| -> Option<std::path::PathBuf> {
        if !dotenv.applied.iter().any(|key| key == name) {
            return None;
        }
        match dotenv.sources.get(name).map(String::as_str) {
            Some("project") => Some(input.project_env.path().clone()),
            _ => Some(input.global_env.path().clone()),
        }
    };
    let mut env_warnings = vec![];
    let resolved = match resolve_fields(&files, &effective, &mut |field, name| {
        let source_path = env_source(name);
        env_warnings.push(ConfigDiagnostic {
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
        })
    }) {
        Ok(config) => config,
        Err(message) => {
            diagnostics.push(ConfigDiagnostic {
                severity: Severity::Error,
                code: "generated_config_contract_error".into(),
                field_path: None,
                message,
                source_path: Some(input.config_file.path().clone()),
            });
            ResolvedConfig::default()
        }
    };
    diagnostics.extend(env_warnings);
    let token = resolve_env_token(&effective);
    let config_path =
        (global_state != LayerState::Absent).then(|| input.config_file.path().clone());
    let project_config_path =
        (project_state != LayerState::Absent).then(|| input.project_config_file.path().clone());
    let child_env = ChildEnvPlan {
        set: effective.clone(),
    };
    ConfigOutput {
        home: super::octocode_home(&input.env, &input.cwd, &input.os_home),
        resolved,
        effective_env: effective,
        dotenv,
        diagnostics,
        token,
        child_env,
        source,
        config_path,
        project_config_path,
        revision: input.revision,
    }
}
pub fn get_config_value(resolved: &ResolvedConfig, path: &str) -> Option<Value> {
    let value = serde_json::to_value(resolved).ok()?;
    if path.is_empty() {
        return None;
    }
    let mut cur = &value;
    for p in path.split('.') {
        cur = cur.as_object()?.get(p)?;
    }
    Some(cur.clone())
}
pub fn is_stats_enabled(resolved: &ResolvedConfig) -> bool {
    resolved.storage.mode == "persistent" && resolved.session.enable_stats
}
pub fn is_persistent_storage_enabled(resolved: &ResolvedConfig) -> bool {
    resolved.storage.mode == "persistent"
}
pub fn is_persistent_storage_enabled_for_extension(resolved: &ResolvedConfig) -> bool {
    resolved.extension.storage.mode == "persistent"
}
pub fn inspector_data(input: &ConfigInput, output: &ConfigOutput) -> ConfigInspectorData {
    let home = input
        .config_file
        .path()
        .parent()
        .unwrap_or(&input.os_home)
        .to_path_buf();
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
        loaded_keys: output.dotenv.keys.clone(),
        skipped_protected: output
            .dotenv
            .skipped_protected
            .iter()
            .map(skip_source)
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
        storage_mode: output.resolved.storage.mode.clone(),
        config_keys: top_level_keys(&input.config_file),
        source: output.source.clone(),
        config_path: output.config_path.clone(),
        project_config_file: input.project_config_file.path().clone(),
        project_config_path: output.project_config_path.clone(),
        project_config_keys: top_level_keys(&input.project_config_file),
        diagnostics: output.diagnostics.clone(),
        revision: output.revision,
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
            trusted_project: false,
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
            revision: 1,
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
