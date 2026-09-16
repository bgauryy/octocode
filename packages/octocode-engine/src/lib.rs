//! `octocode-engine` — Rust algorithms and Node.js N-API cdylib.
//!
//! This crate owns all engine primitives: search, LSP, signatures, structural
//! analysis, minification, security, graph, index, and text utilities.
//! When built with the `napi-addon` feature it also produces the platform
//! `.node` binary consumed by `octocode-tools-core`, `octocode`, and
//! `octocode-pi-extension`.
//!
//! `octocode-native` uses this crate as a pure `rlib` (no N-API) for the
//! native CLI and MCP runtime.

pub mod error;
pub mod graph;
pub mod index;
pub mod lsp;
pub mod minify;
pub mod portable;
pub mod search;
pub mod security;
pub mod signatures;
pub mod structural;
pub mod text;
pub mod types;

/// Node.js N-API function bindings. Only compiled when the `napi-addon`
/// feature is enabled.
#[cfg(feature = "napi-addon")]
pub mod bindings;

pub const ENGINE_CORE_API_VERSION: u32 = 1;

// ── N-API public surface ──────────────────────────────────────────────────────
// Explicit re-exports keep the addon ABI stable and make each symbol
// discoverable from the crate root.

#[cfg(feature = "napi-addon")]
pub use bindings::filesystem::query_file_system;
#[cfg(feature = "napi-addon")]
pub use bindings::graph::scan_graph_facts;
#[cfg(feature = "napi-addon")]
pub use bindings::index::{build_index, index_status, query_index};
#[cfg(feature = "napi-addon")]
pub use bindings::lsp::{
    acquire_pooled_lsp_client, clear_pooled_lsp_clients, configure_lsp_client_pool,
    convert_symbol_kind, detect_language_id, from_uri, get_language_server_for_file,
    is_command_available, pooled_lsp_client_configs, pooled_lsp_client_count,
    release_pooled_lsp_client, resolve_position, resolve_position_from_content,
    resolve_workspace_root_for_file, safe_read_file, safe_read_line_window, to_lsp_symbol_kind,
    to_uri, validate_lsp_server_path,
};
#[cfg(feature = "napi-addon")]
pub use bindings::minify::{apply_content_view_minification, minify_content};
#[cfg(feature = "napi-addon")]
pub use bindings::ripgrep::{parse_ripgrep_json, search_ripgrep, validate_ripgrep_pattern};
#[cfg(feature = "napi-addon")]
pub use bindings::security::{mask_sensitive_data, sanitize_content};
#[cfg(feature = "napi-addon")]
pub use bindings::signatures::{
    extract_graph_facts, extract_js_symbols, extract_signatures, find_in_file_references,
    get_grammar_capabilities, get_graph_fact_capabilities, get_semantic_boundary_offsets,
    get_supported_graph_fact_extensions, get_supported_js_ts_extensions,
    get_supported_signature_extensions, get_supported_structural_extensions, inspect_syntax_tree,
    structural_search, structural_search_detailed, structural_search_files,
    structural_search_files_detailed, SIGNATURES_ONLY_HINT,
};
#[cfg(feature = "napi-addon")]
pub use bindings::text::{
    byte_slice_content, byte_to_char_offset, char_to_byte_offset, extract_matching_lines,
    filter_patch, slice_content,
};
#[cfg(feature = "napi-addon")]
pub use bindings::yaml::json_to_yaml_string;
#[cfg(feature = "napi-addon")]
pub use lsp::client::NativeLspClient;
