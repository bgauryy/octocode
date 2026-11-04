//! The bytes every in-process store holds together.
use octocode_io::memo::MemoryBudget;

/// The bytes every in-process store holds together: each tool's page memo, the
/// memory tier of every disk store, and what the LSP discovery keeps. Past it
/// the least recently used entries of all of them go first, and a value larger
/// than all of it is not kept.
pub static MEMORY: MemoryBudget = MemoryBudget::new(memory_budget);

/// The budget's floor: what a host whose memory cannot be read, or a small
/// one, still gets, so one large scan's continuation stays retained.
pub const MIN_MEMORY_BUDGET: usize = 512 * 1024 * 1024;
/// The budget's ceiling: retained continuation state is a convenience, so a
/// large host keeps most of its memory for language servers and its users.
pub const MAX_MEMORY_BUDGET: usize = 2 * 1024 * 1024 * 1024;

fn memory_budget() -> usize {
    memory_budget_for(crate::host::memory_mb())
}

/// An eighth of `host_mb` MiB, within [`MIN_MEMORY_BUDGET`] and
/// [`MAX_MEMORY_BUDGET`]; the floor when the host's memory is unknown.
#[must_use]
pub fn memory_budget_for(host_mb: Option<u64>) -> usize {
    host_mb.map_or(MIN_MEMORY_BUDGET, |mb| {
        usize::try_from(mb.saturating_mul(1024 * 1024) / 8)
            .unwrap_or(usize::MAX)
            .clamp(MIN_MEMORY_BUDGET, MAX_MEMORY_BUDGET)
    })
}
