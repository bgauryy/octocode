//! What a filesystem walk keeps so the window that continues it neither
//! replays nor recounts anything: the [`WalkCursor`] (where the walk stands:
//! every directory it is inside, with the entries still to inspect) and the
//! [`WalkTally`] (what the walk counted, stamped with the walk position that
//! counted it, so a window that ends at an earlier entry keeps only the
//! counts of the entries it covers).

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

/// One entry of a directory as its listing read it.
#[derive(Debug, Clone)]
pub(crate) struct Child {
    pub(crate) name: OsString,
    pub(crate) file_type: Option<fs::FileType>,
}

/// A directory the walk is inside: its sorted entries and the index of the
/// next one to inspect. `children` is `None` for a directory the walk is
/// about to enter and has not read yet.
#[derive(Debug, Clone)]
pub(crate) struct Frame {
    pub(crate) base: PathBuf,
    pub(crate) depth: u32,
    pub(crate) children: Option<Arc<Vec<Child>>>,
    /// The directory's modification time just before its listing was read:
    /// a cursor holder compares it with the directory's now, since an entry
    /// created, removed or renamed in it moves the time on.
    pub(crate) read_at: Option<std::time::SystemTime>,
    pub(crate) next: usize,
}

/// Where a walk stood after `position` entries: the directories it was
/// inside, outermost first. A walk resumed from it starts at the entry after
/// `position` without reading anything it had already read. It is valid only
/// for the walk shape (root, depth, ignore and exclusion rules) that made it.
#[derive(Debug, Clone)]
pub struct WalkCursor {
    pub(crate) position: u32,
    pub(crate) shape: u64,
    pub(crate) frames: Vec<Frame>,
}

impl WalkCursor {
    /// The `skip_entries` this cursor resumes at.
    #[must_use]
    pub fn position(&self) -> u32 {
        self.position
    }

    /// The directories whose listing the cursor holds, with the modification
    /// time each had when it was read: a cursor serves only while these
    /// still hold.
    #[must_use]
    pub fn listed_directories(&self) -> Vec<(&std::path::Path, Option<std::time::SystemTime>)> {
        self.frames
            .iter()
            .filter(|frame| frame.children.is_some())
            .map(|frame| (frame.base.as_path(), frame.read_at))
            .collect()
    }

    /// Entries the cursor still holds to inspect, what keeping it costs.
    #[must_use]
    pub fn weight(&self) -> usize {
        self.frames
            .iter()
            .map(|frame| {
                frame
                    .children
                    .as_ref()
                    .map_or(0, |children| children.len().saturating_sub(frame.next))
            })
            .sum::<usize>()
            + self.frames.len()
    }
}

/// One thing a walk counted.
#[derive(Debug, Clone)]
enum Counted {
    /// An entry or directory that could not be read; `denied` for want of
    /// permission.
    Unreadable { denied: bool },
    /// An entry the ignore rules left out: its root-relative path, a
    /// directory's with a trailing `/`.
    Ignored(String),
    /// A directory the prune skipped, root-relative.
    Pruned(String),
}

/// What a walk counted, each count stamped with the walk position of the
/// entry that caused it.
#[derive(Debug, Clone, Default)]
pub struct WalkTally {
    events: Vec<(u32, Counted)>,
}

/// The counts of a [`WalkTally`] through some position.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Totals {
    pub(crate) skipped: u32,
    pub(crate) permission_denied: u32,
    pub(crate) ignored: Vec<String>,
    pub(crate) pruned_dirs: Vec<String>,
}

impl WalkTally {
    pub(crate) fn unreadable(&mut self, position: u32, denied: bool) {
        self.events.push((position, Counted::Unreadable { denied }));
    }

    /// `relative`: the ignored entry's root-relative path, a directory's
    /// with a trailing `/`.
    pub(crate) fn ignored(&mut self, position: u32, relative: String) {
        self.events.push((position, Counted::Ignored(relative)));
    }

    pub(crate) fn pruned(&mut self, position: u32, relative: String) {
        self.events.push((position, Counted::Pruned(relative)));
    }

    /// The counts of the entries up to and including `position`.
    pub(crate) fn through(&self, position: u32) -> Totals {
        let mut totals = Totals::default();
        for (at, counted) in &self.events {
            if *at > position {
                continue;
            }
            match counted {
                Counted::Unreadable { denied } => {
                    totals.skipped += 1;
                    totals.permission_denied += u32::from(*denied);
                }
                Counted::Ignored(relative) => totals.ignored.push(relative.clone()),
                Counted::Pruned(relative) => totals.pruned_dirs.push(relative.clone()),
            }
        }
        totals
    }
}
