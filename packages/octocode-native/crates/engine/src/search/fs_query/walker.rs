//! The walk itself: directories read in name order (some read ahead), each
//! entry decided, listed, counted, and the walk's position kept for a resume.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use super::compiled::{CompiledQuery, Verdict};
use super::filters::{EntryKind, lstat, visit_path};
use super::read_ahead::{self, ReadAhead};
use super::{EntryFilter, Visit};
use crate::error::Error;
use crate::search::walk::Stop;
use crate::types::FileSystemEntry;
use crate::types::fs_resume::{Child, Frame, WalkCursor, WalkTally};

/// Hard ceiling on directory-recursion depth. Symlink cycles are already
/// avoided (symlink_metadata never reports a symlink as a dir), so this only
/// guards against pathologically deep real trees overflowing the stack when no
/// `max_depth` was supplied. Far deeper than any realistic project layout.
pub(super) const MAX_RECURSION_DEPTH: u32 = 100;

#[derive(Default)]
pub(super) struct QueryState {
    /// When the walk stops at its next entry; set when the first new entry
    /// is reached, so replayed entries spend none of the budget.
    pub(super) deadline: Option<Instant>,
    /// Entries inspected so far, replayed ones included.
    pub(super) position: u32,
    pub(super) entries: Vec<FileSystemEntry>,
    /// Entries listed (stored, the root excepted) so far.
    pub(super) listed: u32,
    pub(super) total_discovered: u32,
    /// The first match the entry limit left unlisted.
    pub(super) first_unlisted: Option<PathBuf>,
    pub(super) tally: WalkTally,
    pub(super) time_cut: Option<String>,
    /// The directories the walk is inside, outermost first.
    pub(super) stack: Vec<Frame>,
    /// Where the walk stood when it was cut by time.
    pub(super) cursor: Option<WalkCursor>,
    /// Where the walk stood at the `window_rows`-th listed entry.
    pub(super) window_cursor: Option<WalkCursor>,
    /// Directories the walk read, itself or ahead.
    pub(super) directories_read: u32,
    /// The workers reading the walk's next directories, once it read
    /// [`read_ahead::START_AFTER_DIRECTORIES`] itself.
    pub(super) ahead: Option<ReadAhead>,
}

impl QueryState {
    /// Count an entry or directory that could not be read.
    pub(super) fn unreadable(&mut self, err: &std::io::Error) {
        let denied = err.kind() == std::io::ErrorKind::PermissionDenied;
        self.tally.unreadable(self.position, denied);
    }

    /// Where the walk stands: the directories it is inside and, when the
    /// walk is about to enter one, that directory.
    fn cursor(&self, shape: u64, entering: Option<Frame>) -> WalkCursor {
        let mut frames = self.stack.clone();
        frames.extend(entering);
        WalkCursor {
            position: self.position,
            shape,
            frames,
        }
    }

    /// The walk has nothing more to inspect: out of time or past the limit.
    fn finished(&self, query: &CompiledQuery) -> bool {
        self.time_cut.is_some()
            || (query.stop_at_limit && self.total_discovered as usize > query.limit)
    }
}

/// Walk the directory `base` (a fresh one: read now) and everything under it.
pub(super) fn walk_children(
    base: &Path,
    depth: u32,
    query: &CompiledQuery,
    state: &mut QueryState,
    admit: &EntryFilter<'_>,
    stop: &Stop<'_>,
) -> Result<(), Error> {
    let frame = Frame {
        base: base.to_path_buf(),
        depth,
        children: None,
        read_at: None,
        next: 0,
    };
    walk_frame(frame, &[], query, state, admit, stop)
}

/// What reading a directory the walk enters gave.
enum Children {
    Read(Vec<Child>, Option<std::time::SystemTime>),
    /// Too deep, past `max_depth`, or unreadable (counted, unless the walk
    /// this one resumes counted it): nothing to walk.
    None,
    /// The walk's time budget ran out while the listing was read: the walk
    /// stops before the directory's first entry, and the window that
    /// resumes it lists the directory whole.
    Late,
}

/// A directory's entries in walk order ([`Children`]).
fn read_children(
    base: &Path,
    depth: u32,
    query: &CompiledQuery,
    state: &mut QueryState,
) -> Children {
    // The walk this one resumes read this directory when it inspected the
    // entry that led here (the root: before its first entry), and counted
    // what failed to read: counted again, a failure would show twice.
    let reread = query.skip_entries > 0 && state.position <= query.skip_entries;
    if depth > MAX_RECURSION_DEPTH {
        if !reread {
            state.tally.unreadable(state.position, false);
        }
        return Children::None;
    }
    if query.max_depth.is_some_and(|max_depth| depth > max_depth) {
        return Children::None;
    }

    // The deadline is set once the window inspected a new entry, so a
    // directory the window starts in (the root, a resumed one) is always
    // listed and every window makes progress.
    let deadline = state.deadline;
    let taken = match state.ahead.as_mut() {
        Some(ahead) => ahead.take(base, deadline),
        None => read_ahead::Taken::NotQueued,
    };
    let listing = match taken {
        read_ahead::Taken::Read(listing) => listing,
        read_ahead::Taken::Late => return Children::Late,
        read_ahead::Taken::NotQueued => match read_ahead::read_listing_until(base, deadline) {
            Some(listing) => listing,
            None => return Children::Late,
        },
    };
    state.directories_read += 1;
    if state.ahead.is_none() && state.directories_read == read_ahead::START_AFTER_DIRECTORIES {
        state.ahead = ReadAhead::start();
    }
    // Read errors are surfaced as skips.
    let (children, errors) = match listing.entries {
        Ok(read) => read,
        Err(err) => {
            if !reread {
                state.unreadable(&err);
            }
            return Children::None;
        }
    };
    if !reread {
        for err in &errors {
            state.unreadable(err);
        }
    }
    Children::Read(children, listing.read_at)
}

/// Walk one directory of the walk: `frame` from its `next` entry on, the
/// directories `inner` (a cursor's deeper ones, outermost first) before it.
pub(super) fn walk_frame(
    mut frame: Frame,
    inner: &[Frame],
    query: &CompiledQuery,
    state: &mut QueryState,
    admit: &EntryFilter<'_>,
    stop: &Stop<'_>,
) -> Result<(), Error> {
    let children = match frame.children.clone() {
        Some(children) => children,
        None => match read_children(&frame.base, frame.depth, query, state) {
            Children::Read(children, read_at) => {
                frame.read_at = read_at;
                Arc::new(children)
            }
            Children::None => return Ok(()),
            Children::Late => {
                let relative = frame.base.strip_prefix(&query.root).unwrap_or(&frame.base);
                let mut cut = crate::text::slash_path(relative);
                cut.push('/');
                state.time_cut = Some(cut);
                state.cursor = Some(state.cursor(query.shape, Some(frame)));
                return Ok(());
            }
        },
    };
    frame.children = Some(children);
    state.stack.push(frame);
    let level = state.stack.len() - 1;
    let walked = walk_level(level, inner, query, state, admit, stop);
    state.stack.pop();
    walked
}

fn walk_level(
    level: usize,
    inner: &[Frame],
    query: &CompiledQuery,
    state: &mut QueryState,
    admit: &EntryFilter<'_>,
    stop: &Stop<'_>,
) -> Result<(), Error> {
    if let Some((first, rest)) = inner.split_first() {
        walk_frame(first.clone(), rest, query, state, admit, stop)?;
        if state.finished(query) {
            return Ok(());
        }
    }
    let frame = &state.stack[level];
    let (base, depth, start) = (frame.base.clone(), frame.depth, frame.next);
    let children = frame.children.clone().unwrap_or_default();
    // Child directories this one reads at the next depth: the ones read
    // ahead while the walk is inside an earlier sibling.
    let reads_next = query.recursive
        && depth < MAX_RECURSION_DEPTH
        && query.max_depth.is_none_or(|max_depth| depth < max_depth);
    // The directory child last inspected: when the walk did not enter it,
    // its listing read ahead is dropped.
    let mut passed: Option<PathBuf> = None;

    for (index, child) in children.iter().enumerate().skip(start) {
        if let Some(dir) = passed.take()
            && let Some(ahead) = state.ahead.as_mut()
        {
            ahead.drop_unused(&dir);
        }
        // The next entry of this directory a resume inspects.
        state.stack[level].next = index;
        // Look ahead to one additional matching entry before declaring overflow.
        // Reaching the stored-entry cap alone does not prove the scan is partial.
        if query.stop_at_limit && state.total_discovered as usize > query.limit {
            return Ok(());
        }
        if stop() {
            return Err(Error::cancelled());
        }
        let file_name = child.name.as_os_str();
        // The leading `skip_entries` entries were inspected by the walk this
        // one resumes: replayed below, they spend none of the time budget.
        let replaying = state.position < query.skip_entries;
        if !replaying && let Some(budget) = query.time_budget {
            // The budget starts at the first new entry, which a positive
            // budget always inspects: every resumed window makes progress.
            let first = state.deadline.is_none();
            let deadline = *state
                .deadline
                .get_or_insert_with(|| crate::scan_budget::walk_now() + budget);
            if (budget.is_zero() || !first) && crate::scan_budget::walk_now() >= deadline {
                let path = base.join(file_name);
                let relative = path.strip_prefix(&query.root).unwrap_or(&path);
                state.time_cut = Some(crate::text::slash_path(relative));
                state.cursor = Some(state.cursor(query.shape, None));
                return Ok(());
            }
        }
        state.position += 1;
        state.stack[level].next = index + 1;

        let path = base.join(file_name);
        let file_type = child.file_type;
        let mut metadata = None;
        let kind = match file_type {
            Some(file_type) => EntryKind::of(file_type),
            // A replayed entry was counted by the walk it resumes.
            None if replaying => match fs::symlink_metadata(&path) {
                Ok(meta) => meta.file_type().into(),
                Err(_) => continue,
            },
            None => match lstat(&path, state) {
                Some(meta) => metadata.insert(meta).file_type().into(),
                None => continue,
            },
        };
        let is_directory = kind.is_dir;
        if is_directory {
            passed = Some(path.clone());
        }
        // A replayed entry only counts: the walk it resumes listed it or
        // not, and entered it or not by this same verdict, which only a
        // directory's entry needs again.
        if replaying && !(query.recursive && is_directory) {
            continue;
        }
        let visit = if replaying {
            Visit::Replayed
        } else {
            Visit::Fresh {
                position: state.position,
            }
        };
        match query.verdict(&path, file_name, is_directory, (file_type, visit), admit) {
            Verdict::Ignored { counted } => {
                if counted && !replaying {
                    let relative = path.strip_prefix(&query.root).unwrap_or(&path);
                    let mut relative = crate::text::slash_path(relative);
                    if is_directory {
                        relative.push('/');
                    }
                    state.tally.ignored(state.position, relative);
                }
                continue;
            }
            Verdict::Pruned => {
                if !replaying {
                    let relative = path.strip_prefix(&query.root).unwrap_or(&path);
                    state
                        .tally
                        .pruned(state.position, crate::text::slash_path(relative));
                }
                continue;
            }
            Verdict::Left => continue,
            Verdict::Enters => {}
        }

        if !replaying {
            let listed = state.entries.len();
            if visit_path(
                &path,
                &file_name.to_string_lossy(),
                depth,
                kind,
                &mut metadata,
                query,
                state,
            )
            .is_err()
            {
                continue;
            }
            if state.entries.len() > listed && state.position > 0 {
                state.listed += 1;
                if query.window_rows == Some(state.listed) {
                    let entering = (query.recursive && is_directory).then(|| Frame {
                        base: path.clone(),
                        depth: depth + 1,
                        children: None,
                        read_at: None,
                        next: 0,
                    });
                    state.window_cursor = Some(state.cursor(query.shape, entering));
                }
            }
        }

        if query.recursive && is_directory {
            if reads_next && let Some(ahead) = state.ahead.as_mut() {
                ahead.queue(
                    children[index + 1..]
                        .iter()
                        .filter(|child| child.file_type.is_some_and(|kind| kind.is_dir()))
                        .map(|child| base.join(&child.name)),
                );
            }
            walk_children(&path, depth + 1, query, state, admit, stop)?;
            if state.finished(query) {
                return Ok(());
            }
        }
    }
    if let Some(dir) = passed
        && let Some(ahead) = state.ahead.as_mut()
    {
        ahead.drop_unused(&dir);
    }
    Ok(())
}
