//! The one rule for continuing an astSearch scan that stopped before its
//! scope ended: every cut continues, after the files it evaluated
//! (`scanOffset`, a private position with no upper bound).

/// Where a cut scan resumes: the `scanOffset` of its continuation.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ScanWindow {
    pub(super) scan_offset: u32,
}

/// The `partialReasons` entry of a scan cut by its time or memory budget.
pub(super) const CUT_REASON: crate::response::paging::PartialReason =
    crate::response::paging::PartialReason::ScanBudget;

/// Where a cut scan continues, `None` only when the scan was whole: a scan
/// cut at its time or memory budget resumes where it stopped.
pub(super) fn expand_scan(truncated: bool, resume: Option<u32>) -> Option<ScanWindow> {
    if !truncated {
        return None;
    }
    resume.map(|scan_offset| ScanWindow { scan_offset })
}

/// A page whose `next.expandScan` reads more of the same listing: its
/// `pagination` never says the listing ended, and counts only what this
/// window loaded.
pub(super) fn mark_resumed(out: &mut serde_json::Value) {
    if let Some(pagination) = out
        .get_mut("pagination")
        .and_then(serde_json::Value::as_object_mut)
    {
        pagination.insert("hasMore".into(), serde_json::Value::Bool(true));
        pagination.insert("countScope".into(), "loaded".into());
    }
}

/// The engine's file window of a scan resuming at `scan_offset`: none (the
/// time budget alone cuts it). A test narrows it ([`test_window`]) to
/// produce the cuts a large tree produces.
pub(super) fn window_files(scan_offset: u32) -> Option<u32> {
    #[cfg(test)]
    if let Some(files) = TEST_WINDOW.with(std::cell::Cell::get) {
        return Some(scan_offset.saturating_add(files));
    }
    let _ = scan_offset;
    None
}

/// The bytes of rows one scan window holds: half the process's memory budget
/// ([`octocode_engine::memory::MEMORY`]), so the window ends with a result the
/// budget can keep for its pages, however far past the limit its last file took
/// it. Past it the window ends and `next.expandScan` reads on.
pub(super) fn memory_limit() -> usize {
    #[cfg(test)]
    if let Some(limit) = TEST_MEMORY.with(std::cell::Cell::get) {
        return limit;
    }
    octocode_engine::memory::MEMORY.limit() / 2
}

#[cfg(test)]
thread_local! {
    static TEST_WINDOW: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
    static TEST_MEMORY: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// Scans on this thread hold at most `bytes` of rows until the guard drops: the
/// stop a large tree meets at the real budget, with no large tree.
#[cfg(test)]
pub(crate) fn test_memory_limit(bytes: usize) -> impl Drop {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_MEMORY.with(|limit| limit.set(None));
        }
    }
    TEST_MEMORY.with(|limit| limit.set(Some(bytes)));
    Reset
}

/// Scans on this thread evaluate `files` files per window until the guard
/// drops: a deterministic stand-in for a time-budget cut.
#[cfg(test)]
pub(crate) fn test_window(files: u32) -> impl Drop {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_WINDOW.with(|window| window.set(None));
        }
    }
    TEST_WINDOW.with(|window| window.set(Some(files)));
    Reset
}

#[cfg(test)]
mod edits;
#[cfg(test)]
mod tests;
