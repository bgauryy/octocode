#![allow(clippy::needless_pass_by_value)]

mod client;
mod config;
mod json_rpc;
mod resolver;
mod types;
mod uri;
mod validation;
mod workspace;

use napi::Result;
use napi_derive::napi;
use types::{JsFuzzyPosition, JsLanguageServerConfig, JsResolvedSymbol};

pub use client::NativeLspClient;

#[napi(js_name = "resolvePosition")]
pub fn resolve_position(file_path: String, fuzzy: JsFuzzyPosition) -> Result<JsResolvedSymbol> {
    resolver::resolve_position(file_path, fuzzy)
}

#[napi(js_name = "resolvePositionFromContent")]
pub fn resolve_position_from_content(
    content: String,
    fuzzy: JsFuzzyPosition,
) -> Result<JsResolvedSymbol> {
    resolver::resolve_position_from_content(content, fuzzy)
}

#[napi(js_name = "toUri")]
pub fn to_uri(path: String) -> Result<String> {
    uri::path_to_uri(&path)
}

#[napi(js_name = "fromUri")]
pub fn from_uri(uri: String) -> Result<String> {
    uri::uri_to_path(&uri)
}

#[napi(js_name = "resolveWorkspaceRootForFile")]
pub fn resolve_workspace_root_for_file(file_path: String) -> Result<String> {
    workspace::resolve_workspace_root_for_file(file_path)
}

#[napi(js_name = "detectLanguageId")]
pub fn detect_language_id(file_path: String) -> Option<String> {
    config::detect_language_id(file_path)
}

#[napi(js_name = "getLanguageServerForFile")]
pub fn get_language_server_for_file(
    file_path: String,
    workspace_root: String,
) -> Option<JsLanguageServerConfig> {
    config::default_server_for_file(file_path, workspace_root)
}

#[napi(js_name = "isCommandAvailable")]
pub fn is_command_available(command: String) -> Result<bool> {
    config::is_command_available(command)
}

#[napi(js_name = "safeReadFile")]
pub fn safe_read_file(file_path: String) -> Result<String> {
    validation::safe_read_file(file_path)
}

#[napi(js_name = "validateLspServerPath")]
pub fn validate_lsp_server_path(command: String) -> Result<String> {
    validation::validate_lsp_server_path(command)
}

#[napi(js_name = "convertSymbolKind")]
pub fn convert_symbol_kind(kind: Option<u32>) -> String {
    match kind {
        Some(1) | Some(2) | Some(4) => "module".to_owned(),
        Some(3) => "namespace".to_owned(),
        Some(5) | Some(19) | Some(23) => "class".to_owned(),
        Some(6) | Some(9) => "method".to_owned(),
        Some(7) | Some(8) | Some(20) => "property".to_owned(),
        Some(10) => "enum".to_owned(),
        Some(11) => "interface".to_owned(),
        Some(12) => "function".to_owned(),
        Some(13) => "variable".to_owned(),
        Some(14) | Some(22) => "constant".to_owned(),
        Some(26) => "type".to_owned(),
        _ => "unknown".to_owned(),
    }
}

#[napi(js_name = "toLspSymbolKind")]
pub fn to_lsp_symbol_kind(kind: String) -> u32 {
    match kind.as_str() {
        "function" => 12,
        "method" => 6,
        "class" => 5,
        "interface" => 11,
        "type" => 26,
        "variable" => 13,
        "constant" => 14,
        "property" => 7,
        "enum" => 10,
        "module" => 2,
        "namespace" => 3,
        _ => 13,
    }
}
