use octocode_native::execution::availability::Availability;
use octocode_native::id::ToolId;
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};

/// Text for a disabled tool whose gating env key was present in a `.env`
/// file but not applied — the state change would otherwise be invisible.
pub(super) fn dropped_key_hint(
    env_vars: &[&str],
    dotenv: &octocode_native::config::EnvApplyReport,
) -> Option<String> {
    env_vars.iter().find_map(|name| {
        if dotenv.skipped_protected.iter().any(|key| key == name) {
            Some(format!(
                "{name} was found in a .env file but dropped (protected); set it in the process environment or config file"
            ))
        } else if dotenv.skipped_existing.iter().any(|key| key == name) {
            Some(format!(
                "{name} in a .env file is shadowed by the process environment"
            ))
        } else {
            None
        }
    })
}

/// A tool's `availability` in the machine catalog: enabled, or what enables
/// it — the variables of its unset prerequisites, or the tool-list setting.
pub(super) fn availability_entry(
    availability: Option<&Availability>,
    dotenv: &octocode_native::config::EnvApplyReport,
) -> Value {
    let Some(availability) = availability.filter(|a| !a.is_available()) else {
        return json!({ "enabled": true });
    };
    let mut entry = json!({ "enabled": false });
    match availability {
        Availability::ToolList(setting) => entry["configuration"] = json!(setting.path),
        other => {
            let env_vars = other.env_vars();
            if let Some(hint) = dropped_key_hint(&env_vars, dotenv) {
                entry["hint"] = Value::String(hint);
            }
            if !env_vars.is_empty() {
                entry["envVar"] = Value::String(env_vars.join("|"));
            }
        }
    }
    entry
}

/// The machine catalog: runtime availability joined with the compact field
/// lists the embedded CLI presentation precomputes from the core Zod rows.
pub(super) fn machine_catalog(runtime: &ToolRuntime) -> Result<Value, String> {
    let catalog = runtime.catalog();
    let presentation = octocode_native::contracts::cli_presentation()
        .map_err(|error| format!("Embedded CLI presentation is invalid: {error}"))?;
    let presentation_tools = presentation.get("tools").and_then(Value::as_array);
    let dotenv = &runtime.config().dotenv;
    let tools = ToolId::ALL
        .into_iter()
        .map(|tool| {
            let name = tool.as_str();
            let fields = presentation_tools
                .and_then(|tools| tools.iter().find(|candidate| candidate["name"] == name))
                .map(|tool| super::schema::configured_tool(tool, &catalog))
                .and_then(|tool| tool["fields"].as_str())
                .unwrap_or("[]");
            json!({
                "name": name,
                "fields": fields,
                "availability": availability_entry(runtime.availability(name), dotenv),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "fingerprint": catalog["fingerprint"],
        "grammarCapabilities": catalog["grammarCapabilities"],
        // The language servers this runtime resolves: `schema lspSearch`
        // names them as MCP's description does.
        "lspServers": catalog["lspServers"],
        "presentation": catalog["presentation"],
        "tools": tools
    }))
}
