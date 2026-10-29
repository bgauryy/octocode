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
pub(crate) mod source_view;
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

/// The engine walk a tool's own file walk builds: the default ignore policy
/// (ignore rules honored, dot entries skipped), never descending into a
/// directory the one default prune leaves out, nor into the credential
/// stores or the tool's `extra` names ([`crate::policy::prune::OwnWalk`];
/// the root itself is always walked).
pub(crate) fn pruned_walk(
    root: &std::path::Path,
    extra: impl IntoIterator<Item = String>,
) -> ignore::WalkBuilder {
    pruned_walk_with(
        root,
        extra,
        octocode_engine::search::walk::IgnorePolicy::default(),
    )
}

/// [`pruned_walk`] under an explicit ignore `policy`; the default prunes
/// still apply, so `node_modules`, build output and credential stores stay
/// out whatever the policy admits.
pub(crate) fn pruned_walk_with(
    root: &std::path::Path,
    extra: impl IntoIterator<Item = String>,
    policy: octocode_engine::search::walk::IgnorePolicy,
) -> ignore::WalkBuilder {
    let prune = crate::policy::prune::OwnWalk::new(root, extra);
    let mut walk = octocode_engine::search::walk::walk_builder(root, policy, None);
    walk.filter_entry(move |entry| {
        !prune.prunes(
            entry.path(),
            entry.file_type().is_some_and(|kind| kind.is_dir()),
        )
    });
    walk
}
