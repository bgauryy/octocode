//! Bounded large-stack worker pool for recursive parsers and AST walks.
//!
//! oxc parse/visit (and the tree-sitter signature recursion) have no depth
//! guard, so they run on threads with a 64 MB native stack instead of the
//! caller's thread. Jobs go to one lazily built rayon pool (`POOL_THREADS`
//! workers) rather than a fresh thread per call, so concurrent or timed-out
//! calls can never grow the thread count past the pool size.
//!
//! Timeouts are cooperative: when the caller stops waiting, the job's cancel
//! flag is raised. Walks poll [`job_cancelled`] and stop descending, and a job
//! still queued when its caller gave up is skipped entirely. The oxc parser
//! itself is a single non-interruptible call (bounded by the minifier's input
//! size cap); everything after it checks the flag.
//!
//! Each pool thread also owns one reusable oxc [`Allocator`], handed out via
//! [`with_thread_allocator`] and `reset()` before every job.

use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use oxc_allocator::Allocator;

const STACK_SIZE: usize = 64 * 1024 * 1024;
const MIN_POOL_THREADS: usize = 2;
const MAX_POOL_THREADS: usize = 16;
/// Arena capacity retained between jobs. `reset()` keeps the current chunk;
/// after an unusually large file, drop it instead of pinning that memory on
/// an idle worker forever.
const MAX_RETAINED_ARENA_BYTES: usize = 16 * 1024 * 1024;

fn pool_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(MIN_POOL_THREADS, std::num::NonZeroUsize::get)
        .clamp(MIN_POOL_THREADS, MAX_POOL_THREADS)
}

fn pool() -> Option<&'static rayon::ThreadPool> {
    static POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(pool_threads())
            .stack_size(STACK_SIZE)
            .thread_name(|index| format!("octocode-deep-stack-{index}"))
            .build()
            .ok()
    })
    .as_ref()
}

thread_local! {
    static CURRENT_CANCEL: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
    static THREAD_ALLOCATOR: RefCell<Allocator> = RefCell::new(Allocator::default());
}

/// `true` once the caller of the running deep-stack job stopped waiting for
/// it. Walks poll this to stop early; always `false` off the pool.
pub(crate) fn job_cancelled() -> bool {
    CURRENT_CANCEL.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    })
}

/// Run `f` with this thread's reusable oxc arena, reset first. A re-entrant
/// call (arena already borrowed) falls back to a fresh arena.
pub(crate) fn with_thread_allocator<R>(f: impl FnOnce(&Allocator) -> R) -> R {
    THREAD_ALLOCATOR.with(|cell| match cell.try_borrow_mut() {
        Ok(mut allocator) => {
            allocator.reset();
            let result = f(&allocator);
            if allocator.capacity() > MAX_RETAINED_ARENA_BYTES {
                *allocator = Allocator::default();
            }
            result
        }
        Err(_) => f(&Allocator::default()),
    })
}

/// Run a job on a pool thread, installing `cancel` as its cancel flag and
/// containing any panic (a panic escaping a rayon `spawn` aborts the process).
fn run_job<T: Default>(cancel: &Arc<AtomicBool>, f: impl FnOnce() -> T) -> T {
    if cancel.load(Ordering::Relaxed) {
        return T::default();
    }
    let previous = CURRENT_CANCEL.with(|slot| slot.replace(Some(Arc::clone(cancel))));
    let result = catch_unwind(AssertUnwindSafe(f)).unwrap_or_default();
    CURRENT_CANCEL.with(|slot| *slot.borrow_mut() = previous);
    result
}

fn on_pool_thread(pool: &rayon::ThreadPool) -> bool {
    pool.current_thread_index().is_some()
}

/// Run a parser or recursive AST walk on the large-stack pool and wait for it.
/// Panics and pool-construction failure yield `T::default()`.
pub(crate) fn run_on_deep_stack<T: Send + Default + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> T {
    run_on_pool(None, f)
}

/// Like [`run_on_deep_stack`], but bounds the caller's wait by `timeout`
/// (measured from submission, so it includes queueing). On timeout the job's
/// cancel flag is raised — a queued job never starts, a running one stops at
/// its next [`job_cancelled`] check — and `T::default()` is returned, matching
/// the panic fallback.
pub(crate) fn run_on_deep_stack_with_timeout<T: Send + Default + 'static>(
    timeout: Duration,
    f: impl FnOnce() -> T + Send + 'static,
) -> T {
    run_on_pool(Some(timeout), f)
}

fn run_on_pool<T: Send + Default + 'static>(
    timeout: Option<Duration>,
    f: impl FnOnce() -> T + Send + 'static,
) -> T {
    let Some(pool) = pool() else {
        return T::default();
    };
    // Nested call from a pool job: already on a large stack, and blocking on
    // our own pool could deadlock once every worker waits. Run inline under
    // the outer job's cancel flag.
    if on_pool_thread(pool) {
        return catch_unwind(AssertUnwindSafe(f)).unwrap_or_default();
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::sync_channel::<T>(1);
    let job_cancel = Arc::clone(&cancel);
    pool.spawn(move || {
        // If the caller already timed out, the send fails harmlessly.
        let _ = tx.send(run_job(&job_cancel, f));
    });
    let received = match timeout {
        Some(timeout) => rx.recv_timeout(timeout).ok(),
        None => rx.recv().ok(),
    };
    received.unwrap_or_else(|| {
        cancel.store(true, Ordering::Relaxed);
        T::default()
    })
}

/// Run `f` as if it were a pool job whose caller already timed out.
#[cfg(test)]
pub(crate) fn run_as_cancelled_job<T: Default>(f: impl FnOnce() -> T) -> T {
    let cancel = Arc::new(AtomicBool::new(false));
    let previous = CURRENT_CANCEL.with(|slot| slot.replace(Some(Arc::clone(&cancel))));
    cancel.store(true, Ordering::Relaxed);
    let result = catch_unwind(AssertUnwindSafe(f)).unwrap_or_default();
    CURRENT_CANCEL.with(|slot| *slot.borrow_mut() = previous);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;
    use std::time::Instant;

    const SHORT: Duration = Duration::from_millis(50);

    /// Spin until the job is cancelled (or a safety cap elapses), counting
    /// iterations in `ticks`; flags `exited` on return.
    fn spin_until_cancelled(ticks: &AtomicUsize, exited: &AtomicBool) -> u32 {
        let cap = Instant::now() + Duration::from_secs(10);
        while !job_cancelled() && Instant::now() < cap {
            ticks.fetch_add(1, Ordering::Relaxed);
            std::thread::yield_now();
        }
        exited.store(true, Ordering::SeqCst);
        1
    }

    fn wait_for(flag: &AtomicBool, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            if flag.load(Ordering::SeqCst) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        flag.load(Ordering::SeqCst)
    }

    #[test]
    fn returns_the_job_result_on_a_large_stack_pool_thread() {
        let name = run_on_deep_stack(|| std::thread::current().name().map(str::to_owned));
        assert!(
            name.as_deref()
                .is_some_and(|n| n.starts_with("octocode-deep-stack-")),
            "{name:?}"
        );
        assert_eq!(run_on_deep_stack_with_timeout(SHORT * 20, || 7_u32), 7);
    }

    #[test]
    fn a_panicking_job_yields_default_and_keeps_the_pool_alive() {
        #[allow(clippy::panic)]
        let value: u32 = run_on_deep_stack(|| panic!("boom"));
        assert_eq!(value, 0);
        assert_eq!(run_on_deep_stack(|| 3_u32), 3);
    }

    #[test]
    fn timed_out_job_is_cancelled_and_actually_stops() {
        // Other tests share the pool; a job still queued when its caller
        // gives up is skipped (never ticks). Retry until one actually ran.
        for _ in 0..50 {
            let ticks = Arc::new(AtomicUsize::new(0));
            let exited = Arc::new(AtomicBool::new(false));
            let (t, e) = (Arc::clone(&ticks), Arc::clone(&exited));
            let value = run_on_deep_stack_with_timeout(SHORT, move || spin_until_cancelled(&t, &e));
            assert_eq!(value, 0, "a timed-out call returns the default");
            if !wait_for(&exited, Duration::from_secs(2)) {
                assert_eq!(
                    ticks.load(Ordering::Relaxed),
                    0,
                    "a started worker must observe cancellation and exit"
                );
                continue;
            }
            let settled = ticks.load(Ordering::Relaxed);
            std::thread::sleep(SHORT);
            assert_eq!(
                ticks.load(Ordering::Relaxed),
                settled,
                "no CPU burned after cancel"
            );
            return;
        }
        #[allow(clippy::panic)]
        {
            panic!("the job never got a pool thread");
        }
    }

    #[test]
    fn concurrent_timeouts_never_exceed_the_pool_size() {
        let threads_seen = Arc::new(Mutex::new(HashSet::new()));
        let running = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let callers = pool_threads() * 4;
        std::thread::scope(|scope| {
            for _ in 0..callers {
                let (seen, running, peak) = (
                    Arc::clone(&threads_seen),
                    Arc::clone(&running),
                    Arc::clone(&peak),
                );
                scope.spawn(move || {
                    run_on_deep_stack_with_timeout(SHORT, move || {
                        if let Ok(mut seen) = seen.lock() {
                            seen.insert(std::thread::current().id());
                        }
                        let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(now, Ordering::SeqCst);
                        let (ticks, exited) = (AtomicUsize::new(0), AtomicBool::new(false));
                        spin_until_cancelled(&ticks, &exited);
                        running.fetch_sub(1, Ordering::SeqCst);
                        1_u32
                    })
                });
            }
        });
        let distinct = threads_seen.lock().map_or(usize::MAX, |seen| seen.len());
        assert!(distinct <= pool_threads(), "{distinct} worker threads");
        assert!(peak.load(Ordering::SeqCst) <= pool_threads());
    }

    #[test]
    fn nested_deep_stack_calls_run_inline_instead_of_deadlocking() {
        let value = run_on_deep_stack(|| run_on_deep_stack(|| 5_u32) + 1);
        assert_eq!(value, 6);
    }

    #[test]
    fn thread_allocator_is_reused_and_reset_between_jobs() {
        let first = with_thread_allocator(|allocator| {
            allocator.alloc_slice_copy(&vec![0_u8; 64 * 1024]);
            (allocator as *const Allocator, allocator.capacity())
        });
        let second = with_thread_allocator(|allocator| {
            (
                allocator as *const Allocator,
                allocator.capacity(),
                allocator.used_bytes(),
            )
        });
        assert_eq!(first.0, second.0, "same arena instance");
        assert!(
            second.1 >= 64 * 1024,
            "arena chunk retained across jobs: {}",
            second.1
        );
        assert_eq!(second.2, 0, "arena reset before reuse");
        // Re-entrant use falls back to a fresh arena instead of panicking.
        let nested = with_thread_allocator(|_| with_thread_allocator(Allocator::used_bytes));
        assert_eq!(nested, 0);
    }

    #[test]
    fn oversized_arena_is_released_after_the_job() {
        with_thread_allocator(|allocator| {
            allocator.alloc_slice_copy(&vec![0_u8; MAX_RETAINED_ARENA_BYTES + 1]);
        });
        let capacity = with_thread_allocator(Allocator::capacity);
        assert!(capacity <= MAX_RETAINED_ARENA_BYTES, "{capacity}");
    }
}
