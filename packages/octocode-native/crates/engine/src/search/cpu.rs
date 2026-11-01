//! The process-wide CPU budget every parallel fan-out sizes itself from.
//!
//! A call that computes on this host's cores holds a [`Computing`] guard
//! while it runs. A fan-out sized while `k` calls compute takes the fair
//! share `cores / k`, at least one thread, so concurrent calls split the
//! cores instead of each sizing its pools as if it had the machine to
//! itself. Every call keeps at least one thread, so a small call never
//! waits behind a large one, and no work is cut: a smaller share runs the
//! same work on fewer threads. A share is taken when a fan-out starts; a
//! call that starts later shrinks the fan-outs started after it.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The cores one process shares among the calls computing on them.
#[derive(Debug)]
pub struct CpuBudget {
    cores: usize,
    computing: AtomicUsize,
}

/// One call computing on a [`CpuBudget`]'s cores; dropping it returns the
/// call's share to the others.
#[must_use = "the call stops counting toward the budget when the guard drops"]
#[derive(Debug)]
pub struct Computing<'a>(&'a CpuBudget);

impl Drop for Computing<'_> {
    fn drop(&mut self) {
        self.0.computing.fetch_sub(1, Ordering::AcqRel);
    }
}

impl CpuBudget {
    /// A budget of `cores` (at least one).
    #[must_use]
    pub const fn new(cores: usize) -> Self {
        Self {
            cores: if cores == 0 { 1 } else { cores },
            computing: AtomicUsize::new(0),
        }
    }

    /// Count one more call computing until the guard drops.
    pub fn enter(&self) -> Computing<'_> {
        self.computing.fetch_add(1, Ordering::AcqRel);
        Computing(self)
    }

    /// The threads one computing call's fan-out takes now: the cores
    /// divided among the calls computing (all of them when none is), at
    /// least one.
    #[must_use]
    pub fn share(&self) -> usize {
        (self.cores / self.computing.load(Ordering::Acquire).max(1)).max(1)
    }

    /// Every core of the budget.
    #[must_use]
    pub const fn cores(&self) -> usize {
        self.cores
    }
}

/// The host's cores; one when the count is unknown, so an unproven host
/// never runs more threads than it can.
#[must_use]
pub fn host_cores() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

/// This process's budget over the host's cores.
pub fn host() -> &'static CpuBudget {
    static HOST: LazyLock<CpuBudget> = LazyLock::new(|| CpuBudget::new(host_cores()));
    &HOST
}

#[cfg(test)]
mod tests {
    use super::*;

    /// k computing calls each take `cores / k`, so together they never take
    /// more than the cores (or one thread each, when there are more calls
    /// than cores); a call that ends returns its share.
    #[test]
    fn computing_calls_split_the_cores_and_never_exceed_them() {
        let budget = CpuBudget::new(12);
        assert_eq!(budget.share(), 12, "no call: a fan-out takes every core");
        for calls in 1..=30 {
            let guards = (0..calls).map(|_| budget.enter()).collect::<Vec<_>>();
            let share = budget.share();
            assert!(share >= 1);
            assert!(
                share * calls <= budget.cores().max(calls),
                "{calls} calls of {share} threads"
            );
            drop(guards);
            assert_eq!(budget.share(), 12, "every share returned");
        }
        assert_eq!(
            CpuBudget::new(0).share(),
            1,
            "an unknown host runs one thread"
        );
    }
}
