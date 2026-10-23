//! PCRE2 (`-P`) searches: the shared matcher settings, the live-worker bound, and the driver that
//! runs a search on a worker thread under a wall-clock deadline.

use super::collect::{WalkControl, collect};
use super::mode::Mode;
use super::result::build_result;
use super::state::CollectState;
use super::{TextSearchPathFilter, to_engine_err};
use crate::error::{Error, Result};
use crate::types::{TextSearchOptions, TextSearchResult};
use grep_pcre2::RegexMatcherBuilder as Pcre2MatcherBuilder;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Cap on PCRE2's JIT stack (1 MiB). A user `-P` pattern with catastrophic
/// backtracking (`(a+)+$`-class) exhausts this cap and fails fast per file
/// instead of spinning against the JIT's default 32 KB stack growth. Residual
/// risk: `grep-pcre2` 0.1 exposes no `match_limit`/`depth_limit` knob, so a
/// backtracking blowup that stays within the JIT stack is still only bounded by
/// PCRE2's internal default match limit (not the wall clock). Applied to every
/// PCRE2 matcher we build (search + pattern validation).
pub(crate) const PCRE2_MAX_JIT_STACK_BYTES: usize = 1 << 20;

/// A PCRE2 builder with the settings every matcher we build shares: UTF and
/// Unicode properties, JIT when available, and the
/// [`PCRE2_MAX_JIT_STACK_BYTES`] stack cap.
pub(crate) fn pcre2_builder() -> Pcre2MatcherBuilder {
    let mut builder = Pcre2MatcherBuilder::new();
    builder
        .utf(true)
        .ucp(true)
        .jit_if_available(true)
        .max_jit_stack_size(Some(PCRE2_MAX_JIT_STACK_BYTES));
    builder
}

/// Wall-clock ceiling for a whole PCRE2 (`-P`) search. PCRE2's JIT-stack cap
/// bounds a single catastrophic backtrack's *memory*, but nothing bounds its
/// *time*: a pathological `-P` pattern can spin for a long time inside a single
/// `find_at`/`search_path` call that cannot be interrupted from the sink. We
/// bound PCRE2 searches two ways, both keyed off this deadline:
///   1. Cooperatively — the collect walk and the match sink poll the deadline
///      between files and between matched lines, so an accumulation of moderately
///      expensive matches stops promptly while keeping partial results.
///   2. Hard — the whole PCRE2 search runs on a worker thread that writes each
///      finished file into shared [`CollectState`]. After
///      `PCRE2_SEARCH_DEADLINE + PCRE2_DEADLINE_GRACE` the driver raises the
///      shared stop flag and returns the files finished so far, flagged
///      `pcre2Deadline`. A thread cannot be killed: a worker inside one
///      uninterruptible match keeps its slot until that match returns, then
///      sees the stop flag and exits without searching further files.
///
/// Only PCRE2 needs this — the default Rust-regex engine is linear and cannot
/// catastrophically backtrack.
pub(crate) const PCRE2_SEARCH_DEADLINE: Duration = Duration::from_secs(5);

/// Extra time the driver waits past the cooperative deadline before it stops
/// waiting for a stuck PCRE2 worker (see [`PCRE2_SEARCH_DEADLINE`]).
pub(crate) const PCRE2_DEADLINE_GRACE: Duration = Duration::from_secs(2);

/// How often the PCRE2 driver wakes to poll caller cancellation and the hard
/// deadline while it waits for the worker.
const PCRE2_DRIVER_POLL: Duration = Duration::from_millis(25);

/// Maximum number of PCRE2 (`-P`) search worker threads alive at once,
/// including workers still finishing an uninterruptible match after the driver
/// stopped waiting. Once saturated, a new `-P` search is rejected rather than
/// spawning another thread (each may hold a 1 MiB JIT stack — see
/// [`PCRE2_MAX_JIT_STACK_BYTES`]).
pub(crate) const MAX_ACTIVE_PCRE2_WORKERS: usize = 8;

/// Wall-clock limits for one PCRE2 search. Production uses
/// [`PCRE2_SEARCH_DEADLINE`] and [`PCRE2_DEADLINE_GRACE`]; tests shrink them.
#[derive(Clone, Copy)]
pub(super) struct Pcre2Limits {
    pub(super) deadline: Duration,
    pub(super) grace: Duration,
}

pub(super) const PCRE2_LIMITS: Pcre2Limits = Pcre2Limits {
    deadline: PCRE2_SEARCH_DEADLINE,
    grace: PCRE2_DEADLINE_GRACE,
};

/// Live PCRE2 worker count (including abandoned-but-still-running workers).
pub(crate) static ACTIVE_PCRE2_WORKERS: AtomicUsize = AtomicUsize::new(0);

/// Reserve a worker slot if the live count is below `max`. Returns false when
/// saturated (leaving the counter unchanged). Lock-free and pure so the bound is
/// directly unit-testable without driving a real catastrophic regex.
pub(crate) fn try_acquire_worker_slot(counter: &AtomicUsize, max: usize) -> bool {
    let mut current = counter.load(Ordering::Relaxed);
    loop {
        if current >= max {
            return false;
        }
        match counter.compare_exchange_weak(
            current,
            current + 1,
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            Ok(_) => return true,
            Err(observed) => current = observed,
        }
    }
}

pub(crate) fn release_worker_slot(counter: &AtomicUsize) {
    counter.fetch_sub(1, Ordering::AcqRel);
}

/// Releases the global PCRE2 worker slot when the worker thread exits — whether
/// it completed normally or finished after the driver stopped waiting.
pub(crate) struct Pcre2WorkerSlot;

impl Drop for Pcre2WorkerSlot {
    fn drop(&mut self) {
        release_worker_slot(&ACTIVE_PCRE2_WORKERS);
    }
}

/// Run a PCRE2 search on a worker thread bounded by `limits` (see
/// [`PCRE2_SEARCH_DEADLINE`]). The driver polls `cancelled` and the hard
/// deadline; on either it raises the shared stop flag and returns the files the
/// worker finished so far, flagged `cancelled` or `pcre2Deadline`.
pub(super) fn search_pcre2(
    opts: TextSearchOptions,
    mode: Mode,
    matcher: grep_pcre2::RegexMatcher,
    path_filter: Arc<dyn TextSearchPathFilter>,
    cancelled: &(dyn Fn() -> bool + Sync),
    limits: Pcre2Limits,
) -> Result<TextSearchResult> {
    // Bound concurrently live PCRE2 worker threads, including workers still
    // finishing an uninterruptible match after their driver returned.
    if !try_acquire_worker_slot(&ACTIVE_PCRE2_WORKERS, MAX_ACTIVE_PCRE2_WORKERS) {
        return Err(Error::new(
            "Too many concurrent PCRE2 (-P) searches are in flight (some past their wall-clock deadline are still finishing a match); retry shortly, or use regex:\"literal\"/the default engine.",
        ));
    }
    let started = Instant::now();
    let deadline = started + limits.deadline;
    let hard_deadline = deadline + limits.grace;
    let state = Arc::new(CollectState::new());
    let (tx, rx) = std::sync::mpsc::channel();
    // The worker and this driver share one copy of the options.
    let opts = Arc::new(opts);
    let opts_worker = Arc::clone(&opts);
    let state_worker = Arc::clone(&state);
    let spawned = std::thread::Builder::new()
        .name("pcre2-search".into())
        .spawn(move || {
            // Release the slot when this thread exits.
            let _slot = Pcre2WorkerSlot;
            let _ = tx.send(collect(
                &opts_worker,
                &matcher,
                mode,
                path_filter,
                &state_worker,
                WalkControl {
                    deadline: Some(deadline),
                    cancelled: &|| false,
                    eager: true,
                },
            ));
        });
    if spawned.is_err() {
        // Spawn failed: no worker will ever run to release the reserved slot.
        release_worker_slot(&ACTIVE_PCRE2_WORKERS);
    }
    spawned.map_err(to_engine_err)?;
    loop {
        if cancelled() {
            state.cancelled.store(true, Ordering::Relaxed);
            state.stop.store(true, Ordering::Relaxed);
            return Ok(build_result(&opts, mode, state.snapshot()));
        }
        let now = Instant::now();
        if now >= hard_deadline {
            state.timed_out.store(true, Ordering::Relaxed);
            state.stop.store(true, Ordering::Relaxed);
            return Ok(build_result(&opts, mode, state.snapshot()));
        }
        match rx.recv_timeout(PCRE2_DRIVER_POLL.min(hard_deadline - now)) {
            Ok(collected) => {
                collected?;
                return Ok(build_result(&opts, mode, state.snapshot()));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(Error::new("PCRE2 search worker terminated unexpectedly"));
            }
        }
    }
}
