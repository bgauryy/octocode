pub(crate) mod artifact_search;
pub mod ast_graph;
pub(crate) mod ast_rewrite;
pub(crate) mod ast_rule;
pub(crate) mod ast_search;
pub(crate) mod bounded_process;
pub(crate) mod cancel;
pub(crate) mod clasify;
pub(crate) mod gh_clone_repo;
pub(crate) mod gh_get_file_content;
pub(crate) mod gh_get_history_item;
pub(crate) mod gh_search_code;
pub(crate) mod gh_search_history;
pub(crate) mod gh_search_repo;
pub(crate) mod gh_shared;
pub(crate) mod gh_structure;
pub mod id;
pub(crate) mod line_spans;
pub(crate) mod local_fetch;
pub(crate) mod local_search;
pub(crate) mod lsp_search;
pub(crate) mod num;
pub(crate) mod numbered;
pub(crate) mod output;
pub(crate) mod page_memo;
pub(crate) mod result;
pub(crate) mod source;
pub(crate) mod stream_page;
pub(crate) mod structure_search;
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

/// The directory names a tool's own file walk prunes: the syntax-visible
/// defaults (dependency/build output, caches, VCS metadata, credential
/// stores; [`crate::policy::prune::PruneMode`]) plus the tool's `extra`
/// names.
pub(crate) fn syntax_prune(extra: &[&str]) -> std::collections::BTreeSet<String> {
    crate::policy::prune::PruneMode::SyntaxVisible
        .defaults()
        .map(str::to_owned)
        .chain(extra.iter().map(|name| (*name).to_owned()))
        .collect()
}

/// The one `ignore` walk the tools build: it never descends into a directory
/// named in `prune` (the root itself is always walked). The builder keeps the
/// `ignore` defaults (hidden entries skipped, ignore files honored); a caller
/// that differs sets those flags on it before `build`/`build_parallel`.
pub(crate) fn pruned_walk(
    root: &std::path::Path,
    prune: std::collections::BTreeSet<String>,
) -> ignore::WalkBuilder {
    let mut builder = ignore::WalkBuilder::new(root);
    builder.filter_entry(move |entry| {
        entry.depth() == 0
            || !entry.file_type().is_some_and(|kind| kind.is_dir())
            || !prune.contains(entry.file_name().to_string_lossy().as_ref())
    });
    builder
}
