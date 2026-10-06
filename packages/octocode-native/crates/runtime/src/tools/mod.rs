pub mod artifact_search;
pub mod ast_graph;
pub mod ast_rewrite;
pub mod ast_rule;
pub mod ast_search;
pub mod cancel;
pub mod clasify;
pub mod gh_clone_repo;
pub mod gh_get_file_content;
pub mod gh_get_history_item;
pub mod gh_search_code;
pub mod gh_search_history;
pub mod gh_search_repo;
pub(crate) mod gh_shared;
pub mod gh_structure;
pub mod id;
pub(crate) mod line_spans;
pub mod local_fetch;
pub mod local_search;
pub mod lsp_search;
pub(crate) mod num;
pub mod numbered;
pub mod output;
pub mod result;
pub(crate) mod source;
pub mod stream_page;
pub mod structure_search;
pub(crate) mod symbol_outline;
#[cfg(test)]
pub(crate) mod test_support;

/// Listings this small lead to an outline read of their top file
/// (`read`, `minify:"symbols"`): a few files, so one more read costs
/// little.
pub(crate) const OUTLINE_LEAD_MAX_FILES: usize = 5;

/// Whether a `minify:"symbols"` read outlines `path`: source declarations or
/// document headings.
pub(crate) fn outlines(path: &str) -> bool {
    use crate::content::{FileType, classify_file_type};
    matches!(
        classify_file_type(path),
        Some(FileType::Code | FileType::Doc)
    )
}

/// The final component of `path` (lossy UTF-8), or "" when it has none.
pub(crate) fn display_name(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
