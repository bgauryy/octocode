use crate::lsp::symbol_kind;
use crate::lsp::types::{JsFuzzyPosition, JsLanguageServerConfig, JsResolvedSymbol};
use napi::{Error, Result, Status};
use napi_derive::napi;

/// Resolve a fuzzy symbol position (name + optional line hint) to an exact
/// line/character position inside the file at `file_path`.
#[napi(js_name = "resolvePosition")]
pub fn resolve_position(file_path: String, fuzzy: JsFuzzyPosition) -> Result<JsResolvedSymbol> {
    crate::lsp::resolver::resolve_position(file_path, fuzzy)
}

/// Resolve a fuzzy symbol position against in-memory `content` rather than
/// reading from disk. Use when the caller already holds the file text.
#[napi(js_name = "resolvePositionFromContent")]
pub fn resolve_position_from_content(
    content: String,
    fuzzy: JsFuzzyPosition,
) -> Result<JsResolvedSymbol> {
    crate::lsp::resolver::resolve_position_from_content(content, fuzzy)
}

/// Convert a filesystem path to a `file://` URI string.
#[napi(js_name = "toUri")]
pub fn to_uri(path: String) -> Result<String> {
    crate::lsp::uri::path_to_uri(&path)
}

/// Convert a `file://` URI string back to an absolute filesystem path.
#[napi(js_name = "fromUri")]
pub fn from_uri(uri: String) -> Result<String> {
    crate::lsp::uri::uri_to_path(&uri)
}

/// Walk upward from `file_path` to find the workspace root.
#[napi(js_name = "resolveWorkspaceRootForFile")]
pub fn resolve_workspace_root_for_file(file_path: String) -> Result<String> {
    crate::lsp::workspace::resolve_workspace_root_for_file(file_path)
}

/// Return the LSP language identifier for the file at `file_path`.
#[napi(js_name = "detectLanguageId")]
pub fn detect_language_id(file_path: String) -> Option<String> {
    crate::lsp::config::detect_language_id(file_path)
}

/// Return the default language server configuration for `file_path` inside
/// `workspace_root`.
#[napi(js_name = "getLanguageServerForFile")]
pub fn get_language_server_for_file(
    file_path: String,
    workspace_root: String,
) -> Option<JsLanguageServerConfig> {
    crate::lsp::config::default_server_for_file(file_path, workspace_root)
}

/// Check whether `command` is available on `PATH`.
#[napi(js_name = "isCommandAvailable")]
pub fn is_command_available(command: String) -> Result<bool> {
    crate::lsp::config::is_command_available(command)
        .map_err(|e| Error::new(Status::GenericFailure, e))
}

/// Read `file_path` from disk after canonicalizing it and confirming it is an
/// absolute regular file.
#[napi(js_name = "safeReadFile")]
pub fn safe_read_file(file_path: String) -> Result<String> {
    crate::lsp::validation::safe_read_file(file_path)
}

/// Read only a bounded line window around `line_zero_based` after canonicalizing
/// the path or file:// URI and confirming it is an absolute regular file.
#[napi(js_name = "safeReadLineWindow")]
pub fn safe_read_line_window(
    file_path: String,
    line_zero_based: u32,
    context_lines: u32,
) -> Result<String> {
    crate::lsp::validation::safe_read_line_window(file_path, line_zero_based, context_lines)
}

/// Validate that `command` resolves to an executable LSP server binary.
#[napi(js_name = "validateLspServerPath")]
pub fn validate_lsp_server_path(command: String) -> Result<String> {
    crate::lsp::validation::validate_lsp_server_path(command)
}

/// Convert an LSP `SymbolKind` numeric code to a human-readable string tag.
#[napi(js_name = "convertSymbolKind")]
pub fn convert_symbol_kind(kind: Option<u32>) -> String {
    symbol_kind::from_lsp_code(kind).to_owned()
}

/// Convert a human-readable symbol kind string back to the LSP `SymbolKind`
/// numeric code. Unknown strings return `13` (Variable).
#[napi(js_name = "toLspSymbolKind")]
pub fn to_lsp_symbol_kind(kind: String) -> u32 {
    symbol_kind::to_lsp_code(&kind)
}
