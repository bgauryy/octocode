//! How many threads one query's parallel work runs on: the one core-count
//! rule every per-row fan-out sizes itself by, so concurrent rows split the
//! cores the batch budget gave them instead of each taking every core.

use crate::num::usize_of;

/// The host's cores; one when the count is unknown, so an unproven host
/// never runs more threads than it can.
pub(crate) fn host_cores() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

/// Threads for `items` independent jobs of one query: its share of the
/// cores (`walk_threads`, set by the batch budget when several queries run
/// at once; `None` = every core), at most `cap` (a workload's own ceiling,
/// with its rationale beside it) and `items`, at least one.
pub(crate) fn workers(walk_threads: Option<u32>, items: usize, cap: Option<usize>) -> usize {
    walk_threads
        .map_or_else(host_cores, usize_of)
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

    /// A query's share of the cores bounds its fan-out, as do its cap and its
    /// items; there is always one worker.
    #[test]
    fn a_fan_out_takes_its_share_its_cap_and_its_items() {
        assert_eq!(workers(Some(2), 10, None), 2);
        assert_eq!(workers(Some(8), 10, Some(4)), 4);
        assert_eq!(workers(Some(8), 3, None), 3);
        assert_eq!(workers(Some(0), 3, None), 1);
        assert_eq!(workers(Some(3), 0, None), 1);
        assert_eq!(workers(None, usize::MAX, None), host_cores());
        assert_eq!(workers(None, usize::MAX, Some(1)), 1);
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
}
