//! The time budget of an unbounded file scan.
//!
//! A scan with no caller-chosen `max_files` covers its whole scope when it
//! can, and otherwise stops at a chunk boundary past [`SCAN_TIME_BUDGET`]
//! with the walk-order index to resume from, so a huge tree is read in a few
//! resumable windows rather than cut at a fixed file count.

/// Wall time an unbounded scan spends evaluating candidates before it
/// returns a resumable partial result.
pub const SCAN_TIME_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// Candidates the first chunk of a timed scan evaluates, before any
/// throughput is known.
const FIRST_CHUNK_FILES: usize = 128;
/// Bounds of a paced chunk: enough files to keep every worker busy, and few
/// enough that one chunk on fast storage stays short.
const MIN_CHUNK_FILES: usize = 16;
const MAX_CHUNK_FILES: usize = 4_096;
/// Share of the budget one paced chunk aims to fill.
const CHUNK_BUDGET_SHARE: u32 = 10;

/// What a [`WalkClock`] reads the time from.
pub type WalkNow = dyn Fn() -> std::time::Instant + Send + Sync;

/// The clock a walk or scan reads its time budget on. The default reads
/// [`walk_now`]; a caller that moves time itself (a test) passes its own
/// reading.
#[derive(Clone, Default)]
pub struct WalkClock(Option<std::sync::Arc<WalkNow>>);

impl WalkClock {
    #[must_use]
    pub fn now(&self) -> std::time::Instant {
        match &self.0 {
            Some(now) => now(),
            None => walk_now(),
        }
    }
}

impl From<std::sync::Arc<WalkNow>> for WalkClock {
    fn from(now: std::sync::Arc<WalkNow>) -> Self {
        Self(Some(now))
    }
}

impl std::fmt::Debug for WalkClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.0 {
            Some(_) => "WalkClock(given)",
            None => "WalkClock",
        })
    }
}

/// Paces the chunks of a scan: time is checked between chunks, and each
/// chunk of a timed scan is sized from the throughput seen so far to end
/// within a tenth of the budget and within what is left of it, so slow
/// storage overshoots the budget by about one short chunk rather than a
/// fixed file count. An untimed scan runs maximal chunks.
pub(crate) struct ScanPacer {
    clock: WalkClock,
    started: std::time::Instant,
    budget: Option<std::time::Duration>,
    evaluated: usize,
}

impl ScanPacer {
    /// A pacer whose budget starts now on `clock`; `budget: None` never
    /// stops the scan.
    pub(crate) fn new(budget: Option<std::time::Duration>, clock: WalkClock) -> Self {
        Self {
            started: clock.now(),
            clock,
            budget,
            evaluated: 0,
        }
    }

    /// Whether the budget is spent; never before the first chunk ran.
    pub(crate) fn spent(&self) -> bool {
        self.evaluated > 0 && self.budget.is_some_and(|budget| self.elapsed() >= budget)
    }

    /// Time since the scan began, on its clock.
    fn elapsed(&self) -> std::time::Duration {
        self.clock.now().saturating_duration_since(self.started)
    }

    /// Candidates the next chunk evaluates.
    pub(crate) fn next_chunk(&self) -> usize {
        chunk_files(self.budget, self.evaluated, self.elapsed())
    }

    /// Count `files` more candidates as evaluated.
    pub(crate) fn record(&mut self, files: usize) {
        self.evaluated += files;
    }
}

/// The next chunk's size after `evaluated` candidates took `elapsed`.
fn chunk_files(
    budget: Option<std::time::Duration>,
    evaluated: usize,
    elapsed: std::time::Duration,
) -> usize {
    let Some(budget) = budget else {
        return MAX_CHUNK_FILES;
    };
    if evaluated == 0 {
        return FIRST_CHUNK_FILES;
    }
    let target = (budget / CHUNK_BUDGET_SHARE).min(budget.saturating_sub(elapsed));
    let elapsed = elapsed.as_nanos();
    if elapsed == 0 {
        return MAX_CHUNK_FILES;
    }
    // Files that fit `target` at the observed per-file cost, in integer
    // nanoseconds; the clamp keeps it in range.
    let evaluated = u128::try_from(evaluated).unwrap_or(u128::MAX);
    crate::num::saturating_usize(target.as_nanos().saturating_mul(evaluated) / elapsed)
        .clamp(MIN_CHUNK_FILES, MAX_CHUNK_FILES)
}

#[cfg(any(test, feature = "test-hooks"))]
thread_local! {
    static CLOCK_AHEAD: std::cell::Cell<std::time::Duration> =
        const { std::cell::Cell::new(std::time::Duration::ZERO) };
    static CLOCK_FROZEN_AT: std::cell::Cell<Option<std::time::Instant>> =
        const { std::cell::Cell::new(None) };
}

/// The time a walk's budget is read by on this thread: the monotonic clock
/// (or the instant a test froze it at, [`freeze_walk_clock`]), ahead by
/// whatever a test advanced it ([`advance_walk_clock`]).
#[must_use]
pub fn walk_now() -> std::time::Instant {
    let now = std::time::Instant::now();
    #[cfg(any(test, feature = "test-hooks"))]
    let now = CLOCK_FROZEN_AT.with(std::cell::Cell::get).unwrap_or(now)
        + CLOCK_AHEAD.with(std::cell::Cell::get);
    now
}

/// Stops this thread's walk clock until the guard drops: from then on only
/// [`advance_walk_clock`] moves it, so a budget a test sets is spent at the
/// same entry however slowly a loaded machine runs the walk.
#[cfg(any(test, feature = "test-hooks"))]
#[must_use = "the clock runs again when the guard drops"]
pub fn freeze_walk_clock() -> FrozenWalkClock {
    let at = walk_now() - CLOCK_AHEAD.with(std::cell::Cell::get);
    FrozenWalkClock(CLOCK_FROZEN_AT.replace(Some(at)))
}

/// Restores the walk clock [`freeze_walk_clock`] stopped.
#[cfg(any(test, feature = "test-hooks"))]
pub struct FrozenWalkClock(Option<std::time::Instant>);

#[cfg(any(test, feature = "test-hooks"))]
impl Drop for FrozenWalkClock {
    fn drop(&mut self) {
        CLOCK_FROZEN_AT.set(self.0);
    }
}

/// Advances this thread's walk clock by `by`, so a test spends a walk
/// budget entry by entry without sleeping: every timing it sets up is the
/// same on every run and machine.
#[cfg(any(test, feature = "test-hooks"))]
pub fn advance_walk_clock(by: std::time::Duration) {
    CLOCK_AHEAD.with(|ahead| ahead.set(ahead.get() + by));
}

#[cfg(test)]
mod tests;
