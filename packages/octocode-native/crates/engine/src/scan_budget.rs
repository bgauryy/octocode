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

/// Paces the chunks of a scan: time is checked between chunks, and each
/// chunk of a timed scan is sized from the throughput seen so far to end
/// within a tenth of the budget and within what is left of it, so slow
/// storage overshoots the budget by about one short chunk rather than a
/// fixed file count. An untimed scan runs maximal chunks.
pub(crate) struct ScanPacer {
    started: std::time::Instant,
    budget: Option<std::time::Duration>,
    evaluated: usize,
}

impl ScanPacer {
    /// `budget: None` never stops the scan.
    pub(crate) fn new(budget: Option<std::time::Duration>) -> Self {
        Self {
            started: std::time::Instant::now(),
            budget,
            evaluated: 0,
        }
    }

    /// Whether the budget is spent; never before the first chunk ran.
    pub(crate) fn spent(&self) -> bool {
        self.evaluated > 0
            && self
                .budget
                .is_some_and(|budget| self.started.elapsed() >= budget)
    }

    /// Candidates the next chunk evaluates.
    pub(crate) fn next_chunk(&self) -> usize {
        chunk_files(self.budget, self.evaluated, self.started.elapsed())
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
    let per_file = elapsed.as_secs_f64() / evaluated as f64;
    if per_file <= 0.0 {
        return MAX_CHUNK_FILES;
    }
    // Saturating float-to-int cast; the clamp keeps it in range.
    ((target.as_secs_f64() / per_file) as usize).clamp(MIN_CHUNK_FILES, MAX_CHUNK_FILES)
}

/// The most candidates one unbounded scan walks.
pub const SCAN_WALK_FILES: usize = 100_000;

#[cfg(any(test, feature = "test-hooks"))]
thread_local! {
    static LOWERED_CEILING: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// The walk ceiling this thread scans under: [`SCAN_WALK_FILES`], or the
/// lowered one of a test.
#[must_use]
pub fn walk_ceiling() -> usize {
    #[cfg(any(test, feature = "test-hooks"))]
    if let Some(lowered) = LOWERED_CEILING.with(std::cell::Cell::get) {
        return lowered;
    }
    SCAN_WALK_FILES
}

/// Lowers this thread's walk ceiling until the guard drops, so a test
/// reaches the ceiling without a 100,000-file tree.
#[cfg(any(test, feature = "test-hooks"))]
#[must_use]
pub fn lower_walk_ceiling(ceiling: usize) -> impl Drop {
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            LOWERED_CEILING.with(|cell| cell.set(None));
        }
    }
    LOWERED_CEILING.with(|cell| cell.set(Some(ceiling)));
    Restore
}

#[cfg(any(test, feature = "test-hooks"))]
thread_local! {
    static CLOCK_AHEAD: std::cell::Cell<std::time::Duration> =
        const { std::cell::Cell::new(std::time::Duration::ZERO) };
}

/// The time a walk's budget is read by on this thread: the monotonic clock,
/// ahead by whatever a test advanced it ([`advance_walk_clock`]).
#[must_use]
pub fn walk_now() -> std::time::Instant {
    let now = std::time::Instant::now();
    #[cfg(any(test, feature = "test-hooks"))]
    let now = now + CLOCK_AHEAD.with(std::cell::Cell::get);
    now
}

/// Advances this thread's walk clock by `by`, so a test spends a walk
/// budget entry by entry without sleeping: every timing it sets up is the
/// same on every run and machine.
#[cfg(any(test, feature = "test-hooks"))]
pub fn advance_walk_clock(by: std::time::Duration) {
    CLOCK_AHEAD.with(|ahead| ahead.set(ahead.get() + by));
}

/// The walk-order index a cut scan resumes from, `None` when the cut is the
/// walk ceiling itself: a resumed scan walks the same first
/// [`walk_ceiling`] candidates and skips the evaluated ones, so a resume
/// at the ceiling skips the whole walk and reaches nothing new. Files past
/// the ceiling are reachable only by narrowing the scope.
#[must_use]
pub fn resume_offset(resume: u32) -> Option<u32> {
    ((resume as usize) < walk_ceiling()).then_some(resume)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn chunks_follow_the_measured_throughput() {
        let budget = Some(Duration::from_secs(10));
        assert_eq!(
            chunk_files(None, 1_000, Duration::from_secs(1)),
            MAX_CHUNK_FILES
        );
        assert_eq!(chunk_files(budget, 0, Duration::ZERO), FIRST_CHUNK_FILES);
        // 10 ms a file: a one-second chunk is 100 files.
        assert_eq!(chunk_files(budget, 128, Duration::from_millis(1_280)), 100);
        // Near the end only what is left of the budget is filled.
        assert_eq!(chunk_files(budget, 950, Duration::from_millis(9_500)), 50);
        // Very slow storage still keeps every worker busy.
        assert_eq!(
            chunk_files(budget, 128, Duration::from_secs(9)),
            MIN_CHUNK_FILES
        );
        // Fast storage runs maximal chunks.
        assert_eq!(
            chunk_files(budget, 128, Duration::from_micros(10)),
            MAX_CHUNK_FILES
        );
    }

    #[test]
    fn a_resume_at_the_walk_ceiling_is_terminal() {
        assert_eq!(resume_offset(1), Some(1));
        assert_eq!(
            resume_offset(SCAN_WALK_FILES as u32 - 1),
            Some(SCAN_WALK_FILES as u32 - 1)
        );
        assert_eq!(resume_offset(SCAN_WALK_FILES as u32), None);
        assert_eq!(resume_offset(u32::MAX), None);
    }

    #[test]
    fn a_lowered_ceiling_applies_to_its_thread_until_dropped() {
        let guard = lower_walk_ceiling(10);
        assert_eq!(walk_ceiling(), 10);
        assert_eq!(resume_offset(9), Some(9));
        assert_eq!(resume_offset(10), None);
        drop(guard);
        assert_eq!(walk_ceiling(), SCAN_WALK_FILES);
    }
}
