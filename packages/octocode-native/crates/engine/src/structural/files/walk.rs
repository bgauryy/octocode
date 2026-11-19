//! The candidate walk under a structural search root.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use super::Admit;
use crate::error::{Error, Result};
use crate::search::glob::Globs;
use crate::search::text_search::walk_rule::{EntryRule, Skipped};
use crate::search::walk::{GlobalIgnore, Probe, RootedPrune, Stop, walk_builder};
use crate::structural::types::{WalkErrors, WalkSkipped};
use crate::types::IgnorePolicy;

use super::shared::{Step, Walk, WalkRecord};

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

/// What a structural walk takes: its globs, default excludes, ignore
/// policy, depth (the root's entries are depth 1), and how many files it
/// collects.
pub(super) struct Walked<'a> {
    pub(super) globs: &'a Globs,
    /// The default excludes, pruned only where git ignores them.
    pub(super) defaults: Option<RootedPrune>,
    pub(super) ignore: IgnorePolicy,
    pub(super) global_ignore: GlobalIgnore,
    pub(super) max_depth: Option<u32>,
    pub(super) max_files: usize,
}

/// The candidate files under `root`, in path order, the entries the walk
/// could not read on the way, and what its own rules left out. A file root
/// is its own candidate when the globs admit it.
///
/// The walker applies only the globs; the ignore and dot rules are the text
/// search's ([`EntryRule`], the walker's own precedence), applied here with
/// the default prune, so every entry they leave out is recorded
/// ([`WalkSkipped`]) instead of vanishing inside the walker.
pub(super) fn collect_files(
    root: &Path,
    walked: &Walked<'_>,
    (admit, walk): (&Admit<'_>, Walk<'_>),
    stop: &Stop<'_>,
) -> Result<(Vec<PathBuf>, WalkErrors, WalkSkipped)> {
    let metadata = check_root_exists(root)?;
    if let (Walk::Replay(record), true) = (walk, metadata.is_dir()) {
        return replay(record, walked.max_files, admit, stop);
    }
    let mut errors = WalkErrors::default();
    if !metadata.is_dir()
        && let Walk::Record(done) = walk
    {
        // Only a directory's walk is recorded.
        done(None);
    }
    if metadata.is_file() {
        let admitted = !walked.globs.overrides().matched(root, false).is_ignore();
        return Ok((
            if admitted {
                vec![root.to_path_buf()]
            } else {
                Vec::new()
            },
            errors,
            WalkSkipped::default(),
        ));
    }
    if !metadata.is_dir() {
        return Ok((Vec::new(), errors, WalkSkipped::default()));
    }
    let mut builder = walk_builder(
        root,
        IgnorePolicy::ALL,
        walked.max_depth.map(|n| n as usize),
    );
    let rule = EntryRule::of(
        root,
        walked.ignore,
        walked.global_ignore,
        Some(walked.globs.overrides()),
    );
    let skipped = Arc::new(Mutex::new(WalkSkipped::default()));
    // The steps of a walk being recorded, in the order the walker takes them:
    // what its rules leave out is noted as the walker meets it.
    let steps = matches!(walk, Walk::Record(_)).then(|| Arc::new(Mutex::new(Vec::new())));
    {
        let (defaults, skipped, root) =
            (walked.defaults.clone(), skipped.clone(), root.to_path_buf());
        let steps = steps.clone();
        builder.filter_entry(move |entry| {
            if entry.depth() == 0 {
                return true;
            }
            let (path, file_type) = (entry.path(), entry.file_type());
            let is_dir = file_type.is_some_and(|kind| kind.is_dir());
            let probe = Probe {
                path,
                depth: entry.depth(),
                file_type,
            };
            let left_out = match rule
                .as_ref()
                .and_then(|rule| rule.skips(None, probe, is_dir))
            {
                Some(Skipped::Ignored) => Some(Cause::Ignored),
                Some(Skipped::Hidden) => Some(Cause::Hidden),
                None if defaults
                    .as_ref()
                    .is_some_and(|defaults| defaults.prunes(path, is_dir)) =>
                {
                    Some(Cause::Pruned)
                }
                None => None,
            };
            let Some(cause) = left_out else {
                return true;
            };
            let mut shown = crate::text::slash_path(path.strip_prefix(&root).unwrap_or(path));
            if is_dir {
                shown.push('/');
            }
            if let Some(steps) = &steps {
                steps
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(Step::LeftOut(cause, shown.clone()));
            }
            skipped
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .record(cause, shown);
            false
        });
    }
    builder
        .overrides(walked.globs.overrides())
        .sort_by_file_path(Ord::cmp);
    let mut out = Vec::new();
    let record = |step: Step| {
        if let Some(steps) = &steps {
            steps
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(step);
        }
    };
    let mut cut = false;
    for result in builder.build() {
        if out.len() >= walked.max_files {
            cut = true;
            break;
        }
        if stop() {
            return Err(Error::cancelled());
        }
        let entry = match result {
            Ok(entry) => entry,
            Err(error) => {
                record(Step::Error {
                    denied: WalkErrors::denied(&error),
                    message: error.to_string(),
                });
                errors.record(&error);
                continue;
            }
        };
        record(Step::Entry(entry.path().to_path_buf(), entry.file_type()));
        if entry.file_type().is_some_and(|ft| ft.is_dir()) {
            // Let callers retain discovery metadata, including empty directories,
            // without counting them as source-file candidates.
            let _ = admit(entry.path(), entry.file_type());
            continue;
        }
        if entry.file_type().is_some_and(|ft| ft.is_symlink()) {
            // The walk never follows a link; the caller sees it to disclose
            // what it names, never as a candidate.
            let _ = admit(entry.path(), entry.file_type());
            continue;
        }
        if !entry.file_type().is_some_and(|ft| ft.is_file())
            || !admit(entry.path(), entry.file_type())
        {
            continue;
        }
        out.push(entry.into_path());
    }
    if let Walk::Record(done) = walk {
        done(steps.filter(|_| !cut).map(|steps| {
            let steps = std::mem::take(&mut *steps.lock().unwrap_or_else(PoisonError::into_inner));
            let dirs = steps
                .iter()
                .filter_map(|step| match step {
                    Step::Entry(path, Some(kind)) if kind.is_dir() => Some(path.clone()),
                    _ => None,
                })
                .collect();
            WalkRecord { steps, dirs }
        }));
    }
    let skipped = std::mem::take(&mut *skipped.lock().unwrap_or_else(PoisonError::into_inner));
    Ok((out, errors, skipped))
}

/// [`collect_files`] over a recorded walk: the same `admit` calls, errors and
/// left-out entries, in the same order, cut where a walk of `max_files`
/// candidates is. A walk checks its cut and its stop before each entry and
/// error it takes, after the entries its rules left out on the way to it.
fn replay(
    record: &WalkRecord,
    max_files: usize,
    admit: &Admit<'_>,
    stop: &Stop<'_>,
) -> Result<(Vec<PathBuf>, WalkErrors, WalkSkipped)> {
    let mut out = Vec::new();
    let mut errors = WalkErrors::default();
    let mut skipped = WalkSkipped::default();
    for step in &record.steps {
        let (path, file_type) = match step {
            Step::LeftOut(cause, shown) => {
                skipped.record(*cause, shown.clone());
                continue;
            }
            _ if out.len() >= max_files => break,
            _ if stop() => return Err(Error::cancelled()),
            Step::Error { denied, message } => {
                errors.note(*denied, message);
                continue;
            }
            Step::Entry(path, file_type) => (path, *file_type),
        };
        // Directories and links are shown to the caller, never candidates.
        if file_type.is_some_and(|kind| kind.is_dir() || kind.is_symlink()) {
            let _ = admit(path, file_type);
            continue;
        }
        if file_type.is_some_and(|kind| kind.is_file()) && admit(path, file_type) {
            out.push(path.clone());
        }
    }
    Ok((out, errors, skipped))
}

/// Which of the walk's own rules left an entry out.
#[derive(Clone, Copy)]
pub(super) enum Cause {
    Ignored,
    Hidden,
    Pruned,
}

impl WalkSkipped {
    /// Note an entry the walk's own rules left out, as the walk shows it.
    fn record(&mut self, cause: Cause, shown: String) {
        match cause {
            Cause::Ignored => self.ignored.push(shown),
            Cause::Hidden => self.hidden.push(shown),
            Cause::Pruned => self.pruned.push(shown),
        }
    }
}
