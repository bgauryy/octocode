//! Output references and the collection routes needed to reconstruct their pages.
use super::{SchemaError, presentation, tool_entry};
use serde_json::{Value, json};

/// What `tool`'s output fields and coverage reasons mean: the reference for
/// a field an answer holds. A reason also says what it means when it fires;
/// only the reasons this tool can emit are listed.
pub(super) fn output_view(tool: &Value) -> Result<Value, SchemaError> {
    let name = tool["name"].as_str().ok_or_else(|| SchemaError {
        message: "Embedded output reference has no tool name".to_owned(),
        exit: 5,
    })?;
    let contract = octocode_runtime_base::contracts::tool_contract_named(name)
        .ok_or_else(|| SchemaError {
            message: format!("Embedded contract has no tool {name}"),
            exit: 5,
        })?
        .map_err(|error| SchemaError {
            message: format!("Embedded tool contract is invalid: {error}"),
            exit: 5,
        })?;
    let reasons = octocode_runtime_base::id::ToolId::from_name(name)
        .and_then(octocode_runtime_base::contracts::partial_reasons_of);
    let mut output = json!({
        "name": tool["name"],
        "fields": tool["output"],
        "partialReasons": reasons,
    });
    // Clients need the same collection routes as the response pager: only
    // declared collections append across fragments; repeated context stays whole.
    for field in ["collectionPaths", "collectionRefs"] {
        if let Some(value) = contract.get(field) {
            output[field] = value.clone();
        }
    }
    Ok(output)
}

/// [`output_view`] of the tool named `name`, for its MCP resource.
pub(in crate::cli) fn output_of(name: &str) -> Option<Value> {
    let presentation = presentation().ok()?;
    tool_entry(presentation, name).and_then(|tool| output_view(tool).ok())
}
