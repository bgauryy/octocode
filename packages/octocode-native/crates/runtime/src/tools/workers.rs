//! How many threads one query's parallel work runs on: the one core-count
//! rule every per-row fan-out sizes itself by, so concurrent rows and
//! concurrent calls split the cores instead of each taking every core.

use octocode_engine::search::cpu;

/// Threads for `items` independent jobs of one query: its share of the
/// process CPU budget ([`cpu::host`]; every running local row counts toward
/// it), at most `cap` (a workload's own ceiling, with its rationale beside
/// it) and `items`, at least one.
pub(crate) fn workers(items: usize, cap: Option<usize>) -> usize {
    cpu::host()
        .share()
        .min(cap.unwrap_or(usize::MAX))
        .min(items)
        .max(1)
}

/// `job` of every item, by item index, on `workers` scoped threads that
/// each take the next undone item (the work per item varies widely). A
/// panicking job panics the caller. Returned in item order.
pub(crate) fn each_parallel<T: Send, R: Send>(
    items: &mut [T],
    workers: usize,
    job: impl Fn(&mut T) -> R + Sync,
) -> Vec<R> {
    if workers < 2 {
        return items.iter_mut().map(job).collect();
    }
    let queue = std::sync::Mutex::new(items.iter_mut().enumerate());
    let next = || {
        queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .next()
    };
    let drain = || {
        let mut done = Vec::new();
        while let Some((index, item)) = next() {
            done.push((index, job(item)));
        }
        done
    };
    let mut done = std::thread::scope(|scope| {
        let tasks = (0..workers).map(|_| scope.spawn(drain)).collect::<Vec<_>>();
        tasks
            .into_iter()
            .flat_map(|task| {
                task.join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect::<Vec<_>>()
    });
    done.sort_unstable_by_key(|(index, _)| *index);
    done.into_iter().map(|(_, outcome)| outcome).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A query's share of the process budget bounds its fan-out, as do its
    /// cap and its items; there is always one worker.
    #[test]
    fn a_fan_out_takes_its_share_its_cap_and_its_items() {
        let share = workers(usize::MAX, None);
        assert!((1..=cpu::host_cores()).contains(&share));
        assert_eq!(workers(usize::MAX, Some(1)), 1);
        assert_eq!(workers(1, None), 1);
        assert_eq!(workers(0, None), 1);
    }

    /// Every item is done once, on any worker count, and the outcomes come
    /// back in item order.
    #[test]
    fn each_parallel_does_every_item_once_in_order() {
        for threads in [1, 2, 7] {
            let mut items = (0..50).collect::<Vec<u32>>();
            let out = each_parallel(&mut items, threads, |item| {
                *item += 100;
                *item * 2
            });
            assert_eq!(out, (100..150).map(|n| n * 2).collect::<Vec<_>>());
            assert_eq!(items, (100..150).collect::<Vec<_>>());
        }
    }

    /// Concurrent calls' fan-outs draw on one process budget: while six
    /// calls compute, their workers together never run on more threads
    /// than the cores (one each when calls outnumber cores), however many
    /// items each fan-out has. Sized as if each had the machine, six fan-outs
    /// of 64 items would run up to six times the cores at once.
    #[test]
    fn concurrent_calls_fan_out_within_the_cores() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        const CALLS: usize = 6;
        let (active, peak) = (AtomicUsize::new(0), AtomicUsize::new(0));
        let entered = std::sync::Barrier::new(CALLS);
        let done = std::thread::scope(|scope| {
            let calls = (0..CALLS).map(|_| {
                scope.spawn(|| {
                    let _computing = cpu::host().enter();
                    entered.wait();
                    let mut items = vec![0_u8; 64];
                    let threads = workers(items.len(), None);
                    each_parallel(&mut items, threads, |_| {
                        let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(now, Ordering::SeqCst);
                        std::thread::sleep(std::time::Duration::from_millis(1));
                        active.fetch_sub(1, Ordering::SeqCst);
                    })
                    .len()
                })
            });
            calls
                .collect::<Vec<_>>()
                .into_iter()
                .map(|call| call.join().expect("call"))
                .sum::<usize>()
        });
        assert_eq!(done, CALLS * 64, "every item of every call ran");
        let peak = peak.load(Ordering::SeqCst);
        assert!(
            peak <= cpu::host_cores().max(CALLS),
            "{peak} threads on {} cores",
            cpu::host_cores()
        );
    }
}
