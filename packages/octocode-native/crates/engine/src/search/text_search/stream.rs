//! A traversal search that stops once its page is full and resumes where it
//! stopped, like `rg --sort path | head` that can be continued.
//!
//! One feeder walks the tree lazily, siblings by name and depth first, and
//! hands each candidate file an index in that order. Workers search any
//! candidate; a file is committed strictly in index order, so what a page
//! holds and where it ends do not depend on how many workers ran or how they
//! were scheduled. The page-full test runs on commit only: files finished
//! past the cut are discarded, so the search reads at most the page plus
//! what the workers held in flight.
//!
//! A continuation names its first file by index (`from`), where the page
//! before it began ([`Anchor`]: the path of the candidate before that page),
//! and a pin over the candidates of that page up to its own start: the digest
//! of each one's path and stamp, in walk order. The walk starts at the anchor
//! (it lists only the directories on the way there), replays the page before
//! (no search, one stamp each), and a pin that no longer matches says those
//! files changed under the cursor: the caller restarts rather than skip or
//! repeat a row. A page costs its own walk and the one before it, wherever in
//! the tree it stands.
use super::TextSearchPathFilter;
use super::collect::{WalkControl, WorkerRecs, file_searcher, walk_scope};
use super::mode::Mode;
use super::open_gate::OpenGate;
use super::ordered::Finished;
use super::result::{build_stats, file_of};
use super::state::{CollectResult, CollectState, Held};
use super::stream_judge::{StreamJudge, Verdict};
use super::visit::WalkScope;
use crate::digest::{ListedFiles, stamp_of};
use crate::error::Result;
use crate::num::{saturating_u32, saturating_usize};
use crate::search::walk::OrderedWalk;
use crate::stamp::SourceStamp;
use crate::types::{TextSearchFile, TextSearchOptions, TextSearchResult};
use grep_matcher::Matcher;
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};

/// Where a traversal stream starts and when it has read enough.
pub struct Stream {
    /// What the search is: the first thing every pin holds, so a pin of
    /// another search never matches.
    pub bind: String,
    /// The candidates, in walk order, before the first one searched: walked
    /// and stamped, never searched.
    pub from: u32,
    /// What the candidates from the anchor to `from` (and the first one
    /// searched, when `covers` is `from + 1`) were when the earlier page read
    /// them.
    pub pin: Option<Pin>,
    /// Where the page before began; [`Anchor::START`] for a first page.
    pub anchor: Anchor,
    /// Called with each committed file in walk order; `true` once that file
    /// overflows the page, which ends the stream after it (its rows past the
    /// page are the next page's).
    pub enough: Box<dyn FnMut(&TextSearchFile) -> bool + Send>,
    /// Called on the worker thread that searched a file, before its commit,
    /// with the path of each result file it found there (as
    /// [`TextSearchFile::path`] names it): what `enough` reads of a file
    /// (its outline) is made here, in parallel, so the commit, which runs
    /// in walk order and holds the page, never waits on it. `None`: nothing
    /// to prepare.
    pub prepare: Option<Prepare>,
}

/// Work a stream's workers do on a result file, by its path, before its
/// commit ([`Stream::prepare`]).
pub type Prepare = Box<dyn Fn(&str) + Send + Sync>;

/// The digest of the bind and of the paths and stamps of the candidates from
/// the anchor's index to `covers`.
pub struct Pin {
    pub covers: u32,
    pub hex: String,
}

/// Where one returned file stands in the walk, with the pin of the
/// candidates of its page before it and through it.
pub struct Landmark {
    /// Its index among the candidates, in walk order.
    pub position: u32,
    /// The pin over the page's candidates before it.
    pub before: String,
    /// The pin over the page's candidates through it.
    pub after: String,
    /// The files the page searched before it: a page that continues at
    /// this file searches it and every candidate after it again.
    pub searched: u32,
}

/// Where a page began, for the page that continues it: the walk starts just
/// past the candidate before that page (`after`), counts the next candidate
/// as `index`, and replays the page up to its continuation's start. Entries
/// the walk left out before candidate `released` that page reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Anchor {
    index: u32,
    /// The names of the path below the root of the candidate before `index`,
    /// as the listings spell them; `None`: the walk's start.
    after: Option<Vec<Vec<u8>>>,
    released: u32,
}

impl Anchor {
    /// The walk's start: a first page.
    pub const START: Self = Self {
        index: 0,
        after: None,
        released: 0,
    };

    /// The anchor as cursor text: `<index>.<released>`, then, past the walk's
    /// start, `.` and each name of `after` in hex, `/` between names.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut text = format!("{}.{}", self.index, self.released);
        if let Some(names) = &self.after {
            text.push('.');
            let names = names.iter().map(hex::encode).collect::<Vec<_>>();
            text.push_str(&names.join("/"));
        }
        text
    }

    /// The anchor [`Self::encode`] wrote; `None` for any other text.
    #[must_use]
    pub fn decode(text: &str) -> Option<Self> {
        let mut parts = text.splitn(3, '.');
        let number = |part: Option<&str>| {
            part.filter(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))?
                .parse::<u32>()
                .ok()
        };
        let index = number(parts.next())?;
        let released = number(parts.next())?;
        let after = match parts.next() {
            None => None,
            Some("") => Some(Vec::new()),
            Some(names) => Some(
                names
                    .split('/')
                    .map(|name| {
                        (!name.is_empty() && !name.bytes().any(|b| b.is_ascii_uppercase()))
                            .then(|| hex::decode(name).ok())
                            .flatten()
                    })
                    .collect::<Option<Vec<_>>>()?,
            ),
        };
        Some(Self {
            index,
            after,
            released,
        })
    }
}

/// What a stream read.
pub struct Streamed {
    /// The files committed in walk order (those holding hits, or the ones a
    /// files-without view lists), and the stats of every candidate searched.
    pub result: TextSearchResult,
    /// One per file of `result`.
    pub landmarks: Vec<Landmark>,
    /// The candidates before it are searched or passed; the next page
    /// starts there unless it re-opens the page's last file.
    pub through: u32,
    /// No candidate lies at or after `through`: the walk is over.
    pub ended: bool,
    /// The pin over the page's candidates before `through`.
    pub pin: String,
    /// Where this page began, for the page that continues it.
    pub anchor: Anchor,
}

/// How a stream ended.
pub enum Streaming {
    Streamed(Box<Streamed>),
    /// The pin no longer matches the candidates it covered: a file in the
    /// prefix was added, removed or changed.
    Stale,
}

/// The lazy walk, in walk order: it hands each candidate file its index.
struct Feeder<'a> {
    walk: OrderedWalk<Verdict>,
    judge: StreamJudge<'a>,
    /// The index the next candidate gets.
    next: usize,
    /// The first candidate the page searches, and the one before it once
    /// walked: where the page that continues this one starts its walk.
    from: usize,
    before_from: Option<PathBuf>,
}

impl Feeder<'_> {
    /// The next candidate file, or `None` at the walk's end or once it must
    /// stop (stopped, cancelled).
    fn candidate(&mut self) -> Option<(usize, PathBuf)> {
        let path = self.walk.next(&mut self.judge)?;
        let index = self.next;
        self.next += 1;
        if index + 1 == self.from {
            self.before_from = Some(path.clone());
        }
        Some((index, path))
    }
}

/// A candidate file: its index in walk order, its entry and its stamp when
/// the page's replay read it already.
type Candidate = (usize, PathBuf, Option<SourceStamp>);

/// What the committing worker keeps.
struct Commit {
    /// The candidate committed next.
    next: usize,
    /// The first candidate this stream walked: the start of the page before.
    start: usize,
    pending: BTreeMap<usize, Finished>,
    /// The page's candidates before `next`, as the search read them.
    listed: ListedFiles,
    /// The candidates the walker produced: at most `window` past `next`, so
    /// a worker slow on one file never lets the others read far ahead of
    /// the page cut.
    handed: usize,
    /// How many candidates past `next` the walker may have handed out. It
    /// starts at `base`, a few per thread, and doubles each time the workers
    /// sit idle with the window full: one slow file at the head then holds
    /// the window, not the other files' reading. It never passes the
    /// candidates this stream walked and committed so far (from `start`): a
    /// stream that fills its page early reads no further past it than it
    /// started with, and one that read far spends at most as much again on
    /// the reading past its end. A fixed ceiling
    /// would instead stall every worker behind one slow file, whenever it
    /// takes longer than the workers need to search the window.
    window: usize,
    base: usize,
    /// The window was widened and the walker has not handed out more since.
    widened: bool,
    /// The produced candidates no worker took yet, in walk order.
    queue: VecDeque<Candidate>,
    /// The walker is done: the walk ended or it must stop.
    walk_ended: bool,
    /// What the committed candidates counted (no files: those are below).
    done: CollectResult,
    files: Vec<(Landmark, TextSearchFile)>,
    not_utf8: Vec<String>,
    utf8_bom: Vec<String>,
    enough: Box<dyn FnMut(&TextSearchFile) -> bool + Send>,
    full: bool,
}

/// Run one traversal stream of the linear engine.
pub(super) fn run<M: Matcher + Sync>(
    opts: &TextSearchOptions,
    mode: Mode,
    matchers: (&M, &[M]),
    path_filter: Arc<dyn TextSearchPathFilter>,
    cancelled: &(dyn Fn() -> bool + Sync),
    stream: Stream,
) -> Result<Streaming> {
    let state = CollectState::new();
    let held = Arc::new(Held::new(opts.max_held_bytes));
    let (_, mut scope) = walk_scope(opts, mode, &state, WalkControl { cancelled })?;
    scope.drains = true;
    let Stream {
        bind,
        from,
        pin,
        anchor,
        enough,
        prepare,
    } = stream;
    let prepare = prepare.as_deref();
    let from = saturating_usize(from);
    let first = saturating_usize(anchor.index);
    if first > from {
        return Ok(Streaming::Stale);
    }
    // The host's share of cores is one budget: the threads that search and
    // the threads that read directories ahead of the walk divide it.
    let room = share();
    let (threads, reading, opening) = budget(room);
    scope.open_gate = (opening < threads).then(|| OpenGate::new(opening));
    let root = Path::new(&opts.path);
    let mut feeder = Feeder {
        // The walker's depth counts the root as 0, the contract's as -1.
        walk: OrderedWalk::new(
            root,
            opts.max_depth.map(|max_depth| max_depth as usize + 1),
            reading,
        ),
        judge: StreamJudge::new(&scope, &path_filter, first),
        next: first,
        from,
        before_from: None,
    };
    if let Some(after) = &anchor.after {
        feeder.walk.seek_after(after, &mut feeder.judge);
    }
    // The page before is walked again from its start and stamped, not
    // searched; the candidate that opens the page, when it is a file's
    // continuation, is stamped here too so its pin is read before its
    // search.
    let covers = pin
        .as_ref()
        .map_or(from, |pin| saturating_usize(pin.covers));
    let stamped = covers.max(from);
    let mut bound = ListedFiles::default();
    bound.add(std::path::Path::new(&bind), None);
    // `room`: candidates handed out past the last one committed, so a worker
    // slow on one file never lets the others read far ahead of the page cut.
    let replayed = replay_prefix(
        std::iter::from_fn(|| {
            if feeder.next < stamped {
                feeder.candidate()
            } else {
                None
            }
        }),
        (|path: &PathBuf| path.as_path(), stamp_of),
        (first, from, stamped, room),
        bound.clone(),
    );
    if replayed.count < stamped - first {
        if state.stopped() {
            return finish(&scope, &state, &held, opts, mode, (None, anchor));
        }
        return Ok(Streaming::Stale);
    }
    let Replayed { check, opening, .. } = replayed;
    if let Some(pin) = &pin
        && check.hex() != pin.hex
    {
        return Ok(Streaming::Stale);
    }
    let committed = Condvar::new();
    let work = Condvar::new();
    let commit = Mutex::new(Commit {
        next: from,
        start: first,
        pending: BTreeMap::new(),
        // The page's own pins start over at its first candidate: the page
        // that continues it replays it from there.
        listed: bound,
        handed: stamped,
        window: CANDIDATES_PER_THREAD * room,
        base: CANDIDATES_PER_THREAD * room,
        widened: false,
        queue: opening.into_iter().collect(),
        walk_ended: false,
        done: CollectResult::default(),
        files: Vec::new(),
        not_utf8: Vec::new(),
        utf8_bom: Vec::new(),
        enough,
        full: false,
    });
    let (scope_ref, commit_ref, held_ref, state_ref) = (&scope, &commit, &held, &state);
    let (committed_ref, work_ref) = (&committed, &work);
    let feeder = std::thread::scope(|threads_scope| {
        // The only thread that walks: it feeds the queue the workers take
        // from, at most `room` candidates past the last one committed.
        let walker = threads_scope.spawn(move || {
            walk_candidates(feeder, (commit_ref, committed_ref, work_ref), state_ref)
        });
        for _ in 0..threads {
            threads_scope.spawn(move || {
                let mut searcher =
                    file_searcher(scope_ref, mode, matchers, (&state_ref.stop, held_ref));
                while let Some((index, path, stamp)) =
                    take_candidate((commit_ref, committed_ref, work_ref), state_ref)
                {
                    let stamp = stamp.or_else(|| stamp_of(&path));
                    let one = CollectState::new();
                    {
                        let mut recs = WorkerRecs {
                            local: Vec::new(),
                            state: &one,
                        };
                        scope_ref.search_entry(&one, &path, &mut searcher, &mut recs);
                    }
                    let found = one.snapshot();
                    if let Some(prepare) = prepare.filter(|_| !state_ref.stopped()) {
                        for rec in &found.recs {
                            prepare(&rec.path);
                        }
                    }
                    let finished = Finished::new(path, stamp, found);
                    commit_files(
                        opts,
                        mode,
                        (state_ref, commit_ref, (committed_ref, work_ref)),
                        index,
                        finished,
                    );
                }
            });
        }
        walker
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    });
    let commit = commit
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut feeder = feeder;
    let ended = feeder.walk.ended() && commit.next == feeder.next && !state.stopped();
    // A walk that ran to its end reports every entry it left out. Otherwise
    // only those before the last committed candidate: whether the walker had
    // judged the entries between it and the next candidate when the page
    // filled depends on the read-ahead. What lies before the page before's
    // cut that page reported.
    feeder.judge.release(
        saturating_usize(anchor.released),
        if ended { usize::MAX } else { commit.next },
    );
    // The page that continues this one starts its walk just past the
    // candidate before this page's first: walked here, or the anchor this
    // page started past.
    let after = if from > first {
        feeder
            .before_from
            .take()
            .map(|path| names_below(root, &path))
    } else {
        anchor.after
    };
    let next = Anchor {
        index: saturating_u32(from),
        after,
        released: saturating_u32(commit.next),
    };
    finish(
        &scope,
        &state,
        &held,
        opts,
        mode,
        (Some((commit, ended)), next),
    )
}

/// The names of `path` below `root`, as the walk's listings spell them.
fn names_below(root: &Path, path: &Path) -> Vec<Vec<u8>> {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|name| name.as_os_str().as_encoded_bytes().to_vec())
        .collect()
}

/// The share of the host's cores this call holds.
fn share() -> usize {
    #[cfg(test)]
    if let Some(share) = tests::SHARE.with(std::cell::Cell::get) {
        return share;
    }
    crate::cpu::host().share().max(1)
}

/// Candidates past the last one committed the walker may hand out at first,
/// for each thread that searches.
const CANDIDATES_PER_THREAD: usize = 2;

/// The threads of one stream out of the `share` of cores this call holds:
/// every core searches, directories are read ahead by a quarter of them (the
/// walk reads for itself when one core is all there is), and a third of them
/// at most open files at once ([`OpenGate`]): both wait on the kernel's path
/// lookups, which do not scale with the threads that issue them.
fn budget(share: usize) -> (usize, usize, usize) {
    let reading = if share > 1 { share.div_ceil(4) } else { 0 };
    (share, reading, share.div_ceil(3))
}

/// What replaying the page before a continuation left.
pub(super) struct Replayed<T> {
    /// The candidates replayed.
    pub(super) count: usize,
    /// The pin's digest over them, and the bind before them.
    pub(super) check: ListedFiles,
    /// The last candidate at or after `from`, with its stamp.
    pub(super) opening: Option<(usize, T, Option<SourceStamp>)>,
}

/// Folds stamped candidates into the pin's digest in walk order, whatever
/// order they arrive in.
struct Fold<T> {
    from: usize,
    next: usize,
    waiting: BTreeMap<usize, (T, Option<SourceStamp>)>,
    check: ListedFiles,
    opening: Option<(usize, T, Option<SourceStamp>)>,
}

impl<T> Fold<T> {
    /// A fold whose first candidate is `first`.
    fn new(check: ListedFiles, first: usize, from: usize) -> Self {
        Self {
            from,
            next: first,
            waiting: BTreeMap::new(),
            check,
            opening: None,
        }
    }

    fn accept(
        &mut self,
        (index, item, stamp): (usize, T, Option<SourceStamp>),
        path: &impl Fn(&T) -> &std::path::Path,
    ) {
        self.waiting.insert(index, (item, stamp));
        while let Some((item, stamp)) = self.waiting.remove(&self.next) {
            let index = self.next;
            self.next += 1;
            self.check.add(path(&item), stamp);
            if index >= self.from {
                self.opening = Some((index, item, stamp));
            }
        }
    }

    fn done(self, count: usize) -> Replayed<T> {
        Replayed {
            count,
            check: self.check,
            opening: self.opening,
        }
    }
}

/// Stamp the candidates `first..end` of the page before a continuation on
/// `workers` threads while `candidates` is still walking, and fold the stamps
/// into the pin's digest in walk order: the digest does not depend on which
/// thread stamped a file or when. `from` is the first candidate the page
/// searches.
pub(super) fn replay_prefix<T: Send>(
    candidates: impl Iterator<Item = (usize, T)>,
    (path, stamp): (
        impl Fn(&T) -> &std::path::Path + Sync,
        impl Fn(&std::path::Path) -> Option<SourceStamp> + Sync,
    ),
    (first, from, end, workers): (usize, usize, usize, usize),
    check: ListedFiles,
) -> Replayed<T> {
    let workers = workers.min(end.saturating_sub(first));
    let mut fold = Fold::new(check, first, from);
    let mut count = 0;
    if workers <= 1 {
        for (index, item) in candidates {
            let stamped = stamp(path(&item));
            fold.accept((index, item, stamped), &path);
            count += 1;
        }
        return fold.done(count);
    }
    let (path, stamp) = (&path, &stamp);
    std::thread::scope(|scope| {
        let (results, arrived) = std::sync::mpsc::channel::<(usize, T, Option<SourceStamp>)>();
        let senders = (0..workers)
            .map(|_| {
                let (jobs, job) = std::sync::mpsc::sync_channel::<(usize, T)>(2 * workers);
                let results = results.clone();
                scope.spawn(move || {
                    for (index, item) in job {
                        let stamped = stamp(path(&item));
                        if results.send((index, item, stamped)).is_err() {
                            return;
                        }
                    }
                });
                jobs
            })
            .collect::<Vec<_>>();
        drop(results);
        let folder = scope.spawn(move || {
            for arrival in arrived {
                fold.accept(arrival, path);
            }
            fold
        });
        for (index, item) in candidates {
            if senders[index % workers].send((index, item)).is_err() {
                break;
            }
            count += 1;
        }
        drop(senders);
        folder
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            .done(count)
    })
}

/// The walker thread: walk the tree, hand each candidate to the queue, and
/// wait while its window of candidates past the last one committed is out. It ends
/// the walk for the workers however it ends, a panic included.
fn walk_candidates<'a>(
    mut feeder: Feeder<'a>,
    (commit, committed, work): (&Mutex<Commit>, &Condvar, &Condvar),
    state: &CollectState,
) -> Feeder<'a> {
    let _end = WalkEnd(commit, committed, work);
    loop {
        let stopped = {
            let ahead = commit
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let _ahead = committed
                .wait_while(ahead, |commit| {
                    !state.stopped() && commit.handed >= commit.next + commit.window
                })
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.stopped()
        };
        if stopped {
            return feeder;
        }
        let Some((index, dent)) = feeder.candidate() else {
            return feeder;
        };
        commit
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_candidate((index, dent, None));
        work.notify_one();
    }
}

impl Commit {
    /// A commit of nothing: no candidate handed out, none committed.
    fn empty() -> Self {
        Self {
            next: 0,
            start: 0,
            pending: BTreeMap::new(),
            listed: ListedFiles::default(),
            handed: 0,
            window: 0,
            base: 0,
            widened: false,
            queue: VecDeque::new(),
            walk_ended: false,
            done: CollectResult::default(),
            files: Vec::new(),
            not_utf8: Vec::new(),
            utf8_bom: Vec::new(),
            enough: Box::new(|_| false),
            full: false,
        }
    }

    fn push_candidate(&mut self, candidate: Candidate) {
        self.handed += 1;
        self.widened = false;
        self.queue.push_back(candidate);
    }
}

/// Marks the walk over and wakes everyone waiting on it, when the walker
/// thread ends.
struct WalkEnd<'a>(&'a Mutex<Commit>, &'a Condvar, &'a Condvar);

impl Drop for WalkEnd<'_> {
    fn drop(&mut self) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .walk_ended = true;
        self.1.notify_all();
        self.2.notify_all();
    }
}

/// The next candidate for a worker, or `None` when the queue is drained and
/// the walk is over, or the stream stopped. A worker waits for the walker
/// holding no lock but the commit's. One that finds nothing to take because
/// the window is full (the candidates handed out wait on the head) widens it.
fn take_candidate(
    (commit, committed, work): (&Mutex<Commit>, &Condvar, &Condvar),
    state: &CollectState,
) -> Option<Candidate> {
    let mut ahead = commit
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    loop {
        if state.stopped() {
            return None;
        }
        if let Some(candidate) = ahead.queue.pop_front() {
            return Some(candidate);
        }
        if ahead.walk_ended {
            return None;
        }
        let cap = (ahead.next - ahead.start).max(ahead.base);
        if ahead.handed >= ahead.next + ahead.window && !ahead.widened && ahead.window < cap {
            ahead.window = (2 * ahead.window).min(cap);
            ahead.widened = true;
            committed.notify_all();
        }
        ahead = work
            .wait(ahead)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }
}

/// Commit `finished`, the candidate at `index`, and every later candidate
/// that waited on it; once a file overflows the page, nothing after it is.
fn commit_files(
    opts: &TextSearchOptions,
    mode: Mode,
    (state, commit, wake): (&CollectState, &Mutex<Commit>, (&Condvar, &Condvar)),
    index: usize,
    finished: Finished,
) {
    let mut guard = commit
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let commit = &mut *guard;
    // The walker waiting for room wakes whatever this commit does; the
    // workers wait for candidates, which only the walker hands out.
    let _wake = Wake(wake.0);
    if commit.full || state.stopped() {
        return;
    }
    commit.pending.insert(index, finished);
    while let Some(finished) = commit.pending.remove(&commit.next) {
        let (path, stamp, mut result) = finished.into_parts();
        let before = (!result.recs.is_empty()).then(|| commit.listed.hex());
        let searched = saturating_u32(commit.done.files_searched);
        commit.listed.add(&path, stamp);
        let recs = std::mem::take(&mut result.recs);
        commit.done = std::mem::take(&mut commit.done).merge(result);
        for rec in recs {
            if rec.not_utf8 {
                commit.not_utf8.push(rec.path.clone());
            }
            if rec.utf8_bom {
                commit.utf8_bom.push(rec.path.clone());
            }
            let file = file_of(opts, mode, rec);
            commit.full = (commit.enough)(&file);
            let landmark = Landmark {
                position: saturating_u32(commit.next),
                before: before.clone().unwrap_or_default(),
                after: commit.listed.hex(),
                searched,
            };
            commit.files.push((landmark, file));
        }
        commit.next += 1;
        if commit.full {
            state.stop.store(true, Ordering::Relaxed);
            // The workers waiting for a candidate must see the stop.
            wake.1.notify_all();
            return;
        }
    }
}

/// Wakes the walker waiting for room when a commit ends.
struct Wake<'a>(&'a Condvar);

impl Drop for Wake<'_> {
    fn drop(&mut self) {
        self.0.notify_all();
    }
}

/// The stream as its caller reads it, or a refusal past the held bound;
/// `anchor`: where the page that continues it starts.
fn finish(
    scope: &WalkScope<'_>,
    state: &CollectState,
    held: &Arc<Held>,
    opts: &TextSearchOptions,
    mode: Mode,
    (committed, anchor): (Option<(Commit, bool)>, Anchor),
) -> Result<Streaming> {
    scope.record_unreadable_ignore_files();
    if held.exhausted() {
        return Err(held.refusal());
    }
    let (commit, ended) = committed.unwrap_or_else(|| (Commit::empty(), false));
    let Commit {
        next,
        listed,
        done,
        files,
        not_utf8,
        utf8_bom,
        ..
    } = commit;
    let stats = build_stats(
        opts,
        mode,
        state.snapshot().merge(done).settled(),
        (not_utf8, utf8_bom),
    );
    let (landmarks, files) = files.into_iter().unzip();
    Ok(Streaming::Streamed(Box::new(Streamed {
        result: TextSearchResult { files, stats },
        landmarks,
        through: saturating_u32(next),
        ended,
        pin: listed.hex(),
        anchor,
    })))
}

#[cfg(test)]
pub(super) mod tests;
