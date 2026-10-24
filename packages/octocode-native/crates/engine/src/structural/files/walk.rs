//! The candidate walk under a structural search root.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::search::glob::Globs;
use crate::search::walk::{IgnorePolicy, Prune, RootedPrune, Stop, walk_builder};
use crate::structural::types::WalkErrors;

/// Loud existence check shared by every entry point.
pub(super) fn check_root_exists(root: &Path) -> Result<std::fs::Metadata> {
    fs::metadata(root).map_err(|err| {
        Error::io(
            &err,
            format!(
                "Cannot access structural search path '{}': {err}",
                root.display()
            ),
        )
    })
}

/// What a structural walk takes: its globs, pruned directory names, ignore
/// policy, depth (the root's entries are depth 1), and how many files it
/// collects.
pub(super) struct Walked<'a> {
    pub(super) globs: &'a Globs,
    pub(super) prune: Prune,
    /// The default excludes, pruned only where git ignores them.
    pub(super) defaults: Option<RootedPrune>,
    pub(super) ignore: IgnorePolicy,
    pub(super) max_depth: Option<u32>,
    pub(super) max_files: usize,
}

/// The candidate files under `root`, in path order, and the entries the
/// walk could not read on the way. A file root is its own candidate when
/// the globs admit it.
pub(super) fn collect_files(
    root: &Path,
    walked: &Walked<'_>,
    admit: &(dyn Fn(&Path) -> bool + Sync),
    stop: &Stop<'_>,
) -> Result<(Vec<PathBuf>, WalkErrors)> {
    let metadata = check_root_exists(root)?;
    let mut errors = WalkErrors::default();
    if metadata.is_file() {
        let admitted = !walked.globs.overrides().matched(root, false).is_ignore();
        return Ok((
            if admitted {
                vec![root.to_path_buf()]
            } else {
                Vec::new()
            },
            errors,
        ));
    }
    if !metadata.is_dir() {
        return Ok((Vec::new(), errors));
    }
    let mut builder = walk_builder(root, walked.ignore, walked.max_depth.map(|n| n as usize));
    let (prune, defaults) = (walked.prune.clone(), walked.defaults.clone());
    builder.filter_entry(move |entry| {
        !prune.prunes_entry(entry)
            && !(entry.depth() > 0
                && defaults.as_ref().is_some_and(|defaults| {
                    defaults.prunes(
                        entry.path(),
                        entry.file_type().is_some_and(|kind| kind.is_dir()),
                    )
                }))
    });
    builder
        .overrides(walked.globs.overrides())
        .sort_by_file_path(Ord::cmp);
    let mut out = Vec::new();
    for result in builder.build() {
        if out.len() >= walked.max_files {
            break;
        }
        if stop() {
            return Err(Error::cancelled());
        }
        let entry = match result {
            Ok(entry) => entry,
            Err(error) => {
                errors.record(&error);
                continue;
            }
        };
        if entry.file_type().is_some_and(|ft| ft.is_dir()) {
            // Let callers retain discovery metadata, including empty directories,
            // without counting them as source-file candidates.
            let _ = admit(entry.path());
            continue;
        }
        if entry.file_type().is_some_and(|ft| ft.is_symlink()) {
            // The walk never follows a link; the caller sees it to disclose
            // what it names, never as a candidate.
            let _ = admit(entry.path());
            continue;
        }
        if !entry.file_type().is_some_and(|ft| ft.is_file()) || !admit(entry.path()) {
            continue;
        }
        out.push(entry.into_path());
    }
    Ok((out, errors))
}
