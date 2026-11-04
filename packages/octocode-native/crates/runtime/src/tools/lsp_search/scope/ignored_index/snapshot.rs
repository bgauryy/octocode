//! One request's two policy-equivalent walks select ignored family files.
use super::IgnoredIndex;
use crate::policy::path::PathPolicy;
use crate::tools::lsp_search::scope::canonical;
use crate::tools::page_memo::directory_ignore_inputs;
use octocode_engine::search::text_search::TextSearchPathFilter;
use octocode_engine::search::walk::{NestedRepositories, walk_builder};
use octocode_engine::types::IgnorePolicy;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// Whether `path` has one of the family `extensions` (any, when empty).
fn family(extensions: &[String], path: &Path) -> bool {
    extensions.is_empty()
        || path.extension().is_some_and(|extension| {
            extensions
                .iter()
                .any(|wanted| extension.to_str() == Some(wanted))
        })
}

/// What decides the walks' result: every directory either entered (a new
/// entry changes its stamp), the nested repositories they stayed out of
/// (losing its `.git` changes one's stamp), and the directories whose ignore
/// files the rules read.
#[derive(Debug, Default)]
pub(in crate::tools::lsp_search::scope) struct Watched {
    entered: Vec<PathBuf>,
    nested: Vec<PathBuf>,
    ruled: Vec<PathBuf>,
}

impl Watched {
    /// The paths to stamp and the rule files that may not exist; `None` when an
    /// ignore file is one no stamp can watch.
    pub(super) fn inputs(
        self,
        root: &Path,
        policy: &PathPolicy,
    ) -> Option<(Vec<PathBuf>, Vec<PathBuf>)> {
        let mut sources = self.entered;
        sources.extend(self.nested);
        let mut rules = Vec::new();
        for dir in &self.ruled {
            let inputs =
                directory_ignore_inputs(dir, root, |ignore| policy.validate_read(ignore).is_ok());
            if inputs.untracked {
                return None;
            }
            sources.extend(inputs.files);
            rules.extend(inputs.external);
        }
        Some((sources, rules))
    }
}

/// One walk's family files and entered directories, and whether it finished.
struct Walk {
    files: BTreeSet<String>,
    dirs: Vec<PathBuf>,
    complete: bool,
}

fn walk_family(
    root: &Path,
    extensions: &[String],
    policy: &PathPolicy,
    ignores: IgnorePolicy,
    nested: Option<Arc<NestedRepositories>>,
    stopped: &(dyn Fn() -> bool + Sync),
) -> Walk {
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
    let seen = Mutex::new((BTreeSet::new(), Vec::new()));
    let complete = AtomicBool::new(true);
    builder.build_parallel().run(|| {
        Box::new(|entry| {
            if stopped() {
                complete.store(false, Ordering::Relaxed);
                return ignore::WalkState::Quit;
            }
            let Ok(entry) = entry else {
                complete.store(false, Ordering::Relaxed);
                return ignore::WalkState::Continue;
            };
            let path = entry.path();
            if entry.file_type().is_some_and(|kind| kind.is_dir()) {
                let mut seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
                seen.1.push(path.to_path_buf());
            } else if family(extensions, path) && filter.allows_entry(path, entry.file_type()) {
                // The walk follows no symlink: under a canonical root only a
                // link itself still needs resolving.
                let file = if entry.path_is_symlink() {
                    canonical(&path.to_string_lossy())
                } else {
                    path.to_string_lossy().into_owned()
                };
                seen.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .0
                    .insert(file);
            }
            ignore::WalkState::Continue
        })
    });
    let (files, dirs) = seen.into_inner().unwrap_or_else(PoisonError::into_inner);
    Walk {
        files,
        dirs,
        complete: complete.into_inner(),
    }
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
    let lifted_nested = Arc::clone(&nested);
    let (lifted, honored) = std::thread::scope(|scope| {
        let extensions = &extensions;
        let honored = scope.spawn(move || {
            walk_family(
                root,
                extensions,
                policy,
                IgnorePolicy::default(),
                None,
                stopped,
            )
        });
        let lifted = walk_family(
            root,
            extensions,
            policy,
            IgnorePolicy {
                include_ignored: true,
                include_hidden: false,
            },
            Some(lifted_nested),
            stopped,
        );
        // A walk thread that panicked listed nothing it can prove.
        let honored = honored.join().unwrap_or(Walk {
            files: BTreeSet::new(),
            dirs: Vec::new(),
            complete: false,
        });
        (lifted, honored)
    });
    let skipped = nested.skipped();
    IgnoredIndex {
        files: lifted.files.difference(&honored.files).cloned().collect(),
        watched: Watched {
            entered: lifted.dirs,
            nested: skipped.iter().map(|repo| root.join(repo)).collect(),
            ruled: honored.dirs,
        },
        nested: skipped,
        complete: lifted.complete && honored.complete,
    }
}
