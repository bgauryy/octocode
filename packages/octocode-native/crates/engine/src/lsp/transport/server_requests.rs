//! Replies the client owes the server for server→client requests.

use crate::error::ErrorCode;
use serde_json::{Value, json};
use std::path::Path;

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

/// The configuration section a server's `initializationOptions` stand for,
/// derived from the server command's file stem. `None` = options are not a
/// configuration section (sections are then looked up as keys).
pub(crate) fn configuration_section_for_command(command: &str) -> Option<String> {
    let stem = Path::new(command).file_stem()?.to_str()?;
    matches!(stem, "rust-analyzer" | "gopls").then(|| stem.to_owned())
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

fn lookup_section<'a>(configuration: &'a Value, section: &str) -> Option<&'a Value> {
    if let Some(value) = configuration.get(section) {
        return Some(value);
    }
    section
        .split('.')
        .try_fold(configuration, |value, key| value.get(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(configuration: Value, section_root: Option<&str>) -> ClientRequestContext {
        ClientRequestContext {
            configuration,
            section_root: section_root.map(str::to_owned),
            workspace_folders: json!([{ "uri": "file:///w", "name": "workspace" }]),
        }
    }

    fn configuration(context: &ClientRequestContext, items: Value) -> Value {
        match client_response_for(
            "workspace/configuration",
            Some(&json!({ "items": items })),
            context,
        ) {
            ClientResponse::Result(value) => value,
            other => panic!("configuration must be a result, got {other:?}"),
        }
    }

    #[test]
    fn configuration_answers_one_entry_per_item_by_section() {
        let wrapped = context(json!({"gopls": {"a": 1}, "other": true}), None);
        assert_eq!(
            configuration(
                &wrapped,
                json!([{"section": "gopls"}, {"section": "missing"}, {}, {"scopeUri": "file:///w"}])
            ),
            json!([{"a": 1}, null, {"gopls": {"a": 1}, "other": true}, {"gopls": {"a": 1}, "other": true}])
        );
    }

    #[test]
    fn configuration_resolves_unwrapped_server_section() {
        let unwrapped = context(
            json!({"checkOnSave": false, "cargo": {"features": []}}),
            Some("rust-analyzer"),
        );
        assert_eq!(
            configuration(
                &unwrapped,
                json!([
                    {"section": "rust-analyzer"},
                    {"section": "rust-analyzer.cargo"},
                    {"section": "rust-analyzer.missing"},
                    {"section": "gopls"}
                ])
            ),
            json!([
                {"checkOnSave": false, "cargo": {"features": []}},
                {"features": []},
                null,
                null
            ])
        );
        // Without a section root the same unwrapped object is NOT the section.
        let unknown = context(json!({"checkOnSave": false}), None);
        assert_eq!(
            configuration(&unknown, json!([{"section": "rust-analyzer"}])),
            json!([null])
        );
        // A wrapped object under a declared root still resolves the wrapper key.
        let wrapped = context(json!({"gopls": {"a": 1}}), Some("gopls"));
        assert_eq!(
            configuration(&wrapped, json!([{"section": "gopls"}])),
            json!([{"a": 1}])
        );
    }

    #[test]
    fn configuration_walks_dotted_sections_and_flat_keys() {
        let settings = context(
            json!({
                "java": {"format": {"tabSize": 4}},
                "python.analysis": {"diagnosticMode": "openFilesOnly"}
            }),
            None,
        );
        assert_eq!(
            configuration(
                &settings,
                json!([
                    {"section": "java.format.tabSize"},
                    {"section": "python.analysis"},
                    {"section": "java.format.missing"},
                    {"section": "java.format.tabSize.deeper"}
                ])
            ),
            json!([4, {"diagnosticMode": "openFilesOnly"}, null, null])
        );
        // No items (or a malformed params object) → an empty array.
        assert_eq!(configuration(&settings, json!([])), json!([]));
    }

    #[test]
    fn section_root_is_derived_from_the_command_stem() {
        assert_eq!(
            configuration_section_for_command("/usr/bin/rust-analyzer").as_deref(),
            Some("rust-analyzer")
        );
        assert_eq!(
            configuration_section_for_command("gopls.exe").as_deref(),
            Some("gopls")
        );
        assert_eq!(
            configuration_section_for_command("typescript-language-server"),
            None
        );
    }

    #[test]
    fn client_response_for_unknown_method_is_method_not_found() {
        let context = context(json!({ "settings": true }), None);
        match client_response_for("nonexistent/method", None, &context) {
            ClientResponse::Error { code, .. } => assert_eq!(code.as_i64(), -32601),
            other => panic!("expected -32601 MethodNotFound, got {other:?}"),
        }
        match client_response_for("workspace/workspaceFolders", None, &context) {
            ClientResponse::Result(value) => assert_eq!(value, context.workspace_folders),
            other => panic!("expected workspaceFolders result, got {other:?}"),
        }
        match client_response_for("workspace/applyEdit", None, &context) {
            ClientResponse::Result(value) => assert_eq!(value, json!({ "applied": false })),
            other => panic!("expected applyEdit result, got {other:?}"),
        }
        for method in [
            "client/registerCapability",
            "window/workDoneProgress/create",
        ] {
            match client_response_for(method, None, &context) {
                ClientResponse::Result(Value::Null) => {}
                other => panic!("{method} must stay a null result, got {other:?}"),
            }
        }
    }

    #[test]
    fn replies_echo_the_request_id_unchanged() {
        let reply = ClientResponse::Error {
            code: ErrorCode::MethodNotFound,
            message: "x".to_owned(),
        }
        .into_message(json!("abc"));
        assert_eq!(reply["id"], "abc");
        assert_eq!(reply["error"]["code"], -32601);
    }
}
