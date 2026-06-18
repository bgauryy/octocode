mod client;
mod config;
mod grammar;
mod json_rpc;
mod resolver;
mod types;
mod uri;
mod validation;
mod workspace;

use napi::{Error, Result, Status};
use napi_derive::napi;
use types::{JsFuzzyPosition, JsLanguageServerConfig, JsResolvedSymbol};

pub use client::NativeLspClient;

/// Resolve a fuzzy symbol position (name + optional line hint) to an exact
/// line/character position inside the file at `file_path`.
///
/// # Errors
///
/// Returns `napi::Error` when the file cannot be read or the symbol is not found.
#[napi(js_name = "resolvePosition")]
pub fn resolve_position(file_path: String, fuzzy: JsFuzzyPosition) -> Result<JsResolvedSymbol> {
    resolver::resolve_position(file_path, fuzzy)
}

/// Resolve a fuzzy symbol position against in-memory `content` rather than
/// reading from disk. Use when the caller already holds the file text.
///
/// # Errors
///
/// Returns `napi::Error` when the symbol cannot be located in the content.
#[napi(js_name = "resolvePositionFromContent")]
pub fn resolve_position_from_content(
    content: String,
    fuzzy: JsFuzzyPosition,
) -> Result<JsResolvedSymbol> {
    resolver::resolve_position_from_content(content, fuzzy)
}

/// Convert a filesystem path to a `file://` URI string.
///
/// # Errors
///
/// Returns `napi::Error` when the path cannot be encoded as a valid URI.
#[napi(js_name = "toUri")]
pub fn to_uri(path: String) -> Result<String> {
    uri::path_to_uri(&path)
}

/// Convert a `file://` URI string back to an absolute filesystem path.
///
/// # Errors
///
/// Returns `napi::Error` when the URI is malformed or does not use the `file` scheme.
#[napi(js_name = "fromUri")]
pub fn from_uri(uri: String) -> Result<String> {
    uri::uri_to_path(&uri)
}

/// Walk upward from `file_path` to find the workspace root (directory
/// containing a `package.json`, `.git`, or similar marker).
///
/// # Errors
///
/// Returns `napi::Error` when no workspace root is found above the given path.
#[napi(js_name = "resolveWorkspaceRootForFile")]
pub fn resolve_workspace_root_for_file(file_path: String) -> Result<String> {
    workspace::resolve_workspace_root_for_file(file_path)
}

/// Return the LSP language identifier for the file at `file_path` (e.g.
/// `"typescript"`, `"python"`). Returns `null` for unrecognised extensions.
#[napi(js_name = "detectLanguageId")]
pub fn detect_language_id(file_path: String) -> Option<String> {
    config::detect_language_id(file_path)
}

/// Return the default language server configuration for `file_path` inside
/// `workspace_root`. Returns `null` when no server is registered for the
/// file's language.
#[napi(js_name = "getLanguageServerForFile")]
pub fn get_language_server_for_file(
    file_path: String,
    workspace_root: String,
) -> Option<JsLanguageServerConfig> {
    config::default_server_for_file(file_path, workspace_root)
}

/// Check whether `command` is available on `PATH`.
///
/// # Errors
///
/// Returns `napi::Error` on unexpected I/O failure during the lookup.
#[napi(js_name = "isCommandAvailable")]
pub fn is_command_available(command: String) -> Result<bool> {
    config::is_command_available(command)
        .map_err(|e| Error::new(Status::GenericFailure, e))
}

/// Read `file_path` from disk after canonicalizing it and confirming it is an
/// absolute regular file.
///
/// # Errors
///
/// Returns `napi::Error` when the file cannot be read or fails validation.
#[napi(js_name = "safeReadFile")]
pub fn safe_read_file(file_path: String) -> Result<String> {
    validation::safe_read_file(file_path)
}

/// Validate that `command` resolves to an executable LSP server binary.
/// Returns the resolved absolute path on success and rejects shell wrappers.
///
/// # Errors
///
/// Returns `napi::Error` when the command is not found or is not executable.
#[napi(js_name = "validateLspServerPath")]
pub fn validate_lsp_server_path(command: String) -> Result<String> {
    validation::validate_lsp_server_path(command)
}

/// Convert an LSP `SymbolKind` numeric code to a human-readable string tag
/// (e.g. `12` → `"function"`, `5` → `"class"`). Unknown codes return `"unknown"`.
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

/// Convert a human-readable symbol kind string back to the LSP `SymbolKind`
/// numeric code (e.g. `"function"` → `12`). Unknown strings return `13` (Variable).
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
