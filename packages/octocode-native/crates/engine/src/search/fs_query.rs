//! Name-ordered, resumable file-system queries: [`query_file_system`] walks a
//! root under ignore rules, caller filters and a policy callback.

use std::fs;
use std::path::Path;

mod compiled;
mod filters;
mod read_ahead;
mod walker;

use super::walk::Stop;
use crate::error::Error;
use crate::types::fs_resume::Frame;
use crate::types::{FileSystemQueryOptions, FileSystemQueryResult};
use compiled::CompiledQuery;
pub use filters::NameFilter;
use filters::{EntryKind, file_name, visit_path};
pub(crate) use filters::{has_extension, normalize_extensions};
use walker::{QueryState, walk_children, walk_frame};

/// Whether the walk inspects an entry now or replays one an earlier window
/// of a resumed walk (`skip_entries`) already inspected. A policy callback
/// that counts what it leaves out counts only a [`Visit::Fresh`] entry: the
/// earlier window counted a replayed one. A fresh entry names its walk
/// position, so a window that ends at an earlier entry can drop the counts of
/// the ones after it (the window that resumes there counts them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visit {
    Fresh { position: u32 },
    Replayed,
}

impl Visit {
    /// The walk position of a fresh entry; `None` for a replayed one.
    #[must_use]
    pub fn fresh_at(self) -> Option<u32> {
        match self {
            Self::Fresh { position } => Some(position),
            Self::Replayed => None,
        }
    }
}

/// A walk's policy callback: the path, its directory-entry type (`None`
/// for the root, or when the platform did not report it) and whether the
/// entry is [`Visit::Fresh`]. `false` leaves the entry out, and a directory
/// with everything under it.
pub type EntryFilter<'a> = dyn Fn(&Path, Option<fs::FileType>, Visit) -> bool + 'a;

/// Walk `options.path` in name order, listing the entries its filters
/// match. `admit` decides each descendant before it is inspected or counted
/// (a denied directory is not entered); ignore rules and caller exclusions
/// decide it before that.
/// `stop` cancels the walk ([`ErrorKind::Cancelled`](crate::error::ErrorKind::Cancelled)).
pub fn query_file_system(
    options: FileSystemQueryOptions,
    admit: &EntryFilter<'_>,
    stop: &Stop<'_>,
) -> Result<FileSystemQueryResult, Error> {
    let resume = options.resume.clone();
    let query = CompiledQuery::new(options)?;
    let mut state = QueryState::default();
    // A resumed walk's root was listed (and counted) by the walk it resumes.
    let root_visit = if query.skip_entries == 0 {
        Visit::Fresh { position: 0 }
    } else {
        Visit::Replayed
    };
    if !admit(&query.root, None, root_visit) {
        return Err(Error::new("Filesystem query root is denied by path policy"));
    }
    let root_metadata = fs::symlink_metadata(&query.root).map_err(|err| {
        Error::io(
            &err,
            format!(
                "Cannot access filesystem query root '{}': {err}",
                query.root.display()
            ),
        )
    })?;

    if query.include_root && query.skip_entries == 0 {
        let mut metadata = Some(root_metadata.clone());
        let kind = EntryKind::of(root_metadata.file_type());
        let name = file_name(&query.root);
        // The root's metadata is in hand, so the visit cannot fail to stat it.
        let _ = visit_path(
            &query.root,
            &name,
            0,
            kind,
            &mut metadata,
            &query,
            &mut state,
        );
    }

    if root_metadata.is_dir() && (query.recursive || !query.include_root) {
        // A cursor made by this walk's shape at the resume position continues
        // it without replaying a single entry.
        let frames = resume
            .filter(|cursor| cursor.shape == query.shape && cursor.position == query.skip_entries)
            .map(|cursor| cursor.frames);
        if let Some((first, inner)) = frames.as_deref().and_then(<[Frame]>::split_first) {
            state.position = query.skip_entries;
            walk_frame(first.clone(), inner, &query, &mut state, admit, stop)?;
        } else {
            walk_children(&query.root, 1, &query, &mut state, admit, stop)?;
        }
    } else if !root_metadata.is_dir() && !query.include_root {
        return Err(Error::new(format!(
            "Filesystem query root is not a directory: {}",
            query.root.display()
        )));
    }

    let was_capped = state.total_discovered as usize > query.limit;
    let totals = state.tally.through(u32::MAX);
    Ok(FileSystemQueryResult {
        entries: state.entries,
        total_discovered: state.total_discovered,
        was_capped,
        first_unlisted: state
            .first_unlisted
            .map(|path| path.to_string_lossy().into_owned()),
        skipped: totals.skipped,
        permission_denied: totals.permission_denied,
        ignored: totals.ignored,
        pruned_dirs: totals.pruned_dirs,
        time_cut: state.time_cut,
        walk_position: state.position,
        tally: state.tally,
        cursor: state.cursor,
        window_cursor: state.window_cursor,
    })
}

#[cfg(test)]
mod tests;
