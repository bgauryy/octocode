//! PCRE2 (`-P`) searches: the shared matcher settings, the live-worker bound, and the driver that
//! runs one window of a search on a worker thread.

use super::mode::Mode;
use super::ordered::{Plan, Run, collect_ordered};
use super::window::{SearchWindow, Windowed};
use super::{TextSearchPathFilter, to_engine_err};
use crate::error::{Error, Result};
use crate::types::TextSearchOptions;
use grep_pcre2::RegexMatcherBuilder;
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
pub(crate) fn pcre2_builder() -> RegexMatcherBuilder {
    let mut builder = RegexMatcherBuilder::new();
    builder
        .utf(true)
        .ucp(true)
        .jit_if_available(true)
        .max_jit_stack_size(Some(PCRE2_MAX_JIT_STACK_BYTES));
    builder
}

/// How often the PCRE2 driver wakes to poll caller cancellation and the
/// window's end while it waits for the worker.
const PCRE2_DRIVER_POLL: Duration = Duration::from_millis(25);

/// Maximum number of PCRE2 (`-P`) search worker threads alive at once,
/// including workers still finishing an uninterruptible match after the driver
/// stopped waiting. Once saturated, a new `-P` search is rejected rather than
/// spawning another thread (each may hold a 1 MiB JIT stack — see
/// [`PCRE2_MAX_JIT_STACK_BYTES`]).
pub(crate) const MAX_ACTIVE_PCRE2_WORKERS: usize = 8;

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

/// Run one window of a PCRE2 search on a worker thread ([`super::window`]).
/// Nothing bounds the time of one PCRE2 match, and a thread cannot be killed,
/// so the driver never waits on the worker: it polls `cancelled` and the
/// window's end. Past the end it pauses the search after the last file
/// finished in walk order, as soon as this window finished one; a worker
/// inside one uninterruptible match keeps its slot until that match returns,
/// then sees the stop flag and exits without searching further files.
pub(super) fn search_pcre2(
    opts: TextSearchOptions,
    mode: Mode,
    (matcher, terms): (grep_pcre2::RegexMatcher, Vec<grep_pcre2::RegexMatcher>),
    path_filter: Arc<dyn TextSearchPathFilter>,
    cancelled: &(dyn Fn() -> bool + Sync),
    window: SearchWindow,
) -> Result<Windowed> {
    // Bound concurrently live PCRE2 worker threads, including workers still
    // finishing an uninterruptible match after their driver returned.
    if !try_acquire_worker_slot(&ACTIVE_PCRE2_WORKERS, MAX_ACTIVE_PCRE2_WORKERS) {
        return Err(Error::new(
            "Too many concurrent PCRE2 (-P) searches are in flight (some paused windows are still finishing a match); retry shortly, or use regex:\"literal\"/the default engine.",
        ));
    }
    let SearchWindow {
        ends,
        clock,
        resume,
    } = window;
    let plan = Plan {
        ends,
        clock: clock.unwrap_or_else(|| Arc::new(Instant::now)),
        resume,
    };
    let run = Arc::new(Run::new(opts.max_held_bytes));
    let (tx, rx) = std::sync::mpsc::channel();
    // The worker and this driver share one copy of the options.
    let opts = Arc::new(opts);
    let opts_worker = Arc::clone(&opts);
    let run_worker = Arc::clone(&run);
    let spawned = std::thread::Builder::new()
        .name("pcre2-search".into())
        .spawn(move || {
            // Release the slot when this thread exits.
            let _slot = Pcre2WorkerSlot;
            let _ = tx.send(collect_ordered(
                &opts_worker,
                (&matcher, &terms),
                mode,
                path_filter,
                &run_worker,
                plan,
            ));
        });
    if spawned.is_err() {
        // Spawn failed: no worker will ever run to release the reserved slot.
        release_worker_slot(&ACTIVE_PCRE2_WORKERS);
    }
    spawned.map_err(to_engine_err)?;
    // A search past its held bound is refused, whichever way it ended.
    let refused = || run.held.exhausted().then(|| run.held.refusal());
    loop {
        if cancelled() {
            run.cancel();
            if let Some(refusal) = refused() {
                return Err(refusal);
            }
            return Ok(Windowed::Whole(run.whole(&opts, mode)));
        }
        if let Some(refusal) = refused() {
            return Err(refusal);
        }
        if let Some(pause) = run.paused() {
            return Ok(Windowed::Paused(pause));
        }
        let now = Instant::now();
        if ends.is_some_and(|ends| now >= ends) {
            run.pause_now();
        }
        let wait = ends
            .map(|ends| ends.saturating_duration_since(now))
            .filter(|left| !left.is_zero())
            .map_or(PCRE2_DRIVER_POLL, |left| left.min(PCRE2_DRIVER_POLL));
        match rx.recv_timeout(wait) {
            Ok(collected) => {
                collected?;
                if let Some(refusal) = refused() {
                    return Err(refusal);
                }
                return Ok(match run.paused() {
                    Some(pause) => Windowed::Paused(pause),
                    None => Windowed::Whole(run.whole(&opts, mode)),
                });
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(Error::new("PCRE2 search worker terminated unexpectedly"));
            }
        }
    }
}
