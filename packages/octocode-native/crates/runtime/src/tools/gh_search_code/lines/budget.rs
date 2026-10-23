//! The process-wide scan byte budget and the bounded fan-out of blob scans.
use super::MAX_SCAN_BYTES;
use octocode_github::{ProviderError, RequestContext};

/// Blob bytes the process scans at once. A scanned file's working set (its
/// lines, their lowercase copies and its declarations parse) runs to about
/// 35 times its bytes, so this keeps concurrent scans near 300 MB whatever
/// the core count, page size or number of concurrent calls, while small
/// files still scan on every core.
pub(super) const SCAN_BUDGET_BYTES: usize = 8 * 1024 * 1024;

/// The process's in-flight scan bytes: every row, and every call a
/// long-lived server runs at once, shares it.
pub(super) static SCANS: ScanBudget = ScanBudget::new(SCAN_BUDGET_BYTES);

/// The bytes [`scan`] holds for a blob: none for one it refuses unread.
pub(super) fn scan_weight(bytes: &[u8]) -> usize {
    if bytes.len() > MAX_SCAN_BYTES {
        0
    } else {
        bytes.len()
    }
}

/// Bytes held by running scans, at most `limit`.
pub(super) struct ScanBudget {
    pub(super) limit: usize,
    pub(super) held: std::sync::Mutex<usize>,
    pub(super) freed: std::sync::Condvar,
    #[cfg(test)]
    pub(super) waiting: std::sync::atomic::AtomicUsize,
}

impl ScanBudget {
    pub(super) const fn new(limit: usize) -> Self {
        Self {
            limit,
            held: std::sync::Mutex::new(0),
            freed: std::sync::Condvar::new(),
            #[cfg(test)]
            waiting: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Hold `bytes` (at most the whole budget: a larger item runs alone)
    /// until the guard drops, waiting while other scans hold too much.
    pub(super) fn admit(
        &self,
        bytes: usize,
        context: &RequestContext,
    ) -> Result<Admitted<'_>, ProviderError> {
        context.check()?;
        let bytes = bytes.min(self.limit);
        let mut held = self
            .held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while bytes > self.limit - *held {
            context.check()?;
            let wait = context
                .budget
                .effective_deadline()
                .saturating_duration_since(std::time::Instant::now())
                .min(std::time::Duration::from_millis(10));
            #[cfg(test)]
            self.waiting
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            held = self
                .freed
                .wait_timeout(held, wait)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
            #[cfg(test)]
            self.waiting
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
        context.check()?;
        *held += bytes;
        Ok(Admitted {
            budget: self,
            bytes,
        })
    }

    /// Workers for `items` items, the largest weighing `largest`: one per
    /// core, never more than the items, nor than the budget admits at once.
    pub(super) fn workers(&self, cores: usize, items: usize, largest: usize) -> usize {
        cores.min(items).min(self.limit / largest.max(1)).max(1)
    }
}

/// A scan's hold on its [`ScanBudget`] bytes, released on drop (a
/// panicking scan's too).
pub(super) struct Admitted<'a> {
    budget: &'a ScanBudget,
    bytes: usize,
}

impl Drop for Admitted<'_> {
    fn drop(&mut self) {
        let mut held = self
            .budget
            .held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *held -= self.bytes;
        drop(held);
        self.budget.freed.notify_all();
    }
}

/// `work` on each item, in order, each holding `weight(item)` bytes of
/// `budget` while it runs; several items run on scoped threads (at most
/// [`ScanBudget::workers`]) that each take the next item until none is
/// left. An item whose `work` panicked is `None`; the others keep their
/// results, on one thread or many.
pub(super) fn fan_out<I: Sync, T: Send>(
    budget: &ScanBudget,
    context: &RequestContext,
    cores: usize,
    items: &[I],
    weight: impl Fn(&I) -> usize + Sync,
    work: impl Fn(&I) -> T + Sync,
) -> Result<Vec<Option<T>>, ProviderError> {
    context.check()?;
    let run = |item: &I| {
        context.check()?;
        let _held = budget.admit(weight(item), context)?;
        context.check()?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(item))).ok();
        context.check()?;
        Ok(result)
    };
    if items.len() < 2 {
        return items.iter().map(run).collect();
    }
    let largest = items.iter().map(&weight).max().unwrap_or(0);
    let workers = budget.workers(cores, items.len(), largest);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut out: Vec<Option<T>> = items.iter().map(|_| None).collect();
    std::thread::scope(|scope| {
        let tasks: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        context.check()?;
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(item) = items.get(index) else {
                            return Ok::<_, ProviderError>(done);
                        };
                        done.push((index, run(item)?));
                    }
                })
            })
            .collect();
        for task in tasks {
            let done = match task.join() {
                Ok(done) => done?,
                Err(panic) => std::panic::resume_unwind(panic),
            };
            for (index, value) in done {
                out[index] = value;
            }
        }
        context.check()?;
        Ok(out)
    })
}
