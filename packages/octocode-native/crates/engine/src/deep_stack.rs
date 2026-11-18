//! Bounded large-stack worker pool for recursive parsers and AST walks.
//!
//! oxc parse/visit (and the tree-sitter signature recursion) have no depth
//! guard, so they run on threads with a 64 MB native stack instead of the
//! caller's thread. A file nested deeper than that stack holds never reaches
//! oxc (`signatures::js_oxc::nesting`): an overflow aborts the process. Jobs go to one lazily built rayon pool (`POOL_THREADS`
//! workers) rather than a fresh thread per call, so concurrent or cancelled
//! calls can never grow the thread count past the pool size.
//!
//! A job runs to completion unless its caller's work is cancelled: a caller
//! running under [`while_stoppable`] whose flag is raised stops waiting and
//! raises the job's cancel flag. No clock ends a job, so what a job returns
//! never depends on machine load. Walks poll [`job_cancelled`] and stop
//! descending, and a job still queued when its caller stopped is skipped. The
//! oxc parser itself is a single non-interruptible call (bounded by the input
//! size cap); everything after it checks the flag.
//!
//! Each pool thread also owns one reusable oxc [`Allocator`], handed out via
//! [`with_thread_allocator`] and `reset()` before every job. Between jobs they
//! hold at most [`MAX_POOL_THREADS`] x [`MAX_RETAINED_ARENA_BYTES`], whatever the
//! inputs were, so they draw on no memory budget: an arena in use belongs to the
//! thread running on it, and no other thread could release it.

use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use oxc_allocator::Allocator;

pub(crate) const STACK_SIZE: usize = 64 * 1024 * 1024;
const MIN_POOL_THREADS: usize = 2;
const MAX_POOL_THREADS: usize = 16;
/// Arena capacity retained between jobs. `reset()` keeps the current chunk;
/// after an unusually large file, drop it instead of pinning that memory on
/// an idle worker forever.
const MAX_RETAINED_ARENA_BYTES: usize = 16 * 1024 * 1024;
/// How often a waiting caller looks at its stop flag: the latency of a
/// cancellation, not a limit on the job.
const STOP_POLL: Duration = Duration::from_millis(20);

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
    /// The stop flag of the work this thread runs, set by [`while_stoppable`].
    static CALLER_STOP: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
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
            let retained = RetainedArena(&mut allocator);
            f(retained.0)
        }
        Err(_) => f(&Allocator::default()),
    })
}

/// A recovered parser panic must release oversized scratch space too.
struct RetainedArena<'a>(&'a mut Allocator);

impl Drop for RetainedArena<'_> {
    fn drop(&mut self) {
        if self.0.capacity() > MAX_RETAINED_ARENA_BYTES {
            *self.0 = Allocator::default();
        }
    }
}

struct CallerStopScope(Option<Arc<AtomicBool>>);

impl Drop for CallerStopScope {
    fn drop(&mut self) {
        CALLER_STOP.with(|slot| *slot.borrow_mut() = self.0.take());
    }
}

/// Run `f` as stoppable work: a deep-stack job it waits on is cancelled once
/// `stopped` is raised (the caller's own cancellation), never by a clock.
/// A request runs its work under its stop flag, and work it hands to another
/// thread runs there under the same flag ([`current_stop`]).
pub fn while_stoppable<R>(stopped: &Arc<AtomicBool>, f: impl FnOnce() -> R) -> R {
    let _scope = CallerStopScope(CALLER_STOP.with(|slot| slot.replace(Some(Arc::clone(stopped)))));
    f()
}

/// The stop flag of the work this thread runs ([`while_stoppable`]), for
/// the threads it hands work to.
#[must_use]
pub fn current_stop() -> Option<Arc<AtomicBool>> {
    CALLER_STOP.with(|slot| slot.borrow().clone())
}

/// `true` once the work this thread runs ([`while_stoppable`]) was stopped:
/// what it would do next is not wanted, a fallback included.
pub(crate) fn caller_stopped() -> bool {
    CALLER_STOP.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
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

/// Run a parser or recursive AST walk on the large-stack pool and wait for it
/// until it finishes or the caller's work is stopped ([`while_stoppable`]).
/// Panics, pool-construction failure and a stopped caller yield `T::default()`.
pub(crate) fn run_on_deep_stack<T: Send + Default + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> T {
    if caller_stopped() || job_cancelled() {
        return T::default();
    }
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
    // A panic the job contains is recorded where the caller contains its own.
    let scope = crate::contain::current();
    pool.spawn(move || {
        // If the caller already stopped, the send fails harmlessly.
        let _ = tx.send(crate::contain::adopt(scope, || run_job(&job_cancel, f)));
    });
    loop {
        match rx.recv_timeout(STOP_POLL) {
            Ok(value) => return value,
            Err(RecvTimeoutError::Timeout) if !caller_stopped() => {}
            Err(_) => {
                cancel.store(true, Ordering::Relaxed);
                return T::default();
            }
        }
    }
}

/// Run `f`, which borrows from its caller, on the large-stack pool and wait
/// for it: work whose recursion follows its input (structural pattern
/// compile and match) and must not take the caller's stack. The caller holds
/// the borrows, so it always waits for `f` rather than abandoning it like a
/// [`run_on_deep_stack`] job: `f` bounds its own work (a deadline). A panic
/// in `f` is recorded in the caller's containment scope and resumes on the
/// caller. A call from a pool thread runs inline, on the large stack it is
/// on. `None` when no pool could be built: nothing ran.
pub(crate) fn run_borrowing_on_deep_stack<T: Send>(f: impl FnOnce() -> T + Send) -> Option<T> {
    let pool = pool()?;
    Some(if on_pool_thread(pool) {
        f()
    } else {
        let scope = crate::contain::current();
        pool.install(move || crate::contain::adopt(scope, f))
    })
}

#[cfg(test)]
pub(crate) mod tests;
