//! The bytes every in-process store holds together.
use octocode_io::memo::MemoryBudget;
use std::time::Duration;

/// The bytes every in-process store holds together: each tool's page memo, the
/// memory tier of every disk store, and what the LSP discovery keeps. Past it
/// the least recently used entries of all of them go first, and a value larger
/// than all of it is not kept.
pub static MEMORY: MemoryBudget = MemoryBudget::new(memory_budget);

/// How long a kept entry stays once nothing reads it, for a memo whose value
/// can be worked out again: a read restarts it.
pub const IDLE: Duration = Duration::from_secs(600);

/// The budget when the host's memory cannot be read. A known host always
/// uses its proportional share, even when that is smaller than this default.
pub const DEFAULT_MEMORY_BUDGET: usize = 512 * 1024 * 1024;
/// The budget's ceiling: retained continuation state is a convenience, so a
/// large host keeps most of its memory for language servers and its users.
pub const MAX_MEMORY_BUDGET: usize = 2 * 1024 * 1024 * 1024;

fn memory_budget() -> usize {
    memory_budget_for(crate::host::memory_mb())
}

/// An eighth of `host_mb` MiB, at most [`MAX_MEMORY_BUDGET`];
/// [`DEFAULT_MEMORY_BUDGET`] when the host's memory is unknown.
#[must_use]
pub fn memory_budget_for(host_mb: Option<u64>) -> usize {
    host_mb.map_or(DEFAULT_MEMORY_BUDGET, |mb| {
        usize::try_from(mb.saturating_mul(1024 * 1024) / 8)
            .unwrap_or(usize::MAX)
            .min(MAX_MEMORY_BUDGET)
    })
}
