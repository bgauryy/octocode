use super::ManageError;
use octocode_native::runtime::ToolRuntime;
use serde_json::json;

/// Structured local management endpoint; input and errors never echo secret values.
pub fn manage(
    runtime: &ToolRuntime,
    request: &serde_json::Value,
) -> Result<serde_json::Value, ManageError> {
    use octocode_native::config::{edit_scoped_env, edit_setting, inspect_management};
    let operation = request
        .get("operation")
        .and_then(serde_json::Value::as_str)
        .ok_or("Operation is required.")?;
    let inspector = runtime.inspect_config();
    let workspace = inspector
        .project_env_path
        .parent()
        .ok_or("Workspace configuration directory is unavailable.")?;
    if operation == "inspect" {
        return Ok(enrich_settings(inspect_management(
            runtime.config(),
            workspace,
        )));
    }
    let scope = request
        .get("scope")
        .and_then(serde_json::Value::as_str)
        .ok_or("Scope is required.")?;
    let directory = match scope {
        "home" => runtime.config().home.as_path(),
        "workspace" => workspace,
        _ => return Err("Scope must be home or workspace.".into()),
    };
    let key = request
        .get("key")
        .and_then(serde_json::Value::as_str)
        .ok_or("Key is required.")?;
    let revision = request
        .get("revision")
        .and_then(serde_json::Value::as_str)
        .ok_or("File revision is required; inspect before saving.")?;
    let changed = match operation {
        "setEnv" | "removeEnv" => {
            let value = if operation == "setEnv" {
                Some(
                    request
                        .get("value")
                        .and_then(serde_json::Value::as_str)
                        .ok_or("Environment value must be a string.")?,
                )
            } else {
                None
            };
            edit_scoped_env(
                &directory.join(".env"),
                key,
                value,
                scope == "workspace",
                Some(revision),
            )
            .map_err(|e| ManageError::from_edit(&e, "Cannot safely write configuration."))?
        }
        "setSetting" | "removeSetting" => {
            let value = if operation == "setSetting" {
                Some(request.get("value").ok_or("Setting value is required.")?)
            } else {
                None
            };
            edit_setting(
                &directory.join(".octocoderc"),
                key,
                value,
                revision,
                scope == "workspace",
            )
            .map_err(|e| ManageError::from_edit(&e, &e.to_string()))?
        }
        _ => return Err("Unknown configuration operation.".into()),
    };
    Ok(json!({"success":true,"changed":changed,"key":key,"scope":scope}))
}

/// The generated native contract is the one metadata source for the browser
/// editor.  This replaces the former Node-side join with `CONFIG_FIELDS`.
pub(super) fn enrich_settings(mut view: serde_json::Value) -> serde_json::Value {
    use octocode_native::config::{CONFIG_FIELDS, ConfigFieldKind};
    let Some(settings) = view
        .get_mut("settings")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return view;
    };
    for setting in settings {
        let Some(key) = setting.get("key").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Some(field) = CONFIG_FIELDS.iter().find(|field| field.path == key) else {
            continue;
        };
        let kind = match field.kind {
            ConfigFieldKind::Boolean => "boolean",
            ConfigFieldKind::String => "string",
            ConfigFieldKind::Url => "url",
            ConfigFieldKind::Path => "path",
            ConfigFieldKind::StringArray => "stringArray",
            ConfigFieldKind::Enum => "enum",
        };
        setting["type"] = json!(kind);
        setting["credential"] = json!(field.credential);
        if let Ok(default) = serde_json::from_str(field.default_json) {
            setting["defaultValue"] = default;
        }
        if !field.values.is_empty() {
            setting["values"] = json!(field.values);
        }
    }
    view
}
