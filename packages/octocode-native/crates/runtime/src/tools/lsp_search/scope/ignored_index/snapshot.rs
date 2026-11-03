//! One request's two policy-equivalent walks select ignored family files.
use super::IgnoredIndex;
use crate::policy::path::PathPolicy;
use crate::tools::lsp_search::scope::canonical;
use octocode_engine::search::text_search::TextSearchPathFilter;
use octocode_engine::search::walk::{NestedRepositories, walk_builder};
use octocode_engine::types::IgnorePolicy;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

/// Whether `path` has one of the family `extensions` (any, when empty).
fn family(extensions: &[String], path: &Path) -> bool {
    extensions.is_empty()
        || path.extension().is_some_and(|extension| {
            extensions
                .iter()
                .any(|wanted| extension.to_str() == Some(wanted))
        })
}

fn walk_family(
    root: &Path,
    extensions: &[String],
    policy: &PathPolicy,
    ignores: IgnorePolicy,
    nested: Option<Arc<NestedRepositories>>,
    stopped: &(dyn Fn() -> bool + Sync),
) -> (BTreeSet<String>, bool) {
    let prune = crate::policy::prune::OwnWalk::new(root, []);
    let filter = Arc::new(crate::policy::path::DiscoveryWalk::owned(policy.clone()));
    let dirs_policy = Arc::clone(&filter);
    let mut builder = walk_builder(root, ignores, None);
    builder.filter_entry(move |entry| {
        let path = entry.path();
        let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
        !is_dir
            || (!prune.prunes(path, true)
                && !nested
                    .as_ref()
                    .is_some_and(|nested| nested.skips(path, true))
                && dirs_policy.allows_entry(path, entry.file_type()))
    });
    let mut files = BTreeSet::new();
    let mut complete = true;
    for entry in builder.build() {
        if stopped() {
            return (files, false);
        }
        let Ok(entry) = entry else {
            complete = false;
            continue;
        };
        let path = entry.path();
        if !entry.file_type().is_some_and(|kind| kind.is_dir())
            && family(extensions, path)
            && filter.allows_entry(path, entry.file_type())
        {
            files.insert(canonical(&path.to_string_lossy()));
        }
    }
    (files, complete)
}

pub(super) fn build(
    root: &Path,
    include: &[String],
    policy: &PathPolicy,
    stopped: &(dyn Fn() -> bool + Sync),
) -> IgnoredIndex {
    let extensions = include
        .iter()
        .filter_map(|glob| glob.strip_prefix("*.").map(str::to_owned))
        .collect::<Vec<_>>();
    let nested = Arc::new(NestedRepositories::new(root));
    let (lifted, lifted_complete) = walk_family(
        root,
        &extensions,
        policy,
        IgnorePolicy {
            include_ignored: true,
            include_hidden: false,
        },
        Some(Arc::clone(&nested)),
        stopped,
    );
    let (honored, honored_complete) = walk_family(
        root,
        &extensions,
        policy,
        IgnorePolicy::default(),
        None,
        stopped,
    );
    IgnoredIndex {
        files: lifted.difference(&honored).cloned().collect(),
        nested: nested.skipped(),
        complete: lifted_complete && honored_complete,
    }
}
