use super::dotenv::{
    apply_env, merged_env, parse_boolean_env, parse_int_env, parse_string_array_env,
};
use super::loader::load_config;
use super::types::*;
use super::validation::validate_config;
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
pub fn resolve_sections(
    file: Option<&Value>,
    environment: &BTreeMap<String, String>,
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
            if binding.invalid == ConfigInvalidEnv::Default {
                selected = Some(default.clone());
                break;
            }
        }

        if selected.is_none()
            && field.file
            && let Some(raw) = file.and_then(|config| get_path(config, field.path))
        {
            selected = parse_candidate(field, raw, false, None);
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

fn apply_credential_file_fallbacks(file: Option<&Value>, effective: &mut BTreeMap<String, String>) {
    let Some(file) = file else { return };
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
        if let Some(value) = get_path(file, field.path)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            effective.insert(binding.name.to_owned(), value.to_owned());
        }
    }
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
    let load = load_config(&input.config_file);
    let mut diagnostics = vec![];
    let (file, state) = if load.success {
        let value = load.config.unwrap_or_else(|| json!({}));
        let v = validate_config(&value);
        for m in &v.warnings {
            diagnostics.push(ConfigDiagnostic {
                severity: Severity::Warning,
                code: "unknown_or_future_config".into(),
                field_path: None,
                message: m.clone(),
                source_path: Some(load.path.clone()),
            })
        }
        if v.valid {
            (Some(value), "valid")
        } else {
            for m in v.errors {
                diagnostics.push(ConfigDiagnostic {
                    severity: Severity::Error,
                    code: "invalid_config".into(),
                    field_path: None,
                    message: m,
                    source_path: Some(load.path.clone()),
                })
            }
            (None, "invalid")
        }
    } else if load.error.as_deref() == Some("Config file does not exist") {
        (None, "absent")
    } else {
        diagnostics.push(ConfigDiagnostic {
            severity: Severity::Error,
            code: "config_load_error".into(),
            field_path: None,
            message: load.error.unwrap_or_else(|| "Invalid configuration".into()),
            source_path: Some(load.path.clone()),
        });
        (None, "invalid")
    };
    let has_env = CONFIG_SOURCE_ENV_KEYS
        .iter()
        .any(|key| effective.contains_key(*key));
    let source = match state {
        "invalid" => ConfigSource::Invalid,
        "valid" if has_env => ConfigSource::Mixed,
        "valid" => ConfigSource::File,
        "absent" if has_env => ConfigSource::Env,
        _ => ConfigSource::Defaults,
    };
    apply_credential_file_fallbacks(file.as_ref(), &mut effective);
    let resolved = match resolve_sections(file.as_ref(), &effective) {
        Ok(config) => config,
        Err(message) => {
            diagnostics.push(ConfigDiagnostic {
                severity: Severity::Error,
                code: "generated_config_contract_error".into(),
                field_path: None,
                message,
                source_path: Some(load.path.clone()),
            });
            ResolvedConfig::default()
        }
    };
    let token = resolve_env_token(&effective);
    let config_path = (state != "absent").then(|| load.path.clone());
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
    let load = load_config(&input.config_file);
    let mut config_keys: Vec<String> = load
        .config
        .and_then(|v| v.as_object().cloned())
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    config_keys.sort();
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
        config_keys,
        source: output.source.clone(),
        config_path: output.config_path.clone(),
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
        let resolved = resolve_sections(None, &env(&[("GITHUB_API_URL", "ftp://evil.example")]));
        assert!(resolved.is_ok());
        assert_eq!(
            resolved.unwrap_or_default().github.api_url,
            "https://api.github.com"
        );
    }

    #[test]
    fn https_env_api_url_is_honored() {
        let resolved = resolve_sections(
            None,
            &env(&[("GITHUB_API_URL", "https://ghe.internal/api/v3")]),
        );
        assert!(resolved.is_ok());
        assert_eq!(
            resolved.unwrap_or_default().github.api_url,
            "https://ghe.internal/api/v3"
        );
    }
}
