use super::write_json;
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};
use std::io::{self};

/// Largest management request read from stdin.
pub(super) const MAX_MANAGEMENT_REQUEST_BYTES: u64 = 128 * 1024;

pub(super) fn config_management(runtime: &ToolRuntime) -> u8 {
    let result = octocode_io::read::read_limited(io::stdin(), MAX_MANAGEMENT_REQUEST_BYTES)
        .map_err(|_| super::config::ManageError::from("Cannot read management request."))
        .and_then(|request| {
            let request = request.ok_or("Management request exceeds 128 KiB.")?;
            config_management_response(runtime, &request)
        });
    match result {
        Ok(response) => write_json(&response),
        Err(error) => {
            write_json(
                &json!({"success": false, "apiVersion": 1, "error": {"code": error.code(), "message": error.to_string()}}),
            );
            error.exit_code()
        }
    }
}

pub(super) fn config_management_response(
    runtime: &ToolRuntime,
    bytes: &[u8],
) -> Result<Value, super::config::ManageError> {
    let request: Value =
        serde_json::from_slice(bytes).map_err(|_| "Management request must be valid JSON.")?;
    if !request.is_object() {
        return Err("Management request must be an object.".into());
    }
    let catalog = runtime.catalog();
    if request.get("expectedFingerprint") != catalog.get("fingerprint") {
        return Err(super::config::ManageError::Unavailable(
            "Configuration management contracts do not match. Rebuild or update Octocode.".into(),
        ));
    }
    let operation = request
        .get("operation")
        .and_then(Value::as_str)
        .ok_or("Missing management operation.")?;
    let data = match operation {
        "inspect" | "setEnv" | "removeEnv" | "setSetting" | "removeSetting" => {
            super::config::manage(runtime, &request)?
        }
        "agents" | "setAgent" | "removeAgent" => {
            let cwd = std::env::current_dir().map_err(|_| "Cannot resolve workspace.")?;
            super::mcp_manage::manage(&request, &cwd)?
        }
        _ => return Err("Unsupported management operation.".into()),
    };
    Ok(
        json!({"success": true, "apiVersion": 1, "fingerprint": catalog["fingerprint"], "data": data}),
    )
}
