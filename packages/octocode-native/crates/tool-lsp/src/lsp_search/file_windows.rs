//! Digest-pinned windows over one sorted candidate list: importer
//! candidates (`importerPage`) and implementation candidates
//! (`candidatePage`) page the same way. Window `n` is the `n`th run of
//! `size` items; a continuation that enters a later window carries the
//! list's digest as its `snapshot`, so a changed list restarts the walk
//! instead of skipping or repeating items. Each owner computes its own
//! digest (its snapshot salt).

use octocode_engine::num::saturating_u32;
use std::ops::Range;

#[derive(Debug)]
pub(super) struct FileWindows<T> {
    items: Vec<T>,
    size: usize,
    digest: String,
}

impl<T> FileWindows<T> {
    /// Windows of `size` over `items` (sorted by the owner), pinned to `digest`.
    pub(super) fn new(items: Vec<T>, size: usize, digest: String) -> Self {
        Self {
            items,
            size: size.max(1),
            digest,
        }
    }

    pub(super) fn digest(&self) -> &str {
        &self.digest
    }

    /// Items in the whole list.
    pub(super) fn len(&self) -> usize {
        self.items.len()
    }

    pub(super) fn items(&self) -> &[T] {
        &self.items
    }

    /// Window count; an empty list has one (empty) window.
    pub(super) fn pages(&self) -> u32 {
        saturating_u32(octocode_runtime_base::response::page_windows::count_pages(
            self.items.len(),
            self.size,
        ))
    }

    /// Item indexes of one-based window `page`.
    pub(super) fn range(&self, page: u32) -> Range<usize> {
        let start = (page.max(1) as usize - 1).saturating_mul(self.size);
        start.min(self.len())..start.saturating_add(self.size).min(self.len())
    }

    /// The window that holds item `index`, one-based.
    pub(super) fn page_of(&self, index: usize) -> u32 {
        saturating_u32(index / self.size + 1)
    }

    /// Whether one-based `page` is a window of this list and `expected` (the
    /// snapshot of a continuation entering a later window) names this list.
    pub(super) fn admits(&self, page: u32, expected: Option<&str>) -> bool {
        page.max(1) <= self.pages() && expected.is_none_or(|expected| expected == self.digest)
    }

    /// Whether a window follows `page`.
    pub(super) fn more_after(&self, page: u32) -> bool {
        page < self.pages()
    }

    /// The items of window `page`, owned.
    pub(super) fn into_window(self, page: u32) -> Vec<T> {
        let range = self.range(page);
        self.items
            .into_iter()
            .skip(range.start)
            .take(range.len())
            .collect()
    }
}

#[cfg(test)]
mod tests;
