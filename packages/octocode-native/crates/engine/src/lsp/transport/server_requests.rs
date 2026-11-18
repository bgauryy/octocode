//! Replies the client owes the server for server→client requests.

use crate::error::ErrorCode;
use serde_json::{Value, json};

/// What the client knows when the server asks it something.
#[derive(Clone)]
pub(crate) struct ClientRequestContext {
    /// Settings answered to `workspace/configuration`, looked up per
    /// `items[i].section`.
    pub(crate) configuration: Value,
    /// When set, `configuration` is already the *unwrapped* settings object of
    /// this section (rust-analyzer's and gopls's `initializationOptions` take
    /// the same keys as their `rust-analyzer` / `gopls` configuration
    /// section). A request for that section, or a dotted path under it, is then
    /// answered from `configuration` itself when it has no wrapping key.
    pub(crate) section_root: Option<String>,
    pub(crate) workspace_folders: Value,
}

/// A reply to a server→client request. Genuinely unknown methods map to
/// MethodNotFound (-32601) rather than a misleading `result: null`.
#[derive(Debug)]
pub(super) enum ClientResponse {
    Result(Value),
    Error { code: ErrorCode, message: String },
}

impl ClientResponse {
    /// The complete JSON-RPC response echoing `id` unchanged.
    pub(super) fn into_message(self, id: Value) -> Value {
        match self {
            Self::Result(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Self::Error { code, message } => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": code.as_i64(), "message": message },
            }),
        }
    }
}

/// Action titles with which Metals asks to import a build (`Messages.scala`
/// `ImportBuild`, `ImportBuildChanges`). Importing runs the project's build tool,
/// repository code, so the prompt is declined, and then Metals analyses only the
/// files open in it.
const BUILD_IMPORT_ACTIONS: [&str; 2] = ["Import build", "Import changes"];

/// What declining `method` costs, when it is a server's prompt to import its
/// build: said as the server's health, since the server reports nothing itself.
pub(super) fn declined_build_import(method: &str, params: Option<&Value>) -> Option<&'static str> {
    let asks_import = method == "window/showMessageRequest"
        && params
            .and_then(|params| params.get("actions"))
            .and_then(Value::as_array)
            .is_some_and(|actions| {
                actions.iter().any(|action| {
                    action
                        .get("title")
                        .and_then(Value::as_str)
                        .is_some_and(|title| BUILD_IMPORT_ACTIONS.contains(&title))
                })
            });
    asks_import.then_some(
        "the server asked to import the build and Octocode declined, because importing runs the project's build tool; it analyses only the files open in it, so uses in other files are missing.",
    )
}

pub(super) fn client_response_for(
    method: &str,
    params: Option<&Value>,
    context: &ClientRequestContext,
) -> ClientResponse {
    match method {
        "workspace/configuration" => {
            let items = params
                .and_then(|value| value.get("items"))
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default();
            ClientResponse::Result(Value::Array(
                items
                    .iter()
                    .map(|item| {
                        configuration_for_section(
                            context,
                            item.get("section").and_then(Value::as_str),
                        )
                    })
                    .collect(),
            ))
        }
        "workspace/workspaceFolders" => ClientResponse::Result(context.workspace_folders.clone()),
        "workspace/applyEdit" => ClientResponse::Result(json!({ "applied": false })),
        "client/registerCapability"
        | "client/unregisterCapability"
        | "window/showMessageRequest"
        | "window/workDoneProgress/create" => ClientResponse::Result(Value::Null),
        other => ClientResponse::Error {
            code: ErrorCode::MethodNotFound,
            message: format!("Method not found: {other}"),
        },
    }
}

/// Resolves one `ConfigurationItem.section`:
/// 1. no section → the whole configuration;
/// 2. the section (a flat key, or a dotted path walked object by object) found
///    in the configuration → that value;
/// 3. the section is [`ClientRequestContext::section_root`] (or a dotted path
///    under it) and the configuration has no wrapping key → answered from the
///    unwrapped configuration;
/// 4. otherwise `null` (unknown to us; the server uses its defaults).
fn configuration_for_section(context: &ClientRequestContext, section: Option<&str>) -> Value {
    let Some(section) = section.filter(|section| !section.is_empty()) else {
        return context.configuration.clone();
    };
    if let Some(value) = lookup_section(&context.configuration, section) {
        return value.clone();
    }
    if let Some(root) = context.section_root.as_deref() {
        if section == root {
            return context.configuration.clone();
        }
        if let Some(rest) = section
            .strip_prefix(root)
            .and_then(|rest| rest.strip_prefix('.'))
            && let Some(value) = lookup_section(&context.configuration, rest)
        {
            return value.clone();
        }
    }
    Value::Null
}

/// The value a server reads for a dotted `section` of its configuration: a
/// flat dotted key first, then the nested path. The one rule for every reader
/// of server options, so a gate never reads a setting differently from the
/// server it guards.
pub fn lookup_section<'a>(configuration: &'a Value, section: &str) -> Option<&'a Value> {
    if let Some(value) = configuration.get(section) {
        return Some(value);
    }
    section
        .split('.')
        .try_fold(configuration, |value, key| value.get(key))
}

#[cfg(test)]
mod tests;
