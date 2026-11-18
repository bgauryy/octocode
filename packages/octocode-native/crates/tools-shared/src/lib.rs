//! The code every tool family shares: walks, selection and judgment, source
//! reading and capture, GitHub query helpers and the leads rows carry. No
//! module here names a tool.

// Numeric conversions are explicit (`try_from` with a stated outcome): a
// lossy `as` cast fails CI (`-D warnings`). Tests may cast fixture values.
#![cfg_attr(
    not(test),
    warn(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap
    )
)]
// Library output reaches people through its callers or the `octocode_io::notice`
// owner, never stray stdout/stderr prints; deliberate sites carry `#[expect]`.
#![deny(clippy::print_stdout, clippy::print_stderr)]

pub mod ast_rule;
pub mod bounded_process;
pub mod declarations;
pub mod editor_lock;
pub mod empty_walk;
pub mod gh_git;
pub mod gh_shared;
pub mod import_resolution;
pub mod judged_rows;
pub mod judgment;
pub mod large_source;
pub mod leads;
pub mod line_spans;
pub mod match_keys;
pub mod match_list;
pub mod page_memo;
#[cfg(test)]
mod process_fixture;
pub mod root_read;
pub mod same_line;
pub mod selection;
pub mod source;
pub mod source_capture;
pub mod source_sections;
pub mod source_view;
pub mod test_code;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod variant_field;
pub mod workers;

/// The final component of `path` (lossy UTF-8), or "" when it has none.
pub fn display_name(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// The engine walk a tool's own file walk builds: the default ignore policy
/// (ignore rules honored, dot entries skipped), never descending into a
/// directory the one default prune leaves out, nor into the credential
/// stores or the tool's `extra` names ([`octocode_runtime_base::policy::prune::OwnWalk`];
/// the root itself is always walked).
pub fn pruned_walk(
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
pub fn pruned_walk_under(
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
/// [`PathPolicy`]: octocode_runtime_base::policy::path::PathPolicy
pub fn pruned_walk_in(
    root: &std::path::Path,
    defaults: bool,
    extra: impl IntoIterator<Item = String>,
    (policy, global): (
        octocode_engine::types::IgnorePolicy,
        octocode_engine::search::walk::GlobalIgnore,
    ),
    keep: impl Fn(&ignore::DirEntry) -> bool + Send + Sync + 'static,
) -> ignore::WalkBuilder {
    let prune =
        octocode_runtime_base::policy::prune::OwnWalk::with_defaults(root, defaults, extra, global);
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
pub fn package_walk(package: &std::path::Path) -> ignore::WalkBuilder {
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
