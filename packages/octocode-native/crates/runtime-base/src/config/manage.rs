//! Management uses generated field metadata and never returns stored secret values.
use super::resolver::{parse_candidate, workspace_file_allowed, workspace_setting_allowed};
use super::validation::{get_path, is_http_url, url_embeds_secret, validate_field};
use super::{
    CONFIG_FIELDS, ConfigFieldKind, ConfigFieldSpec, ConfigOutput, KNOWN_ENV, SettingSource,
    config_revision, edit_config_json, get_config_value, parse_config_json, parse_env,
    read_private_config, replace_private_config,
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
fn url_refusal(key: &str) -> String {
    format!("Invalid value for {key}: use an API root without credentials, a query or a fragment.")
}
/// Whether `value` is one a setting's variable may be saved with: one the
/// resolver accepts. Undeclared keys are free-form.
pub(crate) fn validate_env_value(key: &str, value: &str) -> Result<(), String> {
    if let Some(known) = KNOWN_ENV.iter().find(|known| known.name == key)
        && known.kind == "url"
        && !value.trim().is_empty()
        && (!is_http_url(value.trim()) || url_embeds_secret(value.trim()))
    {
        return Err(url_refusal(key));
    }
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
    if field.kind == ConfigFieldKind::Url && url_embeds_secret(value) {
        return Err(url_refusal(key));
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
/// What one `.octocoderc` edit did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingEdit {
    /// The file already had the requested content.
    Unchanged,
    /// A setting the contract declares was saved or removed.
    Changed,
    /// A key the contract does not declare was removed: one an older or newer
    /// Octocode wrote, which the loader warns about as unknown.
    RemovedUnknown,
}
impl SettingEdit {
    /// Whether the file's bytes changed.
    pub fn changed(self) -> bool {
        self != Self::Unchanged
    }
    /// What a caller tells the person about this edit of `key`, when the
    /// outcome needs more than "changed".
    pub fn notice(self, key: &str) -> Option<String> {
        (self == Self::RemovedUnknown).then(|| {
            format!(
                "Removed unknown configuration key {key}: this Octocode version does not use it."
            )
        })
    }
}
/// Whether `key` names something the contract declares: a setting, a section
/// that holds settings, or a part of a setting's value. Only a key outside all
/// three is unknown, so removing one never drops a declared setting.
fn declared(key: &str) -> bool {
    let within = |path: &str, prefix: &str| {
        path.strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
    };
    CONFIG_FIELDS
        .iter()
        .any(|field| within(field.path, key) || within(key, field.path))
}
/// Set or remove one `.octocoderc` setting under the inspected `revision`.
/// Removal also accepts a key the contract does not declare when the file
/// holds it, so a key the loader warns about can be removed; setting one is
/// refused. Errors are `InvalidInput` for a rejected key or value, `NotFound`
/// when the file does not hold the undeclared key it would remove,
/// `WouldBlock` when the file changed or is locked since it was inspected, and
/// `Other` when it cannot be read or written safely.
pub fn edit_setting(
    path: &Path,
    key: &str,
    value: Option<&Value>,
    revision: &str,
    workspace: bool,
) -> std::io::Result<SettingEdit> {
    let invalid = |message: String| std::io::Error::new(std::io::ErrorKind::InvalidInput, message);
    let unknown = || invalid("Unknown configuration setting.".into());
    let field = CONFIG_FIELDS.iter().find(|f| f.path == key);
    match (field, value) {
        (Some(field), Some(value)) => {
            field_value(field, value).map_err(invalid)?;
            if field.kind == ConfigFieldKind::Url && value.as_str().is_some_and(url_embeds_secret) {
                return Err(invalid(url_refusal(field.path)));
            }
            if workspace && !workspace_setting_allowed(field, value) {
                return Err(invalid(
                    "This setting may only be configured in home scope.".into(),
                ));
            }
        }
        (None, Some(_)) => return Err(unknown()),
        (None, None) if declared(key) => return Err(unknown()),
        (Some(_) | None, None) => {}
    }
    let original = read_private_config(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::InvalidInput {
            error
        } else {
            std::io::Error::other("Cannot safely read configuration.")
        }
    })?;
    if field.is_none() {
        let saved = parse_config_json(original.as_deref().unwrap_or("{}")).map_err(invalid)?;
        if get_path(&saved, key).is_none() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Unknown configuration setting, and this file does not hold it.",
            ));
        }
    }
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
        .map(|(changed, _)| match (changed, field) {
            (false, _) => SettingEdit::Unchanged,
            (true, Some(_)) => SettingEdit::Changed,
            (true, None) => SettingEdit::RemovedUnknown,
        })
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::InvalidInput => e,
            _ => std::io::Error::other("Cannot safely write configuration."),
        })
}
/// A setting's display value: preserve filesystem locators and scrub data.
pub(super) fn shown(field: &ConfigFieldSpec, value: Option<Value>) -> Option<Value> {
    if field.item_path {
        return value;
    }
    match field.kind {
        // Filesystem locators are not content. Preserve their exact spelling.
        ConfigFieldKind::Path => value,
        ConfigFieldKind::Url => value.map(shown_url),
        _ => value.map(|value| crate::security::Sanitized::scrubbed(value).into_value()),
    }
}
/// Hide URL userinfo, query values and fragments, then scrub recognizable
/// credentials from the remaining URL. Invalid URLs still pass the scanner.
pub(super) fn shown_url(value: Value) -> Value {
    let Some(mut url) = value.as_str().and_then(|v| url::Url::parse(v).ok()) else {
        return crate::security::Sanitized::scrubbed(value).into_value();
    };
    let sensitive = !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some_and(|query| !query.is_empty())
        || url.fragment().is_some();
    if !sensitive {
        return crate::security::Sanitized::scrubbed(value).into_value();
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
    Value::String(crate::security::scrub_error_text(url.as_str()))
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
        let Ok(text) = read_private_config(path) else {
            let reason =
                if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
                    "symlink"
                } else {
                    "unreadable"
                };
            warnings.push(
                json!({"file":name,"message":"File cannot be read safely; it is shown read-only."}),
            );
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
            for field in CONFIG_FIELDS.iter().filter(|f| f.credential) {
                if let Some(value) = get_path(file, field.path) {
                    keys.push(json!({"key":field.env_name().unwrap_or(field.path),"setting":field.path,"set":value.as_str().is_some_and(|v|!v.trim().is_empty()),"scope":if index==0{"home"}else{"workspace"},"source":paths[index+2].1,"secret":true}));
                }
            }
        }
    }
    let settings = CONFIG_FIELDS.iter().filter(|f| !f.credential).map(|field| {
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
        .filter(|f| f.credential)
        .map(|f| f.path)
        .collect::<Vec<_>>()
        .join(", ");
    json!({"settings":settings,"keys":keys,"files":files,"warnings":warnings,"storage":{"kind":"dotenv","encrypted":false,"message":format!("Environment keys are saved in .env files and credential settings ({credential_settings}) in .octocoderc, unencrypted with owner-only permissions; the GitHub login store is separate.")}})
}
#[cfg(test)]
mod tests;
