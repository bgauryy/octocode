mod bindings;
mod graph;
mod lsp;
mod minify;
mod search;
mod security;
mod signatures;
mod structural;
mod text;
mod types;

// Keep the NAPI-facing Rust surface explicit. These exports also make FFI-only
// entry points reachable to Rust's dead-code analysis and the benchmark crates.
pub use bindings::filesystem::query_file_system;
pub use bindings::graph::scan_graph_facts;
pub use bindings::lsp::{
    convert_symbol_kind, detect_language_id, from_uri, get_language_server_for_file,
    is_command_available, resolve_position, resolve_position_from_content,
    resolve_workspace_root_for_file, safe_read_file, safe_read_line_window, to_lsp_symbol_kind,
    to_uri, validate_lsp_server_path,
};
pub use bindings::minify::{apply_content_view_minification, minify_content};
pub use bindings::ripgrep::{parse_ripgrep_json, search_ripgrep, validate_ripgrep_pattern};
pub use bindings::security::{mask_sensitive_data, sanitize_content};
pub use bindings::signatures::{
    extract_graph_facts, extract_js_symbols, extract_signatures, find_in_file_references,
    get_grammar_capabilities, get_graph_fact_capabilities, get_semantic_boundary_offsets,
    get_supported_graph_fact_extensions, get_supported_js_ts_extensions,
    get_supported_signature_extensions, get_supported_structural_extensions, inspect_syntax_tree,
    structural_search, structural_search_detailed, structural_search_files,
    structural_search_files_detailed, SIGNATURES_ONLY_HINT,
};
pub use bindings::text::{
    byte_slice_content, byte_to_char_offset, char_to_byte_offset, extract_matching_lines,
    filter_patch, slice_content,
};
pub use bindings::yaml::json_to_yaml_string;
pub use lsp::client::NativeLspClient;
