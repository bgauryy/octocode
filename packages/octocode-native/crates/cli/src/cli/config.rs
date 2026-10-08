use super::{emit_error, write_json};
use octocode_native::runtime::ToolRuntime;
use serde_json::json;
use std::io;

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

/// `octocode config`: where configuration lives and which keys are loaded,
/// never their values (`config get KEY` prints one value on request).
pub fn show(runtime: &ToolRuntime, json_out: bool) -> u8 {
    let view = runtime.inspect_config();
    let config_file = view
        .config_path
        .clone()
        .unwrap_or_else(|| view.home.join(".octocoderc"));
    let config_file_exists = view.config_path.is_some();
    let project_config_exists = view.project_config_path.is_some();
    if json_out {
        return write_json(
            &json!({
                "home": view.home,
                "storage": view.storage_mode,
                "configFile": {
                    "path": config_file,
                    "exists": config_file_exists,
                    "keys": view.config_keys,
                },
                "projectConfigFile": {
                    "path": view.project_config_file,
                    "exists": project_config_exists,
                    "keys": view.project_config_keys,
                },
                "envFiles": {
                    "global": {"path": view.global_env_path, "exists": view.global_env_path.is_file()},
                    "project": {"path": view.project_env_path, "exists": view.project_env_path.is_file()},
                },
                "envKeys": view.loaded_keys,
                "skippedProtected": view.skipped_protected,
                "skippedExisting": view.skipped_existing,
                "diagnostics": view.diagnostics,
            }),
            true,
        );
    }
    let found = |exists: bool| if exists { "" } else { " (not found)" };
    println!("home     {}", view.home.display());
    println!("storage  {}", view.storage_mode);
    println!("config");
    println!(
        "  global   {}{}",
        config_file.display(),
        found(config_file_exists)
    );
    println!(
        "  project  {}{}",
        view.project_config_file.display(),
        found(project_config_exists)
    );
    println!(".env");
    println!(
        "  global   {}{}",
        view.global_env_path.display(),
        found(view.global_env_path.is_file())
    );
    println!(
        "  project  {}{}",
        view.project_env_path.display(),
        found(view.project_env_path.is_file())
    );
    let list = |label: &str, keys: &[String]| {
        if !keys.is_empty() {
            println!("{label} ({}): {}", keys.len(), keys.join(", "));
        }
    };
    list("config keys", &view.config_keys);
    list("project config keys", &view.project_config_keys);
    list("env keys", &view.loaded_keys);
    for skip in &view.skipped_protected {
        println!(
            "skipped (protected) {}: found in {} but not applied; set it in the process environment or config file",
            skip.key,
            skip.source_path.display()
        );
    }
    for skip in &view.skipped_existing {
        println!(
            "skipped (existing) {}: found in {} but the process environment already sets it",
            skip.key,
            skip.source_path.display()
        );
    }
    for diagnostic in &view.diagnostics {
        println!("{diagnostic}");
    }
    println!("Values are not shown here; `octocode config get KEY` prints one.");
    0
}

/// `config home`: the Octocode home directory, one line, for scripts and skills.
pub fn home(runtime: &ToolRuntime, json_out: bool) -> u8 {
    let home = &runtime.config().home;
    if json_out {
        return write_json(&json!({"home": home}), true);
    }
    println!("{}", home.display());
    0
}

/// `config get KEY`: the resolved value on stdout (nothing else, so
/// `$(octocode config get KEY)` captures it); exit 1 when unset.
pub fn get(runtime: &ToolRuntime, key: &str, json_out: bool) -> u8 {
    let config = runtime.config();
    let value = config.env_value(key).filter(|value| !value.is_empty());
    let source = value.map(|_| {
        if config.dotenv.applied.iter().any(|applied| applied == key) {
            config
                .dotenv
                .sources
                .get(key)
                .map_or("global", String::as_str)
        } else {
            "environment"
        }
    });
    if json_out {
        let code = write_json(
            &json!({"key": key, "set": value.is_some(), "value": value, "source": source}),
            true,
        );
        if code != 0 {
            return code;
        }
    } else if let Some(value) = value {
        println!("{value}");
    } else {
        eprintln!("{key}: unset");
    }
    if value.is_some() { 0 } else { 1 }
}

/// `config check KEY`: exit 0 when set, 1 when unset. A GitHub token key that
/// is unset names the credential GitHub calls use instead.
pub async fn check(runtime: &ToolRuntime, key: &str, json_out: bool) -> u8 {
    let set = runtime
        .config()
        .env_value(key)
        .is_some_and(|value| !value.is_empty());
    let github_auth = if !set && octocode_native::config::ENV_TOKEN_VARS.contains(&key) {
        super::system::active_token_source(runtime).await
    } else {
        None
    };
    if json_out {
        let mut value = json!({"key": key, "set": set});
        if let Some(source) = &github_auth {
            value["githubTokenSource"] = json!(source);
        }
        let code = write_json(&value, true);
        if code != 0 {
            return code;
        }
    } else {
        match &github_auth {
            Some(source) => {
                println!("{key}: unset (GitHub calls use the token from {source})");
            }
            None => println!("{key}: {}", if set { "set" } else { "unset" }),
        }
    }
    if set { 0 } else { 1 }
}

/// `config set KEY VALUE` / `config set KEY --stdin`.
pub fn set(
    runtime: &ToolRuntime,
    key: &str,
    value: Option<String>,
    stdin: bool,
    json_out: bool,
) -> u8 {
    let value = if stdin {
        read_stdin_value()
    } else {
        value.ok_or("Supply a value: config set KEY VALUE, or config set KEY --stdin.")
    };
    match value {
        Ok(value) => edit(runtime, key, Some(&value), json_out),
        Err(message) => {
            emit_error(message, json_out);
            2
        }
    }
}

/// `config unset KEY`.
pub fn unset(runtime: &ToolRuntime, key: &str, json_out: bool) -> u8 {
    edit(runtime, key, None, json_out)
}

fn read_stdin_value() -> Result<String, &'static str> {
    let bytes = super::read_bounded(io::stdin(), 64 * 1024)
        .map_err(|_| "Cannot read value from stdin.")?
        .ok_or("Value exceeds 64 KiB.")?;
    let mut value = String::from_utf8(bytes).map_err(|_| "Cannot read value from stdin.")?;
    if value.ends_with('\n') {
        value.pop();
        if value.ends_with('\r') {
            value.pop();
        }
    }
    Ok(value)
}

fn edit(runtime: &ToolRuntime, key: &str, value: Option<&str>, json_out: bool) -> u8 {
    let path = runtime.config().home.join(".env");
    match octocode_native::config::edit_scoped_env(&path, key, value, false, None) {
        Ok(changed) => {
            let action = if value.is_some() { "set" } else { "unset" };
            if json_out {
                write_json(
                    &json!({"action": action, "key": key, "path": path, "changed": changed}),
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
