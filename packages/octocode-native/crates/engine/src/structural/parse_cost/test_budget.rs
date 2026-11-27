//! A parse budget for the scans a test starts on its own thread, so a test
//! that sizes the budget in kilobytes changes no other test's scans. A scan
//! reads the budget when it starts ([`super::parse_budget`]), before its
//! workers run.

use std::cell::Cell;

thread_local! {
    static BUDGET: Cell<Option<usize>> = const { Cell::new(None) };
}

pub(crate) fn get() -> Option<usize> {
    BUDGET.with(Cell::get)
}

/// A budget this thread's scans use until it drops.
#[must_use = "the budget lasts only while the guard lives"]
pub(crate) struct Budget(());

impl Drop for Budget {
    fn drop(&mut self) {
        BUDGET.with(|budget| budget.set(None));
    }
}

/// `bytes` is the parse budget of the scans this thread starts until the
/// returned guard drops.
pub(crate) fn hold(bytes: usize) -> Budget {
    BUDGET.with(|budget| budget.set(Some(bytes)));
    Budget(())
}
