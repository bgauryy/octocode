#[cfg(feature = "napi-addon")]
mod bindings;

pub mod error;
pub mod portable;
pub mod security;
pub mod types;

pub use octocode_engine_core::{graph, index, lsp, minify, search, signatures, structural, text};

// Keep the NAPI-facing Rust surface explicit so the public addon ABI remains
// unchanged while implementation ownership lives in octocode-engine-core.
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
pub use octocode_engine_core::lsp::client::NativeLspClient;
