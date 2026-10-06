//! Management uses generated field metadata and never returns stored secret values.
use super::resolver::{parse_candidate, workspace_file_allowed, workspace_setting_allowed};
use super::validation::{get_path, validate_field};
use super::{
    CONFIG_FIELDS, ConfigFieldKind, ConfigFieldSpec, ConfigNormalize, ConfigOutput,
    config_revision, edit_config_json, get_config_value, parse_config_json, parse_env,
    read_private_config, replace_private_config,
};
use serde_json::{Value, json};
use std::path::Path;
fn field_value(field: &ConfigFieldSpec, value: &Value) -> Result<(), String> {
    let mut errors = Vec::new();
    validate_field(field, Some(value), &mut errors, &mut Vec::new());
    if !errors.is_empty() {
        return Err(format!(
            "Invalid value for {}; check its type, range, and allowed values.",
            field.path
        ));
    }
    Ok(())
}
pub fn validate_env_value(key: &str, value: &str) -> Result<(), String> {
    let Some(field) = CONFIG_FIELDS
        .iter()
        .find(|f| f.env.iter().any(|b| b.name == key))
    else {
        return Ok(());
    };
    if value.trim().is_empty() {
        return Ok(());
    }
    let binding = field
        .env
        .iter()
        .find(|b| b.name == key)
        .ok_or("Unknown environment binding.")?;
    let normalized = match binding.normalize {
        Some(ConfigNormalize::Trim) => value.trim().to_owned(),
        Some(ConfigNormalize::Lower) => value.trim().to_ascii_lowercase(),
        None => value.to_owned(),
    };
    let v = match field.kind {
        ConfigFieldKind::Boolean => super::parse_boolean_env(Some(value)).map(Value::Bool),
        ConfigFieldKind::Number | ConfigFieldKind::SchemaVersion => {
            value.trim().parse::<i64>().ok().map(|n| json!(n))
        }
        ConfigFieldKind::StringArray => Some(json!(
            super::parse_string_array_env(Some(value)).unwrap_or_default()
        )),
        _ => Some(Value::String(normalized)),
    }
    .ok_or_else(|| format!("Invalid value for {key}; check its expected type."))?;
    if field.file {
        field_value(field, &v)
    } else {
        match field.kind {
            ConfigFieldKind::Enum if !field.values.contains(&v.as_str().unwrap_or_default()) => {
                Err(format!(
                    "Invalid value for {key}; check its allowed values."
                ))
            }
            _ => Ok(()),
        }
    }
}
/// Set or remove one `.octocoderc` setting under the inspected `revision`.
/// Errors are `InvalidInput` for a rejected key or value, `WouldBlock` when
/// the file changed or is locked since it was inspected, and `Other` when it
/// cannot be read or written safely.
pub fn edit_setting(
    path: &Path,
    key: &str,
    value: Option<&Value>,
    revision: &str,
    workspace: bool,
) -> std::io::Result<bool> {
    let invalid = |message: String| std::io::Error::new(std::io::ErrorKind::InvalidInput, message);
    let field = CONFIG_FIELDS
        .iter()
        .find(|f| f.file && f.path == key)
        .ok_or_else(|| invalid("Unknown configuration setting.".into()))?;
    if let Some(value) = value {
        field_value(field, value).map_err(invalid)?;
        if field.kind == ConfigFieldKind::Number && value.as_f64().is_some_and(|v| v.fract() != 0.0)
        {
            return Err(invalid(format!(
                "Invalid value for {}; an integer is required.",
                field.path
            )));
        }
        if workspace && !workspace_setting_allowed(field, value) {
            return Err(invalid(
                "This setting may only be configured in home scope.".into(),
            ));
        }
    }
    let original = read_private_config(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::InvalidInput {
            error
        } else {
            std::io::Error::other("Cannot safely read configuration.")
        }
    })?;
    let parts = key.split('.').collect::<Vec<_>>();
    let mut body =
        edit_config_json(original.as_deref().unwrap_or("{}"), &parts, value).map_err(invalid)?;
    if value.is_none() {
        // A section the removal leaves empty goes with it.
        for depth in (1..parts.len()).rev() {
            let parsed = parse_config_json(&body).map_err(invalid)?;
            if !get_path(&parsed, &parts[..depth].join("."))
                .and_then(Value::as_object)
                .is_some_and(serde_json::Map::is_empty)
            {
                break;
            }
            body = edit_config_json(&body, &parts[..depth], None).map_err(invalid)?;
        }
    }
    replace_private_config(path, revision, &body, false)
        .map(|(changed, _)| changed)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::InvalidInput => e,
            _ => std::io::Error::other("Cannot safely write configuration."),
        })
}
/// A URL setting without its userinfo, so a credential embedded in it is never shown.
fn shown(field: &ConfigFieldSpec, value: Option<Value>) -> Option<Value> {
    if field.kind == ConfigFieldKind::Url
        && let Some(mut url) = value
            .as_ref()
            .and_then(Value::as_str)
            .and_then(|v| url::Url::parse(v).ok())
        && (!url.username().is_empty() || url.password().is_some())
    {
        let _ = url.set_username("");
        let _ = url.set_password(None);
        return Some(Value::String(url.into()));
    }
    value
}
/// Each file degrades on its own: one that cannot be read safely (a symlink,
/// a device, too large) is reported read-only and never read through.
pub fn inspect_management(config: &ConfigOutput, workspace: &Path) -> Value {
    let paths = [
        ("homeEnv", config.home.join(".env")),
        ("workspaceEnv", workspace.join(".env")),
        ("homeSettings", config.home.join(".octocoderc")),
        ("workspaceSettings", workspace.join(".octocoderc")),
    ];
    let mut files = serde_json::Map::new();
    let mut envs = vec![];
    let mut saved = vec![];
    let mut warnings = config.diagnostics.iter().map(|diagnostic|json!({
        "code":diagnostic.code,"severity":diagnostic.severity,"key":diagnostic.field_path,"source":diagnostic.source_path,
        "message":"Configuration resolver reported an invalid, unsupported, or protected value; values are omitted."
    })).collect::<Vec<_>>();
    for (name, path) in &paths {
        let text = match read_private_config(path) {
            Ok(text) => text,
            Err(_) => {
                let reason =
                    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
                        "symlink"
                    } else {
                        "unreadable"
                    };
                warnings.push(json!({"file":name,"message":"File cannot be read safely; it is shown read-only."}));
                files.insert(
                    (*name).into(),
                    json!({"path":path,"exists":true,"writable":false,"reason":reason}),
                );
                if name.ends_with("Env") {
                    envs.push(Default::default());
                } else {
                    saved.push(None);
                }
                continue;
            }
        };
        let mut status = json!({"path":path,"revision":config_revision(text.as_deref()),"exists":text.is_some(),"writable":true});
        if name.ends_with("Env") {
            envs.push(parse_env(text.as_deref()));
        } else {
            let parsed = match text.as_deref() {
                None => Some(json!({})),
                Some(t) => match parse_config_json(t) {
                    Ok(v) => Some(v),
                    Err(_) => {
                        status["valid"] = json!(false);
                        status["writable"] = json!(false);
                        warnings.push(json!({"file":name,"message":"Invalid or ambiguous settings file; repair it before editing."}));
                        None
                    }
                },
            };
            saved.push(parsed);
        }
        files.insert((*name).into(), status);
    }
    let mut keys = vec![];
    for (index, env) in envs.iter().enumerate() {
        for (key, value) in env {
            keys.push(json!({"key":key,"set":!value.trim().is_empty(),"scope":if index==0{"home"}else{"workspace"},"source":paths[index].1,"secret":true}));
        }
    }
    // Include credential presence saved in settings without disclosing its value.
    for (index, file) in saved.iter().enumerate() {
        if let Some(file) = file {
            for field in CONFIG_FIELDS.iter().filter(|f| f.file && f.credential) {
                if let Some(value) = get_path(file, field.path) {
                    keys.push(json!({"key":field.env.first().map(|b|b.name).unwrap_or(field.path),"setting":field.path,"set":value.as_str().is_some_and(|v|!v.trim().is_empty()),"scope":if index==0{"home"}else{"workspace"},"source":paths[index+2].1,"secret":true}));
                }
            }
        }
    }
    let settings = CONFIG_FIELDS.iter().filter(|f| f.file && !f.credential).map(|field| {
        let home_saved = saved[0].as_ref().and_then(|v| get_path(v, field.path));
        let workspace_saved = saved[1].as_ref().and_then(|v| get_path(v, field.path));
        let home = home_saved.filter(|v| field_value(field, v).is_ok());
        let project = workspace_saved.filter(|v| field_value(field, v).is_ok());
        for (name, value) in [("homeSettings", home_saved), ("workspaceSettings", workspace_saved)] {
            if value.is_some_and(|v| field_value(field,v).is_err()) {
                warnings.push(json!({"file":name,"key":field.path,"message":"Invalid saved setting; its value is omitted."}));
            }
        }
        let selected = field.env.iter().find(|binding| {
            config.env_value(binding.name).is_some_and(|value| {
                parse_candidate(field,&Value::String(value.to_owned()),true,binding.normalize).is_some()
                    || binding.invalid == super::ConfigInvalidEnv::Default
            })
        });
        let source = if let Some(binding) = selected {
            if config.dotenv.applied.iter().any(|key| key == binding.name) {
                match config.dotenv.sources.get(binding.name).map(String::as_str) {
                    Some("global") => "homeEnv",
                    Some("project") => "workspaceEnv",
                    _ => "process",
                }
            } else { "process" }
        } else if project.is_some_and(|v| parse_candidate(field,v,false,None).is_some() && workspace_setting_allowed(field,v)) {
            "workspaceSettings"
        } else if home.is_some_and(|v| parse_candidate(field,v,false,None).is_some()) { "homeSettings" } else { "default" };
        let workspace_allowed=workspace_file_allowed(field);
        let workspace_narrow_values=field.values.iter().filter(|value| !workspace_allowed && workspace_setting_allowed(field,&json!(value))).copied().collect::<Vec<_>>();
        json!({"key":field.path,"value":shown(field,get_config_value(&config.resolved,field.path)),"homeValue":shown(field,home.cloned()),"workspaceValue":shown(field,project.cloned()),"homeSet":home_saved.is_some(),"workspaceSet":workspace_saved.is_some(),"source":source,"envKey":selected.map(|b|b.name),"workspaceAllowed":workspace_allowed,"workspaceNarrowValues":workspace_narrow_values})
    }).collect::<Vec<_>>();
    let credential_settings = CONFIG_FIELDS
        .iter()
        .filter(|f| f.file && f.credential)
        .map(|f| f.path)
        .collect::<Vec<_>>()
        .join(", ");
    json!({"settings":settings,"keys":keys,"files":files,"warnings":warnings,"storage":{"kind":"dotenv","encrypted":false,"message":format!("Environment keys are saved in .env files and credential settings ({credential_settings}) in .octocoderc, unencrypted with owner-only permissions; the GitHub login store is separate.")}})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_environment_types() {
        assert!(validate_env_value("REQUEST_TIMEOUT", "12ms").is_err());
        assert!(validate_env_value("REQUEST_TIMEOUT", "4").is_err());
        assert!(validate_env_value("REQUEST_TIMEOUT", "6000").is_ok());
        assert!(validate_env_value("ENABLE_LOCAL", "perhaps").is_err());
        assert!(validate_env_value("CUSTOM_PROVIDER_KEY", "anything").is_ok());
    }
    #[test]
    fn inspect_never_discloses_unknown_or_invalid_values() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let home = root.join("home");
        let cwd = root.join("workspace");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(cwd.join(".octocode")).unwrap();
        std::fs::write(
            home.join(".env"),
            "CUSTOM_KEY=private-marker\nREQUEST_TIMEOUT=6000\n",
        )
        .unwrap();
        std::fs::write(home.join(".octocoderc"),r#"{"classification":{"api":"credential-marker"},"network":{"maxRetries":{"hidden":"invalid-marker"}}}"#).unwrap();
        let input = super::super::acquire_config_input(
            std::collections::BTreeMap::from([(
                "OCTOCODE_HOME".into(),
                home.to_string_lossy().into_owned(),
            )]),
            cwd.clone(),
            root,
            false,
            super::super::RuntimeSurface::Cli,
        );
        let config = super::super::resolve_config(&input);
        let output = inspect_management(&config, &cwd.join(".octocode"));
        let serialized = serde_json::to_string(&output).unwrap();
        for marker in ["private-marker", "credential-marker", "invalid-marker"] {
            assert!(!serialized.contains(marker));
        }
        assert!(
            output["keys"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["key"] == "CUSTOM_KEY" && v["scope"] == "home")
        );
    }
    #[test]
    fn inspection_source_tracks_skipped_environment_and_process_override() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let home = root.join("home");
        let cwd = root.join("workspace");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(cwd.join(".octocode")).unwrap();
        std::fs::write(home.join(".env"), "ENABLE_LOCAL=true\n").unwrap();
        std::fs::write(
            home.join(".octocoderc"),
            r#"{"local":{"enabled":false},"network":{"maxRetries":2.5}}"#,
        )
        .unwrap();
        for (raw, source) in [("nope", "homeSettings"), ("true", "process")] {
            let input = super::super::acquire_config_input(
                std::collections::BTreeMap::from([
                    ("OCTOCODE_HOME".into(), home.to_string_lossy().into_owned()),
                    ("ENABLE_LOCAL".into(), raw.into()),
                ]),
                cwd.clone(),
                root.clone(),
                false,
                super::super::RuntimeSurface::Cli,
            );
            let config = super::super::resolve_config(&input);
            let output = inspect_management(&config, &cwd.join(".octocode"));
            let settings = output["settings"].as_array().unwrap();
            assert_eq!(
                settings
                    .iter()
                    .find(|s| s["key"] == "local.enabled")
                    .unwrap()["source"],
                source
            );
            assert_eq!(
                settings
                    .iter()
                    .find(|s| s["key"] == "network.maxRetries")
                    .unwrap()["source"],
                "homeSettings"
            );
        }
    }
    fn inspect_home(home_env: Option<&str>, home_rc: &str) -> (tempfile::TempDir, Value) {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let home = root.join("home");
        let cwd = root.join("workspace");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(cwd.join(".octocode")).unwrap();
        std::fs::write(home.join(".octocoderc"), home_rc).unwrap();
        if let Some(text) = home_env {
            let target = root.join("elsewhere.env");
            std::fs::write(&target, text).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, home.join(".env")).unwrap();
        }
        let input = super::super::acquire_config_input(
            std::collections::BTreeMap::from([(
                "OCTOCODE_HOME".into(),
                home.to_string_lossy().into_owned(),
            )]),
            cwd.clone(),
            root,
            false,
            super::super::RuntimeSurface::Cli,
        );
        let output = inspect_management(
            &super::super::resolve_config(&input),
            &cwd.join(".octocode"),
        );
        (dir, output)
    }
    #[cfg(unix)]
    #[test]
    fn a_symlinked_file_is_read_only_and_never_read_through() {
        let (_dir, output) = inspect_home(Some("LINKED_SECRET_KEY=linked-marker\n"), "{}");
        assert_eq!(output["files"]["homeEnv"]["writable"], false);
        assert_eq!(output["files"]["homeEnv"]["reason"], "symlink");
        assert!(output["files"]["homeEnv"].get("revision").is_none());
        assert_eq!(output["files"]["homeSettings"]["writable"], true);
        assert!(!output.to_string().contains("LINKED_SECRET_KEY"));
    }
    #[test]
    fn url_settings_never_show_embedded_credentials() {
        let (_dir, output) = inspect_home(
            None,
            r#"{"github":{"apiUrl":"https://user:url-secret@ghe.example/api/v3"}}"#,
        );
        assert!(!output.to_string().contains("url-secret"));
        let api = output["settings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["key"] == "github.apiUrl")
            .unwrap();
        assert_eq!(api["homeValue"], "https://ghe.example/api/v3");
    }
    #[test]
    fn storage_message_names_where_credential_settings_live() {
        let (_dir, output) = inspect_home(None, "{}");
        let message = output["storage"]["message"].as_str().unwrap();
        assert!(message.contains(".octocoderc") && message.contains("classification.api"));
    }
    #[test]
    fn removing_the_last_key_prunes_its_empty_section() {
        let dir = tempfile::tempdir().unwrap();
        let path = std::fs::canonicalize(dir.path())
            .unwrap()
            .join(".octocoderc");
        let text = r#"{"output":{"pagination":{"defaultCharLength":4000}},"network":{"timeout":5000,"maxRetries":2}}"#;
        std::fs::write(&path, text).unwrap();
        edit_setting(
            &path,
            "output.pagination.defaultCharLength",
            None,
            &config_revision(Some(text)),
            false,
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        edit_setting(
            &path,
            "network.timeout",
            None,
            &config_revision(Some(&text)),
            false,
        )
        .unwrap();
        let saved = parse_config_json(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved, json!({"network":{"maxRetries":2}}));
    }
    #[test]
    fn stale_revision_cannot_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = std::fs::canonicalize(dir.path())
            .unwrap()
            .join(".octocoderc");
        std::fs::write(&path, "{/*keep*/\"network\":{\"timeout\":5000}}").unwrap();
        assert!(
            edit_setting(
                &path,
                "network.timeout",
                Some(&json!(6000)),
                "missing",
                false
            )
            .is_err()
        );
        let revision = config_revision(read_private_config(&path).unwrap().as_deref());
        assert!(
            edit_setting(
                &path,
                "network.timeout",
                Some(&json!(6000)),
                &revision,
                false
            )
            .unwrap()
        );
        assert!(std::fs::read_to_string(path).unwrap().contains("/*keep*/"));
    }
}
