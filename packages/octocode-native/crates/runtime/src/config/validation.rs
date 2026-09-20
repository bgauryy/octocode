use super::types::{
    CONFIG_FIELDS, CONFIG_SCHEMA_VERSION, ConfigEnumStyle, ConfigFieldKind, ConfigFieldSpec,
    ValidationResult,
};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn get_path<'a>(root: &'a Value, field_path: &str) -> Option<&'a Value> {
    let mut current = root;
    for part in field_path.split('.') {
        current = current.as_object()?.get(part)?;
    }
    Some(current)
}

fn section_paths() -> Vec<String> {
    let mut sections = BTreeSet::new();
    for field in CONFIG_FIELDS.iter().filter(|field| field.file) {
        let parts = field.section.split('.').collect::<Vec<_>>();
        for index in 0..parts.len() {
            let path = parts[..=index].join(".");
            if !path.is_empty() {
                sections.insert(path);
            }
        }
    }
    sections.into_iter().collect()
}

fn is_absolute_or_home_path(value: &str) -> bool {
    let windows_absolute = value.as_bytes().get(1) == Some(&b':')
        && value
            .as_bytes()
            .get(2)
            .is_some_and(|separator| matches!(separator, b'/' | b'\\'));
    value.starts_with('~') || Path::new(value).is_absolute() || windows_absolute
}

fn validate_path(field_path: &str, value: &str, errors: &mut Vec<String>) {
    if value.trim().is_empty() {
        errors.push(format!("{field_path}: empty or whitespace-only path"));
    } else if value.split(['/', '\\']).any(|part| part == "..") {
        errors.push(format!(
            "{field_path}: path traversal (..) not allowed (got \"{value}\")"
        ));
    } else if !is_absolute_or_home_path(value) {
        errors.push(format!(
            "{field_path}: must be absolute path or start with ~ (got \"{value}\")"
        ));
    }
}

fn validate_url(field_path: &str, value: &str, errors: &mut Vec<String>) {
    match url::Url::parse(value) {
        Ok(url) if matches!(url.scheme(), "http" | "https") => {}
        Ok(_) => errors.push(format!("{field_path}: Only http/https URLs allowed")),
        Err(_) => errors.push(format!("{field_path}: Invalid URL format")),
    }
}

fn validate_field(
    field: &ConfigFieldSpec,
    value: Option<&Value>,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    let Some(value) = value else { return };
    if value.is_null() && field.kind != ConfigFieldKind::SchemaVersion {
        return;
    }
    match field.kind {
        ConfigFieldKind::SchemaVersion => {
            let version = value
                .as_i64()
                .or_else(|| value.as_u64().and_then(|number| i64::try_from(number).ok()));
            match version {
                None => errors.push(format!("{}: Must be an integer", field.path)),
                Some(version) if version > CONFIG_SCHEMA_VERSION => warnings.push(format!(
                    "{}: Config version {version} is newer than supported version {CONFIG_SCHEMA_VERSION}",
                    field.path
                )),
                Some(_) => {}
            }
        }
        ConfigFieldKind::Boolean => {
            if !value.is_boolean() {
                errors.push(format!("{}: Must be a boolean", field.path));
            }
        }
        ConfigFieldKind::Number => match value.as_f64() {
            None => errors.push(format!("{}: Must be a number", field.path)),
            Some(number)
                if field.minimum.is_some_and(|minimum| number < minimum)
                    || field.maximum.is_some_and(|maximum| number > maximum) =>
            {
                errors.push(format!(
                    "{}: Must be between {:.0} and {:.0}",
                    field.path,
                    field.minimum.unwrap_or_default(),
                    field.maximum.unwrap_or_default()
                ));
            }
            Some(_) => {}
        },
        ConfigFieldKind::StringArray => {
            let Some(values) = value.as_array() else {
                errors.push(format!("{}: Must be an array", field.path));
                return;
            };
            for (index, item) in values.iter().enumerate() {
                let item_path = format!("{}[{index}]", field.path);
                if let Some(item) = item.as_str() {
                    if field.item_path {
                        validate_path(&item_path, item, errors);
                    }
                } else {
                    errors.push(format!("{item_path}: Must be a string"));
                }
            }
        }
        ConfigFieldKind::Enum => {
            let Some(value) = value.as_str() else {
                errors.push(format!("{}: Must be a string", field.path));
                return;
            };
            if !field.values.contains(&value) {
                let expected = match field.enum_style {
                    ConfigEnumStyle::List => format!("one of: {}", field.values.join(", ")),
                    ConfigEnumStyle::QuotedOr => field
                        .values
                        .iter()
                        .map(|value| format!("\"{value}\""))
                        .collect::<Vec<_>>()
                        .join(" or "),
                };
                errors.push(format!("{}: Must be {expected}", field.path));
            }
        }
        ConfigFieldKind::String | ConfigFieldKind::Url | ConfigFieldKind::Path => {
            let Some(value) = value.as_str() else {
                errors.push(format!("{}: Must be a string", field.path));
                return;
            };
            match field.kind {
                ConfigFieldKind::Url => validate_url(field.path, value, errors),
                ConfigFieldKind::Path => validate_path(field.path, value, errors),
                _ => {}
            }
        }
    }
}

fn warn_unknown_keys(root: &Map<String, Value>, warnings: &mut Vec<String>) {
    let sections = section_paths();
    let mut known =
        BTreeMap::<String, BTreeSet<&str>>::from([(String::new(), BTreeSet::from(["$schema"]))]);
    for field in CONFIG_FIELDS.iter().filter(|field| field.file) {
        known
            .entry(field.section.to_owned())
            .or_default()
            .insert(field.key);
    }
    for section in &sections {
        let (parent, key) = section
            .rsplit_once('.')
            .map_or(("", section.as_str()), |(parent, key)| (parent, key));
        known.entry(parent.to_owned()).or_default().insert(key);
    }
    let root_value = Value::Object(root.clone());
    for (section, keys) in known {
        let value = if section.is_empty() {
            Some(root)
        } else {
            get_path(&root_value, &section).and_then(Value::as_object)
        };
        let Some(value) = value else { continue };
        for key in value.keys() {
            if !keys.contains(key.as_str()) {
                warnings.push(if section.is_empty() {
                    format!("Unknown configuration key: {key}")
                } else {
                    format!("Unknown configuration key: {section}.{key}")
                });
            }
        }
    }
}

pub fn validate_config(config: &Value) -> ValidationResult {
    let Some(root) = config.as_object() else {
        return ValidationResult {
            valid: false,
            errors: vec!["Configuration must be a JSON object".into()],
            warnings: vec![],
            config: None,
        };
    };
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    for section in section_paths() {
        if let Some(value) = get_path(config, &section) {
            if !value.is_null() && !value.is_object() {
                errors.push(format!("{section}: Must be an object"));
            }
        }
    }
    for field in CONFIG_FIELDS.iter().filter(|field| field.file) {
        let parent = if field.section.is_empty() {
            Some(root)
        } else {
            get_path(config, field.section).and_then(Value::as_object)
        };
        validate_field(
            field,
            parent.and_then(|parent| parent.get(field.key)),
            &mut errors,
            &mut warnings,
        );
    }
    warn_unknown_keys(root, &mut warnings);
    let valid = errors.is_empty();
    ValidationResult {
        valid,
        errors,
        warnings,
        config: valid.then(|| config.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::validate_config;
    use serde_json::json;

    #[test]
    fn output_format_yaml_validates_clean() {
        let result = validate_config(&json!({"output": {"format": "yaml"}}));
        assert!(result.valid, "unexpected errors: {:?}", result.errors);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn output_format_json_validates_clean() {
        let result = validate_config(&json!({"output": {"format": "json"}}));
        assert!(result.valid, "unexpected errors: {:?}", result.errors);
    }

    #[test]
    fn output_format_null_validates_clean() {
        let result = validate_config(&json!({"output": {"format": null}}));
        assert!(result.valid, "unexpected errors: {:?}", result.errors);
    }

    #[test]
    fn output_format_invalid_string_errors() {
        let result = validate_config(&json!({"output": {"format": "xml"}}));
        assert!(!result.valid);
        assert!(
            result
                .errors
                .iter()
                .any(|error| error == "output.format: Must be one of: yaml, json")
        );
    }

    #[test]
    fn output_format_non_string_errors() {
        let result = validate_config(&json!({"output": {"format": 42}}));
        assert!(!result.valid);
        assert!(
            result
                .errors
                .iter()
                .any(|error| error == "output.format: Must be a string")
        );
    }
}
