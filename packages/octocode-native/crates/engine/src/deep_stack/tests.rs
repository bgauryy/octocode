use super::*;
use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::time::Instant;

const SHORT: Duration = Duration::from_millis(50);

/// Run `f` as if it were a pool job whose caller already stopped.
pub(crate) fn run_as_cancelled_job<T: Default>(f: impl FnOnce() -> T) -> T {
    let cancel = Arc::new(AtomicBool::new(false));
    let previous = CURRENT_CANCEL.with(|slot| slot.replace(Some(Arc::clone(&cancel))));
    cancel.store(true, Ordering::Relaxed);
    let result = catch_unwind(AssertUnwindSafe(f)).unwrap_or_default();
    CURRENT_CANCEL.with(|slot| *slot.borrow_mut() = previous);
    result
}

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
    assert_eq!(run_on_deep_stack(|| 7_u32), 7);
}

/// A stopped caller: its flag rises after `after`.
fn stopped_after(after: Duration) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let raise = Arc::clone(&flag);
    std::thread::spawn(move || {
        std::thread::sleep(after);
        raise.store(true, Ordering::SeqCst);
    });
    flag
}

/// No clock ends a job: one that outlasts the former 2 s per-file cut
/// still returns its result, so a slow machine reads the same facts.
#[test]
fn a_long_job_returns_its_result_without_a_clock() {
    let value = run_on_deep_stack(|| {
        std::thread::sleep(Duration::from_millis(2_100));
        9_u32
    });
    assert_eq!(value, 9);
}

#[test]
fn a_panicking_job_yields_default_and_keeps_the_pool_alive() {
    let value: u32 = run_on_deep_stack(|| panic!("boom"));
    assert_eq!(value, 0);
    assert_eq!(run_on_deep_stack(|| 3_u32), 3);
}

#[test]
fn a_panicking_stop_scope_restores_its_parent() {
    let outer = Arc::new(AtomicBool::new(false));
    let inner = Arc::new(AtomicBool::new(true));
    assert!(current_stop().is_none());
    while_stoppable(&outer, || {
        let result = catch_unwind(AssertUnwindSafe(|| {
            while_stoppable(&inner, || panic!("stopped work panicked"));
        }));
        assert!(result.is_err());
        let restored = current_stop().expect("outer scope remains installed");
        assert!(Arc::ptr_eq(&restored, &outer));
        assert!(!caller_stopped());
    });
    assert!(current_stop().is_none());
}

#[test]
fn already_stopped_work_never_starts_a_deep_stack_job() {
    let stopped = Arc::new(AtomicBool::new(true));
    let started = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&started);
    let value = while_stoppable(&stopped, || {
        run_on_deep_stack(move || {
            observed.store(true, Ordering::SeqCst);
            7_u32
        })
    });
    assert_eq!(value, 0);
    assert!(!started.load(Ordering::SeqCst));
}

#[test]
fn a_cancelled_job_never_starts_nested_work() {
    let started = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&started);
    let value = run_on_deep_stack(move || {
        run_as_cancelled_job(|| {
            run_on_deep_stack(move || {
                observed.store(true, Ordering::SeqCst);
                7_u32
            })
        })
    });
    assert_eq!(value, 0);
    assert!(!started.load(Ordering::SeqCst));
}

#[test]
fn a_job_of_stopped_work_is_cancelled_and_actually_stops() {
    // Other tests share the pool; a job still queued when its caller
    // gives up is skipped (never ticks). Retry until one actually ran.
    for _ in 0..50 {
        let ticks = Arc::new(AtomicUsize::new(0));
        let exited = Arc::new(AtomicBool::new(false));
        let (t, e) = (Arc::clone(&ticks), Arc::clone(&exited));
        let stopped = stopped_after(SHORT);
        let value = while_stoppable(&stopped, || {
            run_on_deep_stack(move || spin_until_cancelled(&t, &e))
        });
        assert_eq!(value, 0, "a stopped call returns the default");
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
    panic!("the job never got a pool thread");
}

#[test]
fn concurrent_stopped_jobs_never_exceed_the_pool_size() {
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
                let stopped = stopped_after(SHORT);
                while_stoppable(&stopped, || {
                    run_on_deep_stack(move || {
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

#[test]
fn oversized_arena_is_released_when_the_job_panics() {
    let result = catch_unwind(AssertUnwindSafe(|| {
        with_thread_allocator(|allocator| {
            allocator.alloc_slice_copy(&vec![0_u8; MAX_RETAINED_ARENA_BYTES + 1]);
            panic!("parse failed after allocating");
        });
    }));
    assert!(result.is_err());
    let capacity = THREAD_ALLOCATOR.with(|cell| cell.borrow().capacity());
    assert!(capacity <= MAX_RETAINED_ARENA_BYTES, "{capacity}");
}
