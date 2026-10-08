//! Panic-safe, transport-neutral entry points shared by Rust consumers.

use crate::error::{Error, Result};
use crate::security::types::SanitizationResult;
use crate::types::{
    FileSystemQueryOptions, FileSystemQueryResult, GrammarCapability, GraphFactsScanOptions,
    RipgrepParseResult, RipgrepSearchOptions, YamlConversionConfig,
};

pub use crate::search::ripgrep_pattern::{
    RipgrepPatternValidationResult, validate_ripgrep_pattern,
};
pub use crate::signatures::{
    extract_declarations, extract_graph_facts, extract_graph_facts_with_extension,
};
pub use crate::text::diff_parser::filter_patch;

/// Contain a panic from a fallible engine entry point at the transport-neutral
/// boundary. An unguarded panic here (deep in a parser or filesystem walk on
/// pathological input) would unwind into the caller, and across the runtime
/// addon's FFI boundary it aborts the host process. Converting it to an `Err`
/// keeps the failure catchable. Inner implementations that already guard are
/// unaffected — a nested `catch_unwind` is harmless.
fn guard_panic<T>(what: &str, call: impl FnOnce() -> Result<T>) -> Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(call))
        .unwrap_or_else(|_| Err(Error::new(format!("{what} failed on pathological input"))))
}

#[must_use]
pub fn apply_content_view_minification(content: &str, file_path: &str) -> String {
    let file_path = file_path.to_owned();
    transform_on_deep_stack(content, move |content| {
        crate::minify::apply::apply_content_view_minification_inner(content, &file_path)
    })
}

/// Run a content transform on the deep-stack pool. A panic or an unavailable
/// pool yields `None` there, so the caller gets the original content rather
/// than `String::default()`, an empty view (review L13).
fn transform_on_deep_stack(
    content: &str,
    transform: impl FnOnce(&str) -> String + Send + 'static,
) -> String {
    let shared: std::sync::Arc<str> = content.into();
    let job_content = std::sync::Arc::clone(&shared);
    crate::signatures::run_on_deep_stack(move || Some(transform(&job_content)))
        .unwrap_or_else(|| shared.to_string())
}

/// Apply caller policy before inspecting or counting each descendant. Denied
/// directories are pruned; callback errors abort traversal (e.g. cancellation).
pub fn query_file_system_filtered(
    options: FileSystemQueryOptions,
    allow_path: &dyn Fn(&std::path::Path) -> std::result::Result<bool, String>,
) -> Result<FileSystemQueryResult> {
    guard_panic("filesystem query", || {
        crate::search::fs_query::query_file_system_filtered_inner(options, allow_path)
            .map_err(Error::new)
    })
}

/// A walk's policy callback: the path and its directory-entry type (`None`
/// for the root or when the platform did not report it).
pub type FileSystemEntryFilter<'a> =
    dyn Fn(&std::path::Path, Option<std::fs::FileType>) -> std::result::Result<bool, String> + 'a;

/// [`query_file_system_filtered`] whose callback also gets each descendant's
/// directory-entry type, so the policy check need not stat the path itself.
pub fn query_file_system_typed(
    options: FileSystemQueryOptions,
    allow_path: &FileSystemEntryFilter<'_>,
) -> Result<FileSystemQueryResult> {
    guard_panic("filesystem query", || {
        crate::search::fs_query::query_file_system_typed_inner(options, allow_path)
            .map_err(Error::new)
    })
}

pub fn scan_typed_graph_facts_filtered(
    options: GraphFactsScanOptions,
    allow_path: &(dyn Fn(&std::path::Path) -> std::result::Result<bool, String> + Sync),
) -> Result<crate::graph::GraphFactsTypedScanResult> {
    guard_panic("graph-facts scan", || {
        crate::graph::scan_graph_facts_typed_filtered(options, allow_path).map_err(Error::new)
    })
}

pub use crate::search::pcre2_ranges::{Pcre2RangesError, pcre2_find_ranges};
pub use crate::search::ripgrep_search::RipgrepPathFilter;

/// Ripgrep search restricted by `path_filter` that stops walking at the next
/// entry once `cancelled` returns true; the partial result carries `capReason`
/// `cancelled`.
pub fn search_ripgrep_cancellable(
    options: RipgrepSearchOptions,
    path_filter: std::sync::Arc<dyn RipgrepPathFilter>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<RipgrepParseResult> {
    guard_panic("ripgrep search", || {
        crate::search::ripgrep_search::search_cancellable(options, path_filter, cancelled)
    })
}

pub fn sanitize_content(content: &str, file_path: Option<&str>) -> Result<SanitizationResult> {
    std::panic::catch_unwind(|| crate::security::sanitizer::sanitize_content(content, file_path))
        .map_err(|_| Error::new("content sanitization failed on pathological input"))
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

pub fn structural_search_detailed(
    content: &str,
    file_path: &str,
    pattern: Option<&str>,
    rule: Option<&str>,
) -> Result<crate::structural::StructuralSearchDetailedResult> {
    let extension = crate::text::file_extension::extension_of(file_path, true, "txt");
    structural_search_detailed_with_extension(content, file_path, &extension, pattern, rule)
}

/// Parse with an explicitly selected grammar while retaining the real source path
/// in match IDs, diagnostics, and returned locations.
pub fn structural_search_detailed_with_extension(
    content: &str,
    file_path: &str,
    extension: &str,
    pattern: Option<&str>,
    rule: Option<&str>,
) -> Result<crate::structural::StructuralSearchDetailedResult> {
    std::panic::catch_unwind(|| {
        crate::structural::search_detailed(content, file_path, extension, pattern, rule)
    })
    .map_err(|_| Error::new("structural detailed search failed on pathological input"))
}

pub fn structural_search_files_detailed_filtered_with_extension(
    options: crate::structural::StructuralSearchFilesOptions,
    allow_path: &(dyn Fn(&std::path::Path) -> std::result::Result<bool, String> + Sync),
    select_extension: &(dyn Fn(&std::path::Path) -> String + Sync),
) -> Result<crate::structural::StructuralSearchFilesDetailedResult> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::structural::search_files_detailed_filtered_with_extension(
            options,
            allow_path,
            select_extension,
        )
    }))
    .unwrap_or_else(|_| {
        Err("structural detailed file search failed on pathological input".to_owned())
    })
    .map_err(Error::new)
}

pub fn inspect_syntax_tree_with_extension(
    content: &str,
    file_path: &str,
    extension: Option<&str>,
    options: Option<crate::structural::SyntaxTreeInspectOptions>,
) -> Result<crate::structural::SyntaxTreeInspectResult> {
    std::panic::catch_unwind(|| {
        crate::structural::inspect_with_extension(content, file_path, extension, options)
    })
    .map_err(|_| Error::new("syntax-tree inspection failed on pathological input"))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Review L13: a transform that panics (or a pool that cannot run it)
    /// yields the original content, never an empty content view.
    #[test]
    fn failed_deep_stack_transform_returns_the_original_content() {
        let out = transform_on_deep_stack("fn keep() {}", |_| panic!("minifier bug"));
        assert_eq!(out, "fn keep() {}");
        assert_eq!(
            transform_on_deep_stack("a  b", |content| content.replace("  ", " ")),
            "a b"
        );
    }
}
