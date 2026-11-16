pub(crate) mod artifact_search;
pub(crate) mod ast_graph;
pub(crate) mod ast_rule;
pub(crate) mod ast_search;
pub(crate) mod bounded_process;
pub(crate) mod clasify;
#[cfg(test)]
mod contract_bounds_tests;
pub(crate) mod declarations;
pub(crate) mod editor_lock;
pub(crate) mod empty_walk;
pub(crate) mod gh_clone_repo;
pub(crate) mod gh_get_file_content;
pub(crate) mod gh_get_history_item;
pub(crate) mod gh_git;
pub(crate) mod gh_search_code;
pub(crate) mod gh_search_history;
pub(crate) mod gh_search_repo;
pub(crate) mod gh_shared;
pub(crate) mod gh_structure;
#[cfg(test)]
mod hints_tests;
pub(crate) mod import_resolution;
pub(crate) mod judged_rows;
pub(crate) mod judgment;
pub(crate) mod large_source;
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
pub(crate) mod security_check;
pub(crate) mod selection;
pub(crate) mod source;
pub(crate) mod source_capture;
pub(crate) mod source_sections;
pub(crate) mod source_view;
pub(crate) mod structure_search;
pub(crate) mod test_code;
#[cfg(test)]
pub(crate) mod test_support;
pub(crate) mod variant_field;
pub(crate) mod workers;

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
    global: octocode_engine::search::walk::GlobalIgnore,
) -> ignore::WalkBuilder {
    pruned_walk_under(
        root,
        true,
        extra,
        (octocode_engine::types::IgnorePolicy::default(), global),
    )
}

/// [`pruned_walk`] with the default prune lifted when `defaults` is false
/// (the credential stores stay out) and under an explicit ignore policy; the
/// default prunes still apply, so `node_modules`, build output and credential
/// stores stay out whatever the policy admits.
pub(crate) fn pruned_walk_under(
    root: &std::path::Path,
    defaults: bool,
    extra: impl IntoIterator<Item = String>,
    rules: (
        octocode_engine::types::IgnorePolicy,
        octocode_engine::search::walk::GlobalIgnore,
    ),
) -> ignore::WalkBuilder {
    pruned_walk_in(root, defaults, extra, rules, |_| true)
}

/// [`pruned_walk_under`] that also enters only the entries `keep` admits (a
/// walk has one entry filter, so the prune, the call's global ignore rules
/// (carried by its [`PathPolicy`], asked per repository) and a tool's own
/// rule compose here): a tool's rule such as "nested packages are their own"
/// never walks around the prune.
///
/// [`PathPolicy`]: crate::policy::path::PathPolicy
pub(crate) fn pruned_walk_in(
    root: &std::path::Path,
    defaults: bool,
    extra: impl IntoIterator<Item = String>,
    (policy, global): (
        octocode_engine::types::IgnorePolicy,
        octocode_engine::search::walk::GlobalIgnore,
    ),
    keep: impl Fn(&ignore::DirEntry) -> bool + Send + Sync + 'static,
) -> ignore::WalkBuilder {
    let prune = crate::policy::prune::OwnWalk::with_defaults(root, defaults, extra, global);
    let (mut walk, global) =
        octocode_engine::search::walk::walk_builder_under(root, policy, global, None);
    walk.filter_entry(move |entry| {
        !prune.prunes(
            entry.path(),
            entry.file_type().is_some_and(|kind| kind.is_dir()),
        ) && !global.hides_entry(entry)
            && keep(entry)
    });
    walk
}

/// The walk of one Cargo package's own files: [`pruned_walk`] under no
/// global ignore rules (what a package compiles is no user's choice) that
/// does not enter a nested package (a directory under `package` holding its own
/// `Cargo.toml` is a package of its own).
pub(crate) fn package_walk(package: &std::path::Path) -> ignore::WalkBuilder {
    let root = package.to_path_buf();
    pruned_walk_in(
        package,
        true,
        [],
        (
            octocode_engine::types::IgnorePolicy::default(),
            octocode_engine::search::walk::GlobalIgnore::NONE,
        ),
        move |entry| {
            entry.path() == root
                || !entry.file_type().is_some_and(|kind| kind.is_dir())
                || !entry.path().join("Cargo.toml").is_file()
        },
    )
}
