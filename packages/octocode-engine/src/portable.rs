//! N-API-free entry points for native Rust consumers.

use crate::error::{Error, Result, Status};
use crate::types::{
    ExtractMatchingLinesOptions, ExtractMatchingLinesResult, FileSystemQueryOptions,
    FileSystemQueryResult, FilterPatchOptions, GrammarCapability, GraphFactsScanOptions,
    GraphFactsScanResult, MinifyResult, RipgrepParseOptions, RipgrepParseResult,
    RipgrepSearchOptions, SliceContentOptions, SliceContentResult, YamlConversionConfig,
};

#[must_use]
pub fn minify_content(content: &str, file_path: &str) -> MinifyResult {
    crate::minify::minifier::minify_content_result_inner(content, file_path)
}

#[must_use]
pub fn apply_content_view_minification(content: &str, file_path: &str) -> String {
    let content = content.to_owned();
    let file_path = file_path.to_owned();
    crate::signatures::run_on_deep_stack(move || {
        crate::minify::apply::apply_content_view_minification_inner(&content, &file_path)
    })
}

pub fn query_file_system(options: FileSystemQueryOptions) -> Result<FileSystemQueryResult> {
    crate::search::fs_query::query_file_system_inner(options)
        .map_err(|message| Error::new(Status::InvalidArg, message))
}

/// Apply caller policy before inspecting or counting each descendant. Denied
/// directories are pruned; callback errors abort traversal (e.g. cancellation).
pub fn query_file_system_filtered(
    options: FileSystemQueryOptions,
    allow_path: &dyn Fn(&std::path::Path) -> std::result::Result<bool, String>,
) -> Result<FileSystemQueryResult> {
    crate::search::fs_query::query_file_system_filtered_inner(options, allow_path)
        .map_err(|message| Error::new(Status::InvalidArg, message))
}

pub fn scan_graph_facts(options: GraphFactsScanOptions) -> Result<GraphFactsScanResult> {
    crate::graph::scan_graph_facts(options)
        .map_err(|message| Error::new(Status::InvalidArg, message))
}

pub fn scan_graph_facts_filtered(
    options: GraphFactsScanOptions,
    allow_path: &(dyn Fn(&std::path::Path) -> std::result::Result<bool, String> + Sync),
) -> Result<GraphFactsScanResult> {
    crate::graph::scan_graph_facts_filtered(options, allow_path)
        .map_err(|message| Error::new(Status::InvalidArg, message))
}

#[must_use]
pub fn parse_ripgrep_json(
    stdout: &str,
    options: Option<RipgrepParseOptions>,
) -> RipgrepParseResult {
    crate::search::ripgrep_parser::parse_ripgrep_json_inner(stdout, options)
}

pub fn search_ripgrep(options: RipgrepSearchOptions) -> Result<RipgrepParseResult> {
    crate::search::ripgrep_search::search(options)
}

pub use crate::search::ripgrep_search::RipgrepPathFilter;
pub fn search_ripgrep_filtered(
    options: RipgrepSearchOptions,
    path_filter: std::sync::Arc<dyn RipgrepPathFilter>,
) -> Result<RipgrepParseResult> {
    crate::search::ripgrep_search::search_filtered(options, path_filter)
}

#[must_use]
pub fn validate_ripgrep_pattern(
    pattern: &str,
    fixed_string: bool,
    perl_regex: bool,
) -> crate::search::ripgrep_pattern::RipgrepPatternValidationResult {
    crate::search::ripgrep_pattern::validate(pattern, fixed_string, perl_regex)
}

pub fn sanitize_content(
    content: &str,
    file_path: Option<&str>,
) -> Result<crate::security::types::SanitizationResult> {
    std::panic::catch_unwind(|| crate::security::sanitizer::sanitize_content(content, file_path))
        .map_err(|_| {
            Error::new(
                Status::InvalidArg,
                "content sanitization failed on pathological input",
            )
        })
}

#[must_use]
pub fn mask_sensitive_data(text: String) -> String {
    crate::security::detector::mask_text(text)
}

#[must_use]
pub fn extract_signatures(content: &str, file_path: &str) -> Option<String> {
    let content = content.to_owned();
    let file_path = file_path.to_owned();
    crate::signatures::run_on_deep_stack(move || {
        crate::signatures::extract_signatures_inner(&content, &file_path)
    })
}

#[must_use]
pub fn extract_js_symbols(content: &str, file_path: &str) -> Option<String> {
    crate::signatures::js_oxc::extract_js_symbols(content, file_path)
}

#[must_use]
pub fn find_in_file_references(
    content: &str,
    file_path: &str,
    line: u32,
    character: u32,
) -> Option<String> {
    crate::signatures::js_oxc::find_in_file_references(content, file_path, line, character)
}

#[must_use]
pub fn extract_graph_facts(content: &str, file_path: &str) -> Option<String> {
    crate::signatures::extract_graph_facts_inner(content, file_path)
}

#[must_use]
pub fn supported_js_ts_extensions() -> Vec<String> {
    crate::text::file_extension::JS_TS_EXTENSIONS
        .iter()
        .map(|extension| (*extension).to_owned())
        .collect()
}

#[must_use]
pub fn supported_graph_fact_extensions() -> Vec<String> {
    crate::signatures::graph_facts::graph_fact_extensions()
}

#[must_use]
pub fn graph_fact_capabilities() -> String {
    crate::signatures::graph_facts::graph_fact_capabilities_json()
}

#[must_use]
pub fn grammar_capabilities() -> Vec<GrammarCapability> {
    crate::signatures::languages::all_entries()
        .iter()
        .map(|entry| GrammarCapability {
            language: entry.name.to_owned(),
            language_id: entry.language_id.map(str::to_owned),
            selector_aliases: entry
                .selector_aliases
                .iter()
                .map(|alias| (*alias).to_owned())
                .collect(),
            extensions: entry
                .extensions
                .iter()
                .map(|extension| (*extension).to_owned())
                .collect(),
            structural_search: true,
            signature_outline: !entry.body_query.is_empty(),
            graph_facts: !entry.body_query.is_empty(),
        })
        .collect()
}

pub fn structural_search(
    content: &str,
    file_path: &str,
    pattern: Option<&str>,
    rule: Option<&str>,
) -> Result<Vec<crate::structural::StructuralMatch>> {
    let extension = crate::text::file_extension::get_extension_internal(file_path, true, "txt");
    std::panic::catch_unwind(|| crate::structural::search(content, &extension, pattern, rule))
        .unwrap_or_else(|_| Err("structural search failed on pathological input".to_owned()))
        .map_err(|message| Error::new(Status::InvalidArg, message))
}

pub fn structural_search_detailed(
    content: &str,
    file_path: &str,
    pattern: Option<&str>,
    rule: Option<&str>,
) -> Result<crate::structural::StructuralSearchDetailedResult> {
    let extension = crate::text::file_extension::get_extension_internal(file_path, true, "txt");
    std::panic::catch_unwind(|| {
        crate::structural::search_detailed(content, file_path, &extension, pattern, rule)
    })
    .map_err(|_| {
        Error::new(
            Status::GenericFailure,
            "structural detailed search failed on pathological input",
        )
    })
}

pub fn structural_search_files(
    options: crate::structural::StructuralSearchFilesOptions,
) -> Result<crate::structural::StructuralSearchFilesResult> {
    std::panic::catch_unwind(|| crate::structural::search_files(options))
        .unwrap_or_else(|_| Err("structural file search failed on pathological input".to_owned()))
        .map_err(|message| Error::new(Status::InvalidArg, message))
}

pub fn structural_search_files_detailed(
    options: crate::structural::StructuralSearchFilesOptions,
) -> Result<crate::structural::StructuralSearchFilesDetailedResult> {
    std::panic::catch_unwind(|| crate::structural::search_files_detailed(options))
        .unwrap_or_else(|_| {
            Err("structural detailed file search failed on pathological input".to_owned())
        })
        .map_err(|message| Error::new(Status::InvalidArg, message))
}

/// Run detailed structural search while consulting caller policy before
/// candidate accounting, prefilter reads, metadata reads, and source reads.
pub fn structural_search_files_detailed_filtered(
    options: crate::structural::StructuralSearchFilesOptions,
    allow_path: &(dyn Fn(&std::path::Path) -> std::result::Result<bool, String> + Sync),
) -> Result<crate::structural::StructuralSearchFilesDetailedResult> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::structural::search_files_detailed_filtered(options, allow_path)
    }))
    .unwrap_or_else(|_| {
        Err("structural detailed file search failed on pathological input".to_owned())
    })
    .map_err(|message| Error::new(Status::InvalidArg, message))
}

#[cfg(feature = "embedded-ast-grep-rewrite")]
pub fn structural_rewrite(
    content: &str,
    rule_config: serde_json::Value,
) -> Result<Vec<crate::structural::StructuralRewriteMatch>> {
    std::panic::catch_unwind(|| crate::structural::structural_rewrite(content, rule_config))
        .unwrap_or_else(|_| Err("structural rewrite failed on pathological input".to_owned()))
        .map_err(|message| Error::new(Status::InvalidArg, message))
}

#[must_use]
pub fn supported_structural_extensions() -> Vec<String> {
    crate::structural::supported_extensions()
}

pub fn inspect_syntax_tree(
    content: &str,
    file_path: &str,
    options: Option<crate::structural::SyntaxTreeInspectOptions>,
) -> Result<crate::structural::SyntaxTreeInspectResult> {
    std::panic::catch_unwind(|| crate::structural::inspect_syntax_tree(content, file_path, options))
        .map_err(|_| {
            Error::new(
                Status::GenericFailure,
                "syntax-tree inspection failed on pathological input",
            )
        })
}

pub fn semantic_boundary_offsets(content: &str, file_path: &str) -> Result<Vec<u32>> {
    std::panic::catch_unwind(|| {
        crate::signatures::get_semantic_boundary_offsets_inner(content, file_path)
    })
    .map_err(|_| {
        Error::new(
            Status::GenericFailure,
            "semantic boundary detection failed on pathological input",
        )
    })
}

#[must_use]
pub fn supported_signature_extensions() -> Vec<String> {
    let mut extensions: Vec<String> = crate::signatures::languages::signature_extensions()
        .into_iter()
        .map(str::to_owned)
        .collect();
    extensions.sort();
    extensions
}

#[must_use]
pub fn char_to_byte_offset(content: &str, char_index: usize) -> usize {
    crate::text::utf8_offsets::char_to_byte_offset_inner(content, char_index)
}

#[must_use]
pub fn byte_to_char_offset(content: &str, byte_offset: usize) -> usize {
    crate::text::utf8_offsets::byte_to_char_offset_inner(content, byte_offset)
}

#[must_use]
pub fn byte_slice_content(content: &str, byte_start: usize, byte_end: usize) -> String {
    crate::text::utf8_offsets::byte_slice_content_inner(content, byte_start, byte_end)
}

#[must_use]
pub fn slice_content(
    content: &str,
    char_offset: usize,
    char_length: usize,
    options: Option<SliceContentOptions>,
) -> SliceContentResult {
    crate::text::utf8_offsets::slice_content_inner(content, char_offset, char_length, options)
}

pub fn extract_matching_lines(
    content: &str,
    pattern: &str,
    options: Option<ExtractMatchingLinesOptions>,
) -> Result<ExtractMatchingLinesResult> {
    let is_regex = options
        .as_ref()
        .and_then(|value| value.is_regex)
        .unwrap_or(false);
    if is_regex && !pattern.is_empty() {
        let case_sensitive = options
            .as_ref()
            .and_then(|value| value.case_sensitive)
            .unwrap_or(false);
        regex::RegexBuilder::new(pattern)
            .case_insensitive(!case_sensitive)
            .build()
            .map_err(|error| {
                Error::new(
                    Status::InvalidArg,
                    format!("invalid regex pattern: {error}"),
                )
            })?;
    }
    Ok(crate::search::line_extractor::extract_matching_lines_inner(
        content, pattern, options,
    ))
}

#[must_use]
pub fn filter_patch(patch: &str, options: Option<FilterPatchOptions>) -> String {
    crate::text::diff_parser::filter_patch_inner(patch, options)
}

#[must_use]
pub fn json_to_yaml_string(
    value: serde_json::Value,
    config: Option<YamlConversionConfig>,
) -> String {
    let sort_keys = config
        .as_ref()
        .and_then(|value| value.sort_keys)
        .unwrap_or(false);
    let priority = config
        .as_ref()
        .and_then(|value| value.keys_priority.as_deref())
        .map(<[_]>::to_vec)
        .unwrap_or_default();
    crate::text::yaml_utils::json_to_yaml_string_inner(value, sort_keys, &priority)
}
