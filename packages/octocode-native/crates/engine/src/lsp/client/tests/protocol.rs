//! Wire protocol: `initialize` parameters, position encoding, and retryable errors.

use super::*;
use crate::error::Error;
use crate::lsp::client::lifecycle::{extract_position_encoding, initialize_params};
use crate::lsp::client::requests::is_retryable_error;
use serde_json::json;

fn rpc_error(error: Value) -> Error {
    Error::rpc(crate::error::RpcError::from_value(error))
}

#[test]
fn content_modified_detected_by_typed_error_code() {
    assert!(is_retryable_error(&rpc_error(
        json!({"code": -32801, "message": "content modified"})
    )));
}

#[test]
fn content_modified_not_triggered_by_phrase_in_payload() {
    // A payload that merely mentions the phrase or the number, with a
    // different code, must NOT be treated as ContentModified.
    assert!(!is_retryable_error(&rpc_error(json!({
        "code": -32603,
        "message": "docs say: content modified by user (-32801)"
    }))));
}

#[test]
fn content_modified_not_triggered_by_other_codes_or_untyped_text() {
    assert!(!is_retryable_error(&rpc_error(
        json!({"code": -328011, "message": "x"})
    )));
    // Rendered text is never parsed back into a code.
    assert!(!is_retryable_error(&Error::new(
        "LSP error: {\"code\":-32801,\"message\":\"content modified\"}",
    )));
}

#[test]
fn server_cancelled_is_retried_only_when_the_server_asks_for_a_retrigger() {
    assert!(is_retryable_error(&rpc_error(json!({
        "code": -32802, "message": "cancelled", "data": {"retriggerRequest": true}
    }))));
    assert!(!is_retryable_error(&rpc_error(
        json!({"code": -32802, "message": "cancelled"})
    )));
    assert!(!is_retryable_error(&rpc_error(json!({
        "code": -32802, "message": "cancelled", "data": {"retriggerRequest": false}
    }))));
    assert!(!is_retryable_error(&rpc_error(
        json!({"code": -32800, "message": "request cancelled"})
    )));
}

#[test]
fn extract_position_encoding_reads_server_choice() {
    // Server echoes the negotiated encoding.
    let result = json!({ "capabilities": { "positionEncoding": "utf-16" } });
    assert_eq!(
        extract_position_encoding(&result).as_deref(),
        Some("utf-16")
    );

    // A non-conformant server that ignored our utf-16-only advertisement.
    let result = json!({ "capabilities": { "positionEncoding": "utf-8" } });
    assert_eq!(extract_position_encoding(&result).as_deref(), Some("utf-8"));

    // Omitted ⇒ None (spec default is utf-16).
    let result = json!({ "capabilities": {} });
    assert_eq!(extract_position_encoding(&result), None);

    // No capabilities at all.
    assert_eq!(extract_position_encoding(&json!({})), None);
}

#[test]
fn initialize_advertises_push_diagnostics_so_servers_publish_them() {
    let config = LspLanguageServerConfig {
        command: "typescript-language-server".into(),
        args: Some(vec!["--stdio".into()]),
        workspace_root: std::env::temp_dir().to_string_lossy().into_owned(),
        language_id: Some("typescript".into()),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    };
    let params = initialize_params(&config).expect("initialize params");
    assert_eq!(
        params.pointer("/capabilities/textDocument/publishDiagnostics/versionSupport"),
        Some(&json!(true)),
        "{params}"
    );
}

#[test]
fn initialize_declares_only_what_the_client_handles() {
    let params = initialize_params(&ts_config()).expect("initialize params");
    let stale = params
        .pointer("/capabilities/general/staleRequestSupport")
        .expect("staleRequestSupport");
    assert_eq!(stale["cancel"], json!(true));
    let retried: Vec<&str> = stale["retryOnContentModified"]
        .as_array()
        .expect("method list")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for method in [
        "textDocument/definition",
        "textDocument/references",
        "textDocument/hover",
        "callHierarchy/incomingCalls",
        "workspace/symbol",
        "textDocument/diagnostic",
    ] {
        assert!(
            retried.contains(&method),
            "{method} is retried: {retried:?}"
        );
    }
    // We never send didSave, so we must not claim it.
    assert_eq!(
        params.pointer("/capabilities/textDocument/synchronization/didSave"),
        None
    );
    // serverStatus is rust-analyzer's extension only.
    assert_eq!(params.pointer("/capabilities/experimental"), None);
    assert_eq!(
        params.pointer("/capabilities/general/positionEncodings"),
        Some(&json!(["utf-16"]))
    );

    let mut rust = ts_config();
    rust.command = "/home/u/.cargo/bin/rust-analyzer".into();
    let params = initialize_params(&rust).expect("initialize params");
    assert_eq!(
        params.pointer("/capabilities/experimental/serverStatusNotification"),
        Some(&json!(true))
    );
}

#[test]
fn rust_analyzer_client_starts_headless_unless_the_caller_overrides() {
    let mut config = ts_config();
    config.command = "rust-analyzer".into();
    let client = NativeLspClient::new(config.clone());
    let options = client
        .inner
        .config
        .initialization_options
        .clone()
        .expect("headless defaults");
    assert_eq!(options.pointer("/procMacro/enable"), Some(&json!(false)));
    assert_eq!(
        options.pointer("/cargo/buildScripts/enable"),
        Some(&json!(false))
    );
    assert_eq!(options.pointer("/checkOnSave"), Some(&json!(false)));
    assert_eq!(options.pointer("/cachePriming/enable"), Some(&json!(false)));

    config.initialization_options = Some(json!({"cargo": {"buildScripts": {"enable": true}}}));
    let client = NativeLspClient::new(config);
    let options = client
        .inner
        .config
        .initialization_options
        .clone()
        .expect("options");
    assert_eq!(
        options.pointer("/cargo/buildScripts/enable"),
        Some(&json!(true))
    );
    assert_eq!(options.pointer("/procMacro/enable"), Some(&json!(false)));

    // typescript-language-server runs headless too (see
    // `typescript_headless_options`); servers with no defaults are untouched.
    let ts = NativeLspClient::new(ts_config());
    assert_eq!(
        ts.inner
            .config
            .initialization_options
            .as_ref()
            .and_then(|options| options.get("disableAutomaticTypingAcquisition")),
        Some(&json!(true))
    );
    let mut other = ts_config();
    other.command = "gopls".into();
    assert!(
        NativeLspClient::new(other)
            .inner
            .config
            .initialization_options
            .is_none()
    );
}
