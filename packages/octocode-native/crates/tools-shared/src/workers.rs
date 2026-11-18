//! How many threads one query's parallel work runs on: the core-count rule
//! every per-row fan-out in the runtime sizes itself by, so concurrent rows
//! and concurrent calls split the cores instead of each taking every core.
//!
//! The engine's file scans are the exception: `astSearch` matching
//! (`structural/files.rs`) and the fact scan (`fact_scan/evaluate/ordered.rs`)
//! run on rayon's one global pool, as many threads as cores. Concurrent
//! scans queue on that one pool rather than each starting its own threads, so
//! the cores are not oversubscribed, but a scan does not shrink to
//! [`cpu::host`]'s share while other rows are computing.

use octocode_engine::cpu;

/// Threads for `items` independent jobs of one query: its share of the
/// process CPU budget ([`cpu::host`]; every running local row counts toward
/// it), at most `cap` (a workload's own ceiling, with its rationale beside
/// it) and `items`, at least one.
pub fn workers(items: usize, cap: Option<usize>) -> usize {
    cpu::host()
        .share()
        .min(cap.unwrap_or(usize::MAX))
        .min(items)
        .max(1)
}

/// `job` of every item, by item index, on `workers` scoped threads that
/// each take the next undone item (the work per item varies widely). A
/// panicking job panics the caller. Returned in item order.
pub fn each_parallel<I, R: Send>(
    items: I,
    workers: usize,
    job: impl Fn(I::Item) -> R + Sync,
) -> Vec<R>
where
    I: IntoIterator,
    I::IntoIter: Send,
    I::Item: Send,
{
    let items = items.into_iter();
    if workers < 2 {
        return items.map(job).collect();
    }
    let queue = std::sync::Mutex::new(items.enumerate());
    let next = || {
        queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .next()
    };
    // Each worker runs under the caller's stop flag, so a deep-stack parse
    // a job waits on stops with the caller's work.
    let stop = octocode_engine::deep_stack::current_stop();
    let drain = || {
        let run = || {
            let mut done = Vec::new();
            while let Some((index, item)) = next() {
                done.push((index, job(item)));
            }
            done
        };
        match &stop {
            Some(stop) => octocode_engine::deep_stack::while_stoppable(stop, run),
            None => run(),
        }
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

/// [`each_parallel`] for fallible jobs: the outcomes in item order, or the
/// first error by item index among the jobs that ran. Once a job fails,
/// workers stop taking new items.
pub fn try_each_parallel<I, R: Send, E: Send>(
    items: I,
    workers: usize,
    job: impl Fn(I::Item) -> Result<R, E> + Sync,
) -> Result<Vec<R>, E>
where
    I: IntoIterator,
    I::IntoIter: Send,
    I::Item: Send,
{
    if workers < 2 {
        return items.into_iter().map(job).collect();
    }
    let failed = std::sync::atomic::AtomicBool::new(false);
    let mut items = items.into_iter();
    let pending = std::iter::from_fn(|| {
        if failed.load(std::sync::atomic::Ordering::Relaxed) {
            None
        } else {
            items.next()
        }
    });
    each_parallel(pending, workers, |item| {
        let outcome = job(item);
        if outcome.is_err() {
            failed.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        outcome
    })
    .into_iter()
    .collect()
}

#[cfg(test)]
mod tests;
