//! The PCRE2 walk in walk order: the candidates first, then each searched on
//! its own and committed in order, so a window can pause after the last file
//! finished in walk order ([`super::window`]).
use super::TextSearchPathFilter;
use super::collect::{WalkControl, WorkerRecs, file_searcher, walk_scope};
use super::mode::Mode;
use super::result::{build_result, preserves_traversal_order};
use super::state::{CollectResult, CollectState, Held};
use super::window::{Clock, Earlier, Pause, Resume, Searched};
use crate::digest::{ListedFiles, stamp_of};
use crate::error::Result;
use crate::num::{saturating_u32, saturating_usize};
use crate::stamp::SourceStamp;
use crate::types::{TextSearchOptions, TextSearchResult};
use grep_matcher::Matcher;
use ignore::WalkState;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// One search's shared state: the walk's own records, and the files
/// committed in walk order.
pub(super) struct Run {
    /// What the walk recorded (errors, depth cuts) and the stop flag every
    /// worker polls.
    pub(super) walk: CollectState,
    /// The bound on what every window of the search keeps, the earlier
    /// windows' kept files included.
    pub(super) held: Arc<Held>,
    commit: Mutex<Commit>,
}

/// The files committed so far, and the ones finished out of order.
#[derive(Default)]
struct Commit {
    /// The candidates before it are searched: earlier windows' or this
    /// one's, and the next file committed is this one.
    next: usize,
    /// Where this window resumed: it pauses only past it.
    floor: usize,
    /// Candidates the walk found, once it ended.
    total: Option<usize>,
    /// Files finished before an earlier one, by position.
    pending: BTreeMap<usize, Finished>,
    /// The candidates before `next`, as the search read them.
    listed: ListedFiles,
    /// What the files before `next` accumulated.
    done: CollectResult,
    paused: bool,
}

/// One searched candidate, with the stamp read before its search.
pub(super) struct Finished {
    path: PathBuf,
    stamp: Option<SourceStamp>,
    result: CollectResult,
}

/// When a window ends and how its time is read.
pub(super) struct Plan {
    pub(super) ends: Option<Instant>,
    pub(super) clock: Clock,
    pub(super) resume: Option<Resume>,
}

impl Run {
    pub(super) fn new(held: Option<usize>) -> Self {
        Self {
            walk: CollectState::new(),
            held: Arc::new(Held::new(held)),
            commit: Mutex::new(Commit::default()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Commit> {
        self.commit
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Commit `finished`, the candidate at `index`, and every file after it
    /// that waited on it; pause when the window's end passed after one of
    /// them, past where the window resumed and before the last candidate.
    /// Nothing is committed once the search stopped: a file searched while
    /// it stopped may be cut short.
    fn commit(&self, index: usize, finished: Finished, (ends, clock): (Option<Instant>, &Clock)) {
        let mut guard = self.lock();
        let commit = &mut *guard;
        if commit.paused || self.walk.stopped() {
            return;
        }
        commit.pending.insert(index, finished);
        while let Some(finished) = commit.pending.remove(&commit.next) {
            commit.listed.add(&finished.path, finished.stamp);
            let done = std::mem::take(&mut commit.done);
            commit.done = done.merge(finished.result);
            commit.next += 1;
            if commit.next > commit.floor
                && commit.total.is_some_and(|total| commit.next < total)
                && ends.is_some_and(|ends| clock() >= ends)
            {
                self.pause(commit);
                return;
            }
        }
    }

    fn pause(&self, commit: &mut Commit) {
        commit.paused = true;
        self.walk.stop.store(true, Ordering::Relaxed);
    }

    /// Pause now, when this window committed a file and any is left: the
    /// driver's stop when no file finishes (one stuck match) past the
    /// window's end.
    pub(super) fn pause_now(&self) {
        let mut commit = self.lock();
        if !commit.paused
            && commit.next > commit.floor
            && commit.total.is_some_and(|total| commit.next < total)
        {
            self.pause(&mut commit);
        }
    }

    /// The pause, once the window paused.
    pub(super) fn paused(&self) -> Option<Pause> {
        let mut commit = self.lock();
        if !commit.paused {
            return None;
        }
        Some(Pause {
            position: saturating_u32(commit.next),
            pin: commit.listed.hex(),
            searched: Searched(std::mem::take(&mut commit.done)),
        })
    }

    /// Stop the search for the caller: what it committed is its result.
    pub(super) fn cancel(&self) {
        self.walk.cancelled.store(true, Ordering::Relaxed);
        self.walk.stop.store(true, Ordering::Relaxed);
    }

    /// The walk's records and every committed file, as one result.
    pub(super) fn whole(&self, opts: &TextSearchOptions, mode: Mode) -> TextSearchResult {
        let done = std::mem::take(&mut self.lock().done);
        build_result(opts, mode, self.walk.snapshot().merge(done).settled())
    }
}

/// The digest of `files`' paths and stamps, as an `lstat` reads them now.
fn listed_now(files: &[ignore::DirEntry]) -> ListedFiles {
    let mut listed = ListedFiles::default();
    for file in files {
        listed.add(file.path(), stamp_of(file.path()));
    }
    listed
}

/// Walk the candidates of `opts`, put them in walk order, and search them,
/// committing each in that order into `run`.
pub(super) fn collect_ordered<M: Matcher + Sync>(
    opts: &TextSearchOptions,
    matchers: (&M, &[M]),
    mode: Mode,
    path_filter: Arc<dyn TextSearchPathFilter>,
    run: &Run,
    Plan {
        ends,
        clock,
        resume,
    }: Plan,
) -> Result<()> {
    // The driver polls the caller and raises `stop`.
    let (walk_builder, scope) = walk_scope(
        opts,
        mode,
        &run.walk,
        WalkControl {
            cancelled: &|| false,
        },
    )?;
    let found = Mutex::new(Vec::new());
    let keep = |dent| match scope.admit(dent, &*path_filter) {
        Ok(dent) => {
            found
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(dent);
            WalkState::Continue
        }
        Err(state) => state,
    };
    if Path::new(&opts.path).is_file() {
        for dent in walk_builder.build() {
            if matches!(keep(dent), WalkState::Quit) {
                break;
            }
        }
    } else {
        walk_builder.build_parallel().run(|| Box::new(&keep));
    }
    scope.record_unreadable_ignore_files();
    let mut files = found
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // A one-worker traversal walk is already in walk order; a parallel walk
    // is put in the order of its paths.
    if !preserves_traversal_order(opts) {
        files.sort_by(|a, b| a.path().cmp(b.path()));
    }
    {
        let mut commit = run.lock();
        commit.total = Some(files.len());
        if let Some(Resume { position, earlier }) = resume {
            let floor = saturating_usize(position);
            commit.floor = floor.min(files.len());
            // What the earlier windows kept serves while the candidates
            // they passed are the same files, unchanged.
            if let Some(Earlier { pin, searched }) = earlier
                && floor <= files.len()
            {
                let listed = listed_now(&files[..floor]);
                if listed.hex() == pin {
                    // What they kept is held from the start of this window.
                    if !run.held.charge(searched.weight()) {
                        run.walk.stop.store(true, Ordering::Relaxed);
                    }
                    commit.next = floor;
                    commit.listed = listed;
                    commit.done = searched.0;
                }
            }
        }
    }
    let next = AtomicUsize::new(run.lock().next);
    let left = files.len().saturating_sub(next.load(Ordering::Relaxed));
    let threads = crate::cpu::host().share().min(left).max(1);
    let (files, scope) = (&files, &scope);
    std::thread::scope(|threads_scope| {
        for _ in 0..threads {
            threads_scope.spawn(|| {
                let mut searcher =
                    file_searcher(scope, mode, matchers, (&run.walk.stop, &run.held));
                while !run.walk.stopped() {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(dent) = files.get(index) else {
                        return;
                    };
                    let stamp = stamp_of(dent.path());
                    let state = CollectState::new();
                    {
                        let mut recs = WorkerRecs {
                            local: Vec::new(),
                            state: &state,
                        };
                        scope.search_entry(&state, dent, &mut searcher, &mut recs);
                    }
                    let finished = Finished {
                        path: dent.path().to_path_buf(),
                        stamp,
                        result: state.snapshot(),
                    };
                    run.commit(index, finished, (ends, &clock));
                }
            });
        }
    });
    Ok(())
}
