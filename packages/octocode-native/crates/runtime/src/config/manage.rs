//! Management uses generated field metadata and never returns stored secret values.
use super::resolver::{parse_candidate, workspace_file_allowed, workspace_setting_allowed};
use super::validation::{get_path, validate_field};
use super::{
    CONFIG_FIELDS, ConfigFieldKind, ConfigFieldSpec, ConfigOutput, SettingSource, config_revision,
    edit_config_json, get_config_value, parse_config_json, parse_env, read_private_config,
    removed_reason, replace_private_config,
};
use serde_json::{Value, json};
use std::path::Path;
fn field_value(field: &ConfigFieldSpec, value: &Value) -> Result<(), String> {
    let mut errors = Vec::new();
    validate_field(field, Some(value), &mut errors);
    if !errors.is_empty() {
        return Err(format!(
            "Invalid value for {}; check its type and allowed values.",
            field.path
        ));
    }
    Ok(())
}
/// Why `name` cannot be saved: the contract removed it. Removing it stays
/// allowed, so a stale line can be deleted.
pub(super) fn reject_removed(name: &str) -> Result<(), String> {
    removed_reason(name).map_or(Ok(()), |reason| {
        Err(format!("{name} was removed and cannot be set: {reason}"))
    })
}
/// Whether `value` is one a setting's variable may be saved with: one the
/// resolver accepts. Undeclared keys are free-form.
pub(crate) fn validate_env_value(key: &str, value: &str) -> Result<(), String> {
    let Some((field, binding)) = CONFIG_FIELDS.iter().find_map(|field| {
        field
            .env
            .filter(|binding| binding.name == key)
            .map(|binding| (field, binding))
    }) else {
        return Ok(());
    };
    if value.trim().is_empty() {
        return Ok(());
    }
    if parse_candidate(
        field,
        &Value::String(value.to_owned()),
        true,
        binding.normalize,
    )
    .is_none()
    {
        return Err(format!(
            "Invalid value for {key}; check its type and allowed values."
        ));
    }
    Ok(())
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
    let field = CONFIG_FIELDS.iter().find(|f| f.file && f.path == key);
    if field.is_none() && removed_reason(key).is_none() {
        return Err(invalid("Unknown configuration setting.".into()));
    }
    if let Some(value) = value {
        reject_removed(key).map_err(invalid)?;
        let field = field.ok_or_else(|| invalid("Unknown configuration setting.".into()))?;
        field_value(field, value).map_err(invalid)?;
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
/// A URL setting without the parts that carry credentials (userinfo, query
/// values, fragment), so a token embedded in it is never shown. Query keys
/// stay: they name what is set without revealing it.
fn shown(field: &ConfigFieldSpec, value: Option<Value>) -> Option<Value> {
    if field.kind != ConfigFieldKind::Url {
        return value;
    }
    let Some(mut url) = value
        .as_ref()
        .and_then(Value::as_str)
        .and_then(|v| url::Url::parse(v).ok())
    else {
        return value;
    };
    let sensitive = !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some_and(|query| !query.is_empty())
        || url.fragment().is_some();
    if !sensitive {
        return value;
    }
    let _ = url.set_username("");
    let _ = url.set_password(None);
    let keys: Vec<String> = url.query_pairs().map(|(key, _)| key.into_owned()).collect();
    if keys.is_empty() {
        url.set_query(None);
    } else {
        url.query_pairs_mut()
            .clear()
            .extend_pairs(keys.iter().map(|key| (key.as_str(), "[REDACTED]")));
    }
    url.set_fragment(None);
    Some(Value::String(url.into()))
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
                    keys.push(json!({"key":field.env_name().unwrap_or(field.path),"setting":field.path,"set":value.as_str().is_some_and(|v|!v.trim().is_empty()),"scope":if index==0{"home"}else{"workspace"},"source":paths[index+2].1,"secret":true}));
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
        let source = match config.setting_source(field.path) {
            SettingSource::Process => "process",
            SettingSource::HomeEnv => "homeEnv",
            SettingSource::WorkspaceEnv => "workspaceEnv",
            SettingSource::WorkspaceFile => "workspaceSettings",
            SettingSource::HomeFile => "homeSettings",
            SettingSource::Default => "default",
        };
        let env_key = matches!(source, "process" | "homeEnv" | "workspaceEnv").then(|| field.env_name()).flatten();
        let workspace_allowed=workspace_file_allowed(field);
        let workspace_narrow_values=field.values.iter().filter(|value| !workspace_allowed && workspace_setting_allowed(field,&json!(value))).copied().collect::<Vec<_>>();
        json!({"key":field.path,"value":shown(field,get_config_value(&config.resolved,field.path)),"homeValue":shown(field,home.cloned()),"workspaceValue":shown(field,project.cloned()),"homeSet":home_saved.is_some(),"workspaceSet":workspace_saved.is_some(),"source":source,"envKey":env_key,"workspaceAllowed":workspace_allowed,"workspaceNarrowValues":workspace_narrow_values})
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
mod tests;
