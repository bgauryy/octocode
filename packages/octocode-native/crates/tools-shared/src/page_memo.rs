//! A store of results kept in memory, keyed by what proves them current: a
//! continuation's snapshot and the path policy that produced it, or the
//! digest of the content they were computed from (outlines, declarations,
//! mentions), which a fresh request may reuse. A value lives as issued
//! continuation state does ([`octocode_runtime_base::cache::CONTINUATION_RETENTION`]): it
//! expires [`CONTINUATION_IDLE`] after its last read. A hit is only a
//! candidate: each owner re-validates what it reads before serving a page.
use octocode_engine::stamp::{Link, Proof, SourceStamp, exists};
use octocode_io::memo::Memo;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use octocode_runtime_base::cache::CONTINUATION_IDLE;

/// Values by snapshot, each stored with the policy that produced it.
pub struct PageMemo<V> {
    memo: Memo<String, (String, V)>,
    /// The keys a request computes now ([`PageMemo::get_or_compute`]).
    flights: Flights,
}

/// What a computation of [`PageMemo::get_or_compute`] gives: its value, and
/// the bytes it is kept at, `None` when it is not kept (it stopped early, or
/// cannot be measured).
pub struct Computed<V> {
    pub value: V,
    pub keep: Option<usize>,
}

/// A request's turn at computing a key ([`PageMemo::turn`]).
pub enum Turn<'a> {
    /// This request computes it; dropping the hold lands the flight.
    Lead(Leading<'a>),
    /// Another computed it and is done: read what it kept.
    Landed,
    /// This request was stopped while another computed it: it computes for
    /// itself.
    Stopped,
}

/// The keys being computed, each with the signal its waiters wait on.
struct Flights(std::sync::Mutex<std::collections::BTreeMap<String, Arc<Flight>>>);

/// One key's computation: it lands when its request finishes, keeping its
/// value or not, or unwinds.
#[derive(Default)]
struct Flight {
    landed: std::sync::Mutex<bool>,
    signal: std::sync::Condvar,
}

/// How often a waiter reads its own stop while it waits: it never outwaits
/// its own cancellation or deadline by more than this.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::disallowed_methods,
        reason = "how often a waiter reads its own stop, not a work budget: no limit or deadline derives it"
    )
)]
const STOP_POLL: std::time::Duration = std::time::Duration::from_millis(10);

impl Flight {
    /// Wait until the flight lands (`true`), or until `stop` says this
    /// waiter was stopped (`false`).
    fn wait(&self, stop: &dyn Fn() -> bool) -> bool {
        let mut landed = self
            .landed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while !*landed {
            if stop() {
                return false;
            }
            landed = self
                .signal
                .wait_timeout(landed, STOP_POLL)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
        true
    }
}

/// The computing request's hold on its key: dropping it (done, or
/// unwinding) lands the flight and wakes its waiters.
pub struct Leading<'a> {
    flights: &'a Flights,
    key: String,
    flight: Arc<Flight>,
}

impl Drop for Leading<'_> {
    fn drop(&mut self) {
        let mut flights = self
            .flights
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if flights
            .get(&self.key)
            .is_some_and(|flight| Arc::ptr_eq(flight, &self.flight))
        {
            flights.remove(&self.key);
        }
        drop(flights);
        *self
            .flight
            .landed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        self.flight.signal.notify_all();
    }
}

impl Flights {
    const fn new() -> Self {
        Self(std::sync::Mutex::new(std::collections::BTreeMap::new()))
    }

    /// Lead the computation of `key`, or the flight another request leads.
    fn join(&self, key: &str) -> Result<Leading<'_>, Arc<Flight>> {
        let mut flights = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(flight) = flights.get(key) {
            return Err(Arc::clone(flight));
        }
        let flight = Arc::new(Flight::default());
        flights.insert(key.to_owned(), Arc::clone(&flight));
        Ok(Leading {
            flights: self,
            key: key.to_owned(),
            flight,
        })
    }
}

impl<V: Send + Sync + 'static> Default for PageMemo<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: Send + Sync + 'static> PageMemo<V> {
    /// A store drawing on the one process budget ([`octocode_engine::memory::MEMORY`]),
    /// the least recently used of every store going first; each value
    /// expires [`CONTINUATION_IDLE`] after it was last read.
    pub const fn new() -> Self {
        Self {
            memo: Memo::new(&octocode_engine::memory::MEMORY),
            flights: Flights::new(),
        }
    }

    /// A store drawing on `budget` alone, for a test whose kept values must
    /// not be evicted by other tests filling the process budget.
    #[cfg(test)]
    const fn with_budget(budget: &'static octocode_io::memo::MemoryBudget) -> Self {
        Self {
            memo: Memo::new(budget),
            flights: Flights::new(),
        }
    }

    /// The requests waiting on the flight computing `key` now, for tests.
    #[cfg(test)]
    fn waiting(&self, key: &str) -> usize {
        self.flights
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(key)
            // The map and the leader hold it besides the waiters.
            .map_or(0, |flight| Arc::strong_count(flight).saturating_sub(2))
    }

    /// `read` of the unexpired value stored for `snapshot` under `policy`;
    /// the read restarts its lifetime, so a cursor being paged stays served.
    pub fn get<T>(&self, snapshot: &str, policy: &str, read: impl FnOnce(&V) -> T) -> Option<T> {
        let hit = self.memo.get(snapshot)?;
        let (stored_policy, value) = hit.value.as_ref();
        if stored_policy != policy {
            return None;
        }
        self.memo.touch(snapshot, CONTINUATION_IDLE);
        Some(read(value))
    }

    /// The value kept for `key` under `policy`, else the one the request
    /// computing it now keeps, else `compute`'s, kept when it says so.
    /// Concurrent requests for one key compute it once: the others wait for
    /// the one computing it. A waiter computes itself when that one keeps
    /// nothing (it stopped, or it unwound), or when `stop` says the waiter
    /// itself was stopped, so no request waits past its own cancellation or
    /// deadline, and nothing a stopped request computed is kept unless it
    /// says so. `compute` must not ask this store for `key` again.
    pub fn get_or_compute(
        &self,
        key: String,
        policy: &str,
        stop: &dyn Fn() -> bool,
        compute: impl FnOnce() -> Computed<V>,
    ) -> V
    where
        V: Clone,
    {
        loop {
            if let Some(kept) = self.get(&key, policy, Clone::clone) {
                return kept;
            }
            let leading = match self.turn(&key, stop) {
                // What it kept is read on the next turn; when it kept
                // nothing this request leads the next flight.
                Turn::Landed => continue,
                Turn::Lead(leading) => Some(leading),
                Turn::Stopped => None,
            };
            let Computed { value, keep } = compute();
            if let Some(bytes) = keep {
                self.put(key, policy.to_owned(), bytes, value.clone());
            }
            drop(leading);
            return value;
        }
    }

    /// This request's turn at computing `key`: lead it ([`Turn::Lead`],
    /// whose drop lands the flight), or wait for the request computing it
    /// until it lands ([`Turn::Landed`]: read what it kept, and take a turn
    /// again when it kept nothing), or compute for itself at once when `stop`
    /// says it was stopped while it waited ([`Turn::Stopped`]), so no
    /// request waits past its own cancellation or deadline. The computation
    /// must not ask for `key` again.
    pub fn turn(&self, key: &str, stop: &dyn Fn() -> bool) -> Turn<'_> {
        match self.flights.join(key) {
            Ok(leading) => Turn::Lead(leading),
            Err(flight) if flight.wait(stop) => Turn::Landed,
            Err(_) => Turn::Stopped,
        }
    }

    /// The hit stored for `snapshot` under `policy` while it is current:
    /// `read` copies the stored value's parts and its [`Proof`] out of the
    /// store, and `serve` turns the parts into the hit once the proof holds.
    /// A value whose proof fails, or that `serve` refuses, is evicted.
    pub fn get_current<R, T>(
        &self,
        snapshot: &str,
        policy: &str,
        read: impl FnOnce(&V) -> (R, Proof),
        serve: impl FnOnce(R) -> Option<T>,
    ) -> Option<T> {
        let (parts, proof) = self.get(snapshot, policy, read)?;
        let hit = proof.holds().then(|| serve(parts)).flatten();
        if hit.is_none() {
            self.evict(snapshot);
        }
        hit
    }

    /// Store `value`, about `bytes` large, replacing any value for the same
    /// snapshot; expired values go first, then the least recently used of
    /// every store until the budget holds. A value over the whole budget is
    /// not kept. The key and policy count toward its weight.
    pub fn put(&self, snapshot: String, policy: String, bytes: usize, value: V) {
        let weight = bytes + snapshot.len() + policy.len();
        self.memo
            .insert(snapshot, (policy, value), weight, Some(CONTINUATION_IDLE));
    }

    /// Charge `bytes` more to the value stored for `snapshot`, for what a later
    /// page learned of it; false (nothing charged) when the budget cannot hold
    /// them without reclaiming, or nothing is stored ([`Memo::charge`]).
    pub fn charge(&self, snapshot: &str, bytes: usize) -> bool {
        self.memo.charge(snapshot, bytes)
    }

    /// Drop the value stored for `snapshot`, as expiry or eviction would.
    pub fn evict(&self, snapshot: &str) {
        self.memo.remove(snapshot);
    }

    /// Stored snapshots, least recently used first, and their total weight.
    #[cfg(any(test, feature = "test-support"))]
    pub fn contents(&self) -> (Vec<String>, usize) {
        self.memo.contents()
    }

    /// Drop every value stored under `policy`, as eviction would. Tests share
    /// the process's stores: one forgets what its own calls kept (a workspace
    /// root of its own is a policy of its own), and what the tests beside it
    /// kept stays.
    #[cfg(any(test, feature = "test-support"))]
    pub fn evict_policy(&self, policy: &str) {
        for snapshot in self.memo.contents().0 {
            if self
                .memo
                .get(&snapshot)
                .is_some_and(|hit| hit.value.0 == policy)
            {
                self.memo.remove(&snapshot);
            }
        }
    }
}

/// Concurrent identical requests that share one computation's result: the
/// first to ask for a key leads and computes, and hands what it computed to
/// every request that asked for the same key while it computed. Nothing
/// outlives the flight: a request that asks after it landed computes
/// afresh, so only requests that ran at once share, and each decides by
/// its own check whether the handed result still holds.
pub struct Shared<V> {
    flights: std::sync::Mutex<std::collections::BTreeMap<String, Arc<Handoff<V>>>>,
}

/// One key's flight: what its leader hands its waiters, once it lands.
struct Handoff<V> {
    landed: std::sync::Mutex<Flown<V>>,
    signal: std::sync::Condvar,
}

/// Where a flight stands, or what a waiter got of it.
enum Flown<V> {
    /// Its leader computes.
    Computing,
    /// It landed handing this to its waiters.
    Handed(Arc<V>),
    /// It landed handing nothing: its leader failed, unwound, or made
    /// nothing that holds for another request.
    Nothing,
    /// The waiter was stopped before it landed.
    Stopped,
}

impl<V> Handoff<V> {
    /// What the flight handed, once it lands; [`Flown::Stopped`] when
    /// `stop` says this waiter was stopped first.
    fn wait(&self, stop: &dyn Fn() -> bool) -> Flown<V> {
        let mut landed = self
            .landed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            match &*landed {
                Flown::Handed(handed) => return Flown::Handed(Arc::clone(handed)),
                Flown::Nothing | Flown::Stopped => return Flown::Nothing,
                Flown::Computing => {}
            }
            if stop() {
                return Flown::Stopped;
            }
            landed = self
                .signal
                .wait_timeout(landed, STOP_POLL)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }
}

/// The leader's hold on its flight: dropping it lands the flight with what
/// it hands (nothing, when it unwound) and wakes the waiters.
struct Leads<'a, V> {
    shared: &'a Shared<V>,
    key: String,
    handoff: Arc<Handoff<V>>,
    handed: Option<Arc<V>>,
}

impl<V> Drop for Leads<'_, V> {
    fn drop(&mut self) {
        let mut flights = self
            .shared
            .flights
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if flights
            .get(&self.key)
            .is_some_and(|flight| Arc::ptr_eq(flight, &self.handoff))
        {
            flights.remove(&self.key);
        }
        drop(flights);
        *self
            .handoff
            .landed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            self.handed.take().map_or(Flown::Nothing, Flown::Handed);
        self.handoff.signal.notify_all();
    }
}

impl<V> Default for Shared<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> Shared<V> {
    pub const fn new() -> Self {
        Self {
            flights: std::sync::Mutex::new(std::collections::BTreeMap::new()),
        }
    }

    /// The result of this request for `key`. A request that asks while
    /// another computes `key` waits for it and makes its result from what
    /// that one handed (`take`); it computes for itself (`compute`) when the
    /// leader handed nothing, when `take` refuses what it handed (it no
    /// longer holds), or when `stop` says this request was stopped while
    /// it waited, so no request waits past its own cancellation or
    /// deadline. Otherwise it leads: `compute` gives its own result and
    /// what it hands the requests waiting on it, and may ask (its argument)
    /// whether any request waits on it yet, to make nothing to hand when
    /// none does. `compute` must not ask for `key` again.
    pub fn share<T>(
        &self,
        key: String,
        stop: &dyn Fn() -> bool,
        compute: impl FnOnce(&dyn Fn() -> bool) -> (T, Option<V>),
        take: impl FnOnce(&V) -> Option<T>,
    ) -> T {
        let joined = {
            let mut flights = self
                .flights
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match flights.get(&key) {
                Some(flight) => Err(Arc::clone(flight)),
                None => {
                    let handoff = Arc::new(Handoff {
                        landed: std::sync::Mutex::new(Flown::Computing),
                        signal: std::sync::Condvar::new(),
                    });
                    flights.insert(key.clone(), Arc::clone(&handoff));
                    Ok(handoff)
                }
            }
        };
        match joined {
            Ok(handoff) => {
                let mut leads = Leads {
                    shared: self,
                    key,
                    handoff,
                    handed: None,
                };
                // The map and this leader hold the flight besides its waiters.
                let waited = || Arc::strong_count(&leads.handoff) > 2;
                let (own, handed) = compute(&waited);
                leads.handed = handed.map(Arc::new);
                drop(leads);
                own
            }
            Err(flight) => {
                if let Flown::Handed(handed) = flight.wait(stop)
                    && let Some(own) = take(&handed)
                {
                    return own;
                }
                compute(&|| false).0
            }
        }
    }

    /// How many keys are being computed now, for tests.
    #[cfg(test)]
    fn in_flight_keys(&self) -> usize {
        self.flights
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// The requests waiting on the flight computing `key` now, for tests.
    #[cfg(any(test, feature = "test-support"))]
    pub fn waiting(&self, key: &str) -> usize {
        self.flights
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(key)
            // The map and the leader hold it besides the waiters.
            .map_or(0, |flight| Arc::strong_count(flight).saturating_sub(2))
    }
}

/// A stored value and what proves it current.
pub struct Proven<T: ?Sized> {
    proof: Proof,
    value: Arc<T>,
}

impl<T: ?Sized + Send + Sync + 'static> PageMemo<Proven<T>> {
    /// The value stored for `snapshot` under `policy` while its proof holds;
    /// one whose proof fails is evicted.
    pub fn current(&self, snapshot: &str, policy: &str) -> Option<Arc<T>> {
        self.get_current(
            snapshot,
            policy,
            |stored| (Arc::clone(&stored.value), stored.proof.clone()),
            Some,
        )
    }

    /// Store `value`, about `bytes` large, with what proves it current.
    pub fn put_proven(
        &self,
        snapshot: String,
        policy: String,
        bytes: usize,
        proof: Proof,
        value: Arc<T>,
    ) {
        self.put(snapshot, policy, bytes, Proven { proof, value });
    }
}

/// What a stored result's freshness depends on.
pub struct Watch<'a> {
    /// The scope root: stamped, with every directory holding a source.
    pub root: &'a Path,
    /// Entries the scan read or walked: each keeps its stamp.
    pub sources: &'a [PathBuf],
    /// Files whose contents decide the walk's ignore rules: a present one
    /// keeps its stamp, an absent one stays absent.
    pub rules: &'a [PathBuf],
    /// Entries whose existence alone decides the rules (repository markers).
    pub markers: &'a [PathBuf],
}

impl Watch<'_> {
    /// The proof of this watch: the root, every source and each directory
    /// under the root holding one, and each present rule file stamped
    /// (`link` decides whether a link is stamped as itself or its target);
    /// each absent rule file and each marker by presence. `None` when a
    /// stamped entry is gone, or was written within the settle window
    /// before `settled_before`.
    pub fn capture(&self, link: Link, settled_before: Option<SystemTime>) -> Option<Proof> {
        self.capture_with_taken(link, settled_before, Vec::new())
    }

    /// [`Self::capture`], with `taken` too: sources the walk stamped itself,
    /// as `link` reads them. Each joins the proof with the walk's stamp
    /// instead of a second stat ([`Proof::capture_with_taken`], which refuses
    /// an unsettled one as a stat would), and its directory is stamped. A path
    /// the walk stamped twice is checked against both stamps: when they
    /// differ, it changed under the walk and the proof fails.
    pub fn capture_with_taken(
        &self,
        link: Link,
        settled_before: Option<SystemTime>,
        taken: Vec<(PathBuf, SourceStamp)>,
    ) -> Option<Proof> {
        /// The directory holding `source`, when it lies under `root`.
        fn directory<'p>(source: &'p Path, root: &Path) -> Option<&'p Path> {
            source.parent().filter(|parent| parent.starts_with(root))
        }
        let root = self.root;
        // Deduplicated borrowed, so each shared parent is copied once.
        let mut paths = std::collections::BTreeSet::<&Path>::new();
        paths.insert(root);
        paths.extend(
            taken
                .iter()
                .filter_map(|(source, _)| directory(source, root)),
        );
        for source in self.sources {
            paths.extend(directory(source, root));
            paths.insert(source);
        }
        let mut presence = self
            .markers
            .iter()
            .map(|marker| (marker.clone(), exists(marker)))
            .collect::<Vec<_>>();
        for rule in self.rules {
            if exists(rule) {
                paths.insert(rule);
            } else {
                presence.push((rule.clone(), false));
            }
        }
        let paths = paths.into_iter().map(Path::to_path_buf).collect::<Vec<_>>();
        Some(Proof::capture_with_taken(paths, taken, link, settled_before)?.with_presence(presence))
    }
}

/// What a walked directory adds to the ignore inputs a stored walk watches.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DirectoryInputs {
    /// Its present ignore files (an absent one shows as a change of the
    /// directory's own stamp when it is created).
    pub files: Vec<PathBuf>,
    /// A nested repository's rule files outside the walk (its own
    /// `info/exclude`), which may not exist.
    pub external: Vec<PathBuf>,
    /// An ignore file no stamp can watch: a link, another non-file entry,
    /// an unreadable one, or one `permits` withholds. A walk that depends
    /// on it is not stored.
    pub untracked: bool,
}

/// The ignore inputs of `dir`, a directory a walk of `root` entered: its
/// ignore files, each a regular file `permits` admits, and when `dir` is a
/// repository nested below `root`, the rule files outside the walk its
/// rules read (the root's own are [`external_ignore_inputs`] of the root).
///
/// [`external_ignore_inputs`]: octocode_engine::search::walk::external_ignore_inputs
pub fn directory_ignore_inputs_with(
    dir: &Path,
    root: &Path,
    global: octocode_engine::search::walk::GlobalIgnore,
    permits: impl Fn(&Path) -> bool,
) -> DirectoryInputs {
    let mut inputs = DirectoryInputs::default();
    if dir != root && dir.join(".git").exists() {
        inputs.external = octocode_engine::search::walk::external_ignore_inputs_with(dir, global);
    }
    for name in octocode_engine::search::walk::IGNORE_FILES {
        let ignore = dir.join(name);
        match std::fs::symlink_metadata(&ignore) {
            Ok(metadata) if metadata.is_file() && permits(&ignore) => inputs.files.push(ignore),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => inputs.untracked = true,
        }
    }
    inputs
}

/// The repository markers at and above `root` whose presence decides
/// whether a `.gitignore` applies ([`octocode_engine::search::repository`]).
pub fn repository_markers(root: &Path) -> Vec<PathBuf> {
    root.ancestors()
        .flat_map(|dir| octocode_engine::search::repository::MARKERS.map(|marker| dir.join(marker)))
        .collect()
}

/// A budget of `$bytes` of its own, for a test that must fill one: tests run
/// in parallel, so each owns its budget instead of sharing the process one.
#[cfg(any(test, feature = "test-support"))]
#[macro_export]
macro_rules! test_budget {
    ($bytes:expr) => {{
        fn limit() -> usize {
            $bytes
        }
        static BUDGET: octocode_io::memo::MemoryBudget =
            octocode_io::memo::MemoryBudget::new(limit);
        &BUDGET
    }};
}
#[cfg(any(test, feature = "test-support"))]
pub use test_budget;

/// A new budget of `bytes` on each call, for a test that searches for the
/// least budget some work fits in ([`test_budget!`] fixes its bytes at
/// compile time). A budget reads its limit once, on first use, so each one is
/// built and read under one lock.
#[cfg(test)]
pub(crate) fn sized_test_budget(bytes: usize) -> &'static octocode_io::memo::MemoryBudget {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static LIMIT: AtomicUsize = AtomicUsize::new(0);
    static BUILDING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn limit() -> usize {
        LIMIT.load(Ordering::SeqCst)
    }
    let _building = BUILDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    LIMIT.store(bytes, Ordering::SeqCst);
    let budget: &'static octocode_io::memo::MemoryBudget =
        Box::leak(Box::new(octocode_io::memo::MemoryBudget::new(limit)));
    assert_eq!(budget.limit(), bytes);
    budget
}

#[cfg(test)]
mod budget_tests;

#[cfg(test)]
mod tests;
