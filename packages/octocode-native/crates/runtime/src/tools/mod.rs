pub(crate) mod artifact_search;
pub mod ast_graph;
pub(crate) mod ast_rule;
pub(crate) mod ast_search;
pub(crate) mod bounded_process;
pub(crate) mod clasify;
pub(crate) mod declarations_cache;
pub(crate) mod empty_walk;
pub(crate) mod gh_clone_repo;
pub(crate) mod gh_get_file_content;
pub(crate) mod gh_get_history_item;
pub(crate) mod gh_search_code;
pub(crate) mod gh_search_history;
pub(crate) mod gh_search_repo;
pub(crate) mod gh_shared;
pub(crate) mod gh_structure;
pub(crate) mod judgment;
pub(crate) mod leads;
pub(crate) mod line_spans;
pub(crate) mod local_fetch;
pub(crate) mod local_search;
pub(crate) mod lsp_search;
pub(crate) mod match_keys;
pub(crate) mod match_list;
#[cfg(test)]
mod module_boundaries;
pub(crate) mod page_memo;
pub(crate) mod root_read;
pub(crate) mod selection;
pub(crate) mod source;
pub(crate) mod source_capture;
pub(crate) mod source_sections;
pub(crate) mod structure_search;
#[cfg(test)]
pub(crate) mod test_support;
pub(crate) mod variant_field;
pub(crate) mod workers;

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

/// The directory names a tool's own file walk prunes: the syntax-visible
/// defaults (dependency/build output, caches, VCS metadata, credential
/// stores; [`crate::policy::prune`]) plus the tool's `extra`
/// names.
pub(crate) fn syntax_prune(
    extra: impl IntoIterator<Item = String>,
) -> octocode_engine::search::walk::Prune {
    octocode_engine::search::walk::Prune::new(
        crate::policy::prune::defaults()
            .map(str::to_owned)
            .chain(extra),
    )
}

/// The engine walk a tool's own file walk builds: the default ignore policy
/// (ignore rules honored, dot entries skipped), never descending into a
/// `prune`d directory (the root itself is always walked).
pub(crate) fn pruned_walk(
    root: &std::path::Path,
    prune: octocode_engine::search::walk::Prune,
) -> ignore::WalkBuilder {
    octocode_engine::search::walk::pruned_walk_builder(
        root,
        octocode_engine::search::walk::IgnorePolicy::default(),
        None,
        prune,
    )
}
