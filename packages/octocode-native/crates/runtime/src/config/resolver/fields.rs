use super::super::dotenv::{parse_boolean_env, parse_string_array_env};
use super::super::types::*;
use super::super::validation::{get_path, is_http_url, is_local_path};
use super::Pick;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

fn normalize_string(value: &str, normalize: Option<ConfigNormalize>) -> String {
    let trimmed = value.trim();
    match normalize {
        Some(ConfigNormalize::Lower) => trimmed.to_ascii_lowercase(),
        Some(ConfigNormalize::Trim) | None => trimmed.to_owned(),
    }
}

/// A setting's value from one raw source, or `None` when the source does not
/// hold a valid value for it (`null` included: it means "use the next source").
pub(crate) fn parse_candidate(
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
pub(super) fn resolve_fields(
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
