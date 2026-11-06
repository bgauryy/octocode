//! Memory accounting for a search and its retained file outcomes.
use super::FileRec;
use crate::search::text_parser::RawMatch;
use crate::types::TextSearchMatch;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// The bytes one search's evidence may hold while it is collected. Every
/// order ranks the whole scope and every page is cut from the whole
/// collected result, so what a search keeps grows with its matches, not
/// with the page it answers. The bound is the process memory budget
/// ([`crate::memory::MEMORY`]), the one a stored scan must fit to serve its
/// continuation pages: a scan past it could not be kept for them anyway. A
/// search that reaches it stops and is refused
/// ([`crate::error::ErrorKind::ResourceExhausted`]), never cut short.
pub(in super::super) struct Held {
    used: AtomicUsize,
    limit: usize,
    exhausted: AtomicBool,
}

impl Held {
    pub(in super::super) fn new(limit: Option<usize>) -> Self {
        Self {
            used: AtomicUsize::new(0),
            limit: limit.unwrap_or_else(|| crate::memory::MEMORY.limit()),
            exhausted: AtomicBool::new(false),
        }
    }

    /// Hold `bytes` more; whether the search still fits its bound (once it
    /// does not, it never does again).
    pub(in super::super) fn charge(&self, bytes: usize) -> bool {
        let used = self
            .used
            .fetch_add(bytes, Ordering::Relaxed)
            .saturating_add(bytes);
        if used > self.limit {
            self.exhausted.store(true, Ordering::Relaxed);
        }
        !self.exhausted()
    }

    fn release(&self, bytes: usize) {
        self.used.fetch_sub(bytes, Ordering::Relaxed);
    }

    /// The search held more than its bound at some point.
    pub(in super::super) fn exhausted(&self) -> bool {
        self.exhausted.load(Ordering::Relaxed)
    }

    /// The refusal of a search that passed its bound.
    pub(in super::super) fn refusal(&self) -> crate::error::Error {
        crate::error::Error::resource_exhausted(format!(
            "the search's matches pass the process memory budget ({} bytes) before the search ends",
            self.limit
        ))
    }
}

/// What one file's search holds so far, released when its outcome is
/// dropped unkept (a re-searched prefix, a filtered file) and kept with
/// its record ([`Self::keep`]).
pub(in super::super) struct Charge {
    held: std::sync::Arc<Held>,
    bytes: usize,
}

impl Charge {
    pub(in super::super) fn new(held: &std::sync::Arc<Held>) -> Self {
        Self {
            held: std::sync::Arc::clone(held),
            bytes: 0,
        }
    }

    /// Hold `bytes` more; whether the search still fits its bound.
    pub(in super::super) fn add(&mut self, bytes: usize) -> bool {
        self.bytes = self.bytes.saturating_add(bytes);
        self.held.charge(bytes)
    }

    /// The file's record is kept: its bytes stay held for the search.
    pub(in super::super) fn keep(mut self) {
        self.bytes = 0;
    }
}

impl Drop for Charge {
    fn drop(&mut self) {
        self.held.release(self.bytes);
    }
}

/// Heap bytes one kept span or prepared row holds.
pub(in super::super) fn match_weight(matched: &TextSearchMatch) -> usize {
    std::mem::size_of::<TextSearchMatch>() + matched.value.capacity()
}

/// Heap bytes one kept matched line holds.
pub(in super::super) fn raw_weight(raw: &RawMatch) -> usize {
    std::mem::size_of::<RawMatch>() + raw.line_text.capacity()
}

/// Heap bytes one kept context line holds.
pub(in super::super) fn context_weight(line: &str) -> usize {
    std::mem::size_of::<(u32, String)>() + line.len()
}

/// Heap bytes of a kept file's record past its rows.
pub(in super::super) fn record_weight(path: &str, term_counts: &[u32]) -> usize {
    std::mem::size_of::<FileRec>() + path.len() + std::mem::size_of_val(term_counts)
}
