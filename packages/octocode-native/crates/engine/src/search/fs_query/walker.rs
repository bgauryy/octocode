//! The walk itself: directories read in name order (some read ahead), each
//! entry decided, listed, counted, and the walk's position kept for a resume.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use super::EntryFilter;
use super::compiled::CompiledQuery;
use super::read_ahead::{self, ReadAhead};
use crate::error::Error;
use crate::search::walk::{ListedRules, Stop};
use crate::types::fs_resume::{Child, Frame, WalkCursor, WalkTally};
use crate::types::{FileSystemEntry, WalkedDir};

mod level;

use level::walk_level;

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
    /// Directories the walk entered, as their listings showed them.
    pub(super) walked_dirs: Vec<WalkedDir>,
    /// Directories the walk read, itself or ahead.
    pub(super) directories_read: u32,
    /// The workers reading the walk's next directories, once it read
    /// [`read_ahead::START_AFTER_DIRECTORIES`] itself.
    pub(super) ahead: Option<ReadAhead>,
    /// The directory the walk is about to enter, read early for what its
    /// listing shows ([`entered_empty`]).
    read_first: Option<(PathBuf, Children)>,
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
    Read {
        children: Vec<Child>,
        read_at: Option<std::time::SystemTime>,
        stamp: Option<crate::stamp::SourceStamp>,
        /// Every entry of the listing could be read.
        complete: bool,
    },
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
    if let Some((dir, children)) = state.read_first.take()
        && dir == base
    {
        return children;
    }
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
    Children::Read {
        complete: errors.is_empty(),
        children,
        read_at: listing.read_at,
        stamp: listing.stamp,
    }
}

/// Whether the directory `base`, an entry at `depth`, holds no entries, from
/// the listing the walk is about to enter it by, which it keeps for that.
/// `None` when no listing tells: the walk will not enter it, or the walk's
/// time ran out during the read. One it cannot read counts as not empty.
pub(super) fn entered_empty(
    base: &Path,
    depth: u32,
    query: &CompiledQuery,
    state: &mut QueryState,
) -> Option<bool> {
    let entered = depth + 1;
    if !query.recursive
        || entered > MAX_RECURSION_DEPTH
        || query.max_depth.is_some_and(|max_depth| entered > max_depth)
    {
        return None;
    }
    let children = read_children(base, entered, query, state);
    let empty = match &children {
        Children::Read {
            children, complete, ..
        } => Some(*complete && children.is_empty()),
        Children::None => Some(false),
        Children::Late => None,
    };
    state.read_first = Some((base.to_path_buf(), children));
    empty
}

/// Whether a listing sorted by name holds `name`.
fn listed<'c>(children: &'c [Child], name: &str) -> Option<&'c Child> {
    children
        .binary_search_by(|child| child.name.as_os_str().cmp(std::ffi::OsStr::new(name)))
        .ok()
        .map(|at| &children[at])
}

/// What a directory's name-sorted listing shows of its ignore rules.
fn listed_rules(children: &[Child]) -> ListedRules {
    ListedRules {
        gitignore: listed(children, ".gitignore").is_some(),
        ignore: listed(children, ".ignore").is_some(),
        git: listed(children, ".git").is_some(),
        jj: listed(children, ".jj").is_some(),
    }
}

/// The directory `base` as its listing shows it, for the caller of the walk.
fn walked_dir(
    base: &Path,
    stamp: Option<crate::stamp::SourceStamp>,
    children: &[Child],
) -> WalkedDir {
    WalkedDir {
        path: base.to_path_buf(),
        stamp,
        ignore_files: crate::search::walk::IGNORE_FILES
            .iter()
            .filter_map(|name| {
                let regular = listed(children, name)?
                    .file_type
                    .is_some_and(|kind| kind.is_file());
                Some((base.join(name), regular))
            })
            .collect(),
        holds_git: listed(children, ".git").is_some(),
    }
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
    let (children, complete) = match frame.children.clone() {
        Some(children) => {
            state
                .walked_dirs
                .push(walked_dir(&frame.base, None, &children));
            (children, true)
        }
        None => match read_children(&frame.base, frame.depth, query, state) {
            Children::Read {
                children,
                read_at,
                stamp,
                complete,
            } => {
                frame.read_at = read_at;
                state
                    .walked_dirs
                    .push(walked_dir(&frame.base, stamp, &children));
                (Arc::new(children), complete)
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
    // A listing that read every entry shows whether the directory holds a
    // rule file; one that did not leaves the rules to their own stats.
    let rules = query
        .ignore_rules
        .as_ref()
        .map(|rules| rules.enter(&frame.base, complete.then(|| listed_rules(&children))));
    frame.children = Some(children);
    state.stack.push(frame);
    let level = state.stack.len() - 1;
    let walked = walk_level(level, inner, query, state, admit, stop, rules.as_ref());
    state.stack.pop();
    walked
}
