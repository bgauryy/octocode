use super::{emit_error, write_json};
use octocode_native::runtime::ToolRuntime;
use serde_json::json;
use std::io::{self, Read};

/// A failed `config --manage` request. The code tells the config view how to
/// recover: `CONFLICT` refreshes the inspected files before retrying.
#[derive(Debug, PartialEq, Eq)]
pub enum ManageError {
    /// A malformed request or a rejected value.
    InvalidInput(String),
    /// The file changed or is locked since it was inspected.
    Conflict(String),
}

impl ManageError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "INVALID_INPUT",
            Self::Conflict(_) => "CONFLICT",
        }
    }

    /// Map a locked-file edit error: `WouldBlock` is a revision conflict,
    /// `InvalidInput` keeps its message, and anything else is reported as
    /// `fallback` without echoing OS details.
    pub fn from_edit(error: &io::Error, fallback: &str) -> Self {
        match error.kind() {
            io::ErrorKind::WouldBlock => {
                Self::Conflict("Configuration changed or is busy; refresh and retry.".into())
            }
            io::ErrorKind::InvalidInput => Self::InvalidInput(error.to_string()),
            _ => Self::InvalidInput(fallback.into()),
        }
    }
}

impl std::fmt::Display for ManageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(message) | Self::Conflict(message) => f.write_str(message),
        }
    }
}

impl From<String> for ManageError {
    fn from(message: String) -> Self {
        Self::InvalidInput(message)
    }
}

impl From<&str> for ManageError {
    fn from(message: &str) -> Self {
        Self::InvalidInput(message.into())
    }
}

pub fn show_path(runtime: &ToolRuntime, json_out: bool) -> u8 {
    let path = runtime.inspect_config().global_env_path;
    if json_out {
        write_json(&json!({"path": path, "exists": path.is_file()}), true)
    } else {
        println!("{}", path.display());
        0
    }
}

pub fn edit(
    runtime: &ToolRuntime,
    add: &[String],
    remove: Option<&str>,
    value_stdin: bool,
    json_out: bool,
) -> u8 {
    let result = (|| {
        let key = remove
            .or_else(|| add.first().map(String::as_str))
            .ok_or("Supply --add KEY VALUE or --remove KEY.")?;
        let value = if remove.is_some() {
            None
        } else if value_stdin {
            if add.len() != 1 {
                return Err("Use --add KEY --value-stdin without a VALUE argument.");
            }
            let mut value = String::new();
            io::stdin()
                .take(65_537)
                .read_to_string(&mut value)
                .map_err(|_| "Cannot read value from stdin.")?;
            if value.len() > 65_536 {
                return Err("Value exceeds 64 KiB.");
            }
            if value.ends_with('\n') {
                value.pop();
                if value.ends_with('\r') {
                    value.pop();
                }
            }
            Some(value)
        } else {
            Some(
                add.get(1)
                    .ok_or("Supply --add KEY VALUE or --add KEY --value-stdin.")?
                    .clone(),
            )
        };
        Ok((key, value))
    })();
    let (key, value) = match result {
        Ok(value) => value,
        Err(message) => {
            emit_error(message, json_out);
            return 2;
        }
    };
    let path = runtime.config().home.join(".env");
    match octocode_native::config::edit_scoped_env(&path, key, value.as_deref(), false, None) {
        Ok(changed) => {
            let action = if remove.is_some() { "remove" } else { "add" };
            if json_out {
                write_json(
                    &json!({"success": true, "action": action, "key": key, "path": path, "changed": changed}),
                    true,
                )
            } else {
                println!(
                    "{action} {key}: {} ({})",
                    path.display(),
                    if changed { "updated" } else { "unchanged" }
                );
                0
            }
        }
        Err(error) => {
            emit_error(&format!("Cannot edit global config: {error}"), json_out);
            if error.kind() == io::ErrorKind::InvalidInput {
                2
            } else {
                5
            }
        }
    }
}

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
        return Ok(inspect_management(runtime.config(), workspace));
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
