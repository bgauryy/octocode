use super::super::config::ManageError;
use super::{
    Method, Target, default_entry, entry_at, inspect, parse_document, render_document,
    uses_enabled_key,
};
use octocode_native::config::{read_private_config, replace_private_config};
use serde_json::{Value, json};

pub(super) fn update(target: &Target, request: &Value, remove: bool) -> Result<Value, ManageError> {
    let revision = request
        .get("revision")
        .and_then(Value::as_str)
        .ok_or("Missing revision; refresh agent configuration before editing.")?;
    let original = read_private_config(&target.path)
        .map_err(|error| format!("Agent config cannot be read safely: {error}"))?;
    let root = parse_document(target.client.format, original.as_deref().unwrap_or(""))?;
    let next = if remove {
        None
    } else {
        let patch = request
            .get("patch")
            .and_then(Value::as_object)
            .ok_or("Missing agent patch.")?;
        if patch
            .keys()
            .any(|key| !matches!(key.as_str(), "env" | "enabled" | "method"))
        {
            return Err("Unknown agent patch field.".into());
        }
        let mut server = entry_at(target.client, &root, &target.keys)
            .cloned()
            .unwrap_or_else(|| default_entry(target.client, Method::Npx));
        if !server.is_object() {
            return Err("Octocode entry must be an object; remove the invalid entry first.".into());
        }
        if let Some(method) = patch.get("method") {
            let method = method
                .as_str()
                .and_then(Method::from_name)
                .ok_or("Unsupported installation method.")?;
            let generated = default_entry(target.client, method);
            for key in ["command", "cmd", "args", "type"] {
                if let Some(value) = generated.get(key) {
                    server[key] = value.clone();
                }
            }
        }
        if let Some(enabled) = patch.get("enabled") {
            if target.client.enable.is_none() {
                return Err("This agent's file format does not expose a supported enable flag; use its own MCP controls or remove the entry.".into());
            }
            let enabled = enabled.as_bool().ok_or("enabled must be a boolean.")?;
            if uses_enabled_key(target.client, &root) {
                server["enabled"] = json!(enabled);
            } else {
                server["disabled"] = json!(!enabled);
            }
            // Do not leave an opposite disabling flag behind.
            if enabled && let Some(map) = server.as_object_mut() {
                map.remove("disabled");
                if map.contains_key("enabled") {
                    map.insert("enabled".into(), json!(true));
                }
            }
        }
        if let Some(env) = patch.get("env") {
            let env = env.as_object().ok_or("env must be an object.")?;
            let env_key = target.client.shape.env_key();
            if server.get(env_key).is_none() {
                server[env_key] = json!({});
            }
            let values = server
                .get_mut(env_key)
                .and_then(Value::as_object_mut)
                .ok_or("Existing agent environment is not an object.")?;
            for (key, value) in env {
                let value = if value.is_null() {
                    None
                } else {
                    Some(
                        value
                            .as_str()
                            .ok_or("Environment values must be strings or null.")?,
                    )
                };
                octocode_native::config::validate_env_edit(key, value, target.scope == "workspace")
                    .map_err(|error| error.to_string())?;
                match value {
                    Some(value) => {
                        values.insert(key.clone(), json!(value));
                    }
                    None => {
                        values.remove(key);
                    }
                }
            }
        }
        Some(server)
    };
    let rendered = render_document(
        target.client,
        original.as_deref().unwrap_or(""),
        &target.keys,
        next.as_ref(),
    )?;
    let (changed, _) = replace_private_config(&target.path, revision, &rendered, true)
        .map_err(|error| ManageError::from_edit(&error, &error.to_string()))?;
    let mut result = inspect(target);
    result["changed"] = json!(changed);
    result["restartRequired"] = json!(changed);
    Ok(result)
}
