//! What shaping an item read needs beside its public pages: the first cut body
//! window of a page, and the private page size that resumes a cut comment page.
use super::axis::Axis;
use serde_json::{Map, Value};

/// The body windows of one page that one shared `offset` continues: the first
/// cut window is the page's public body entry.
#[derive(Debug, Default)]
pub(super) struct BodyWindows {
    first: Option<Value>,
}

impl BodyWindows {
    /// Notes one body's window `page`.
    pub(super) fn add(&mut self, page: &Value) {
        if page["hasMore"] == true {
            self.first.get_or_insert_with(|| page.clone());
        }
    }

    /// Puts the page's first cut window under `axis`.
    pub(super) fn attach(self, axis: Axis, pagination: &mut Map<String, Value>) {
        if let Some(first) = self.first {
            pagination.insert(axis.key().into(), first);
        }
    }
}

/// What continuing a shaped page needs beside its public entries.
#[derive(Debug, Default)]
pub(super) struct Remainder {
    /// The private `pageSize` that starts the next issue comment page at its
    /// first unshown comment.
    next_page_size: Option<usize>,
}

impl Remainder {
    /// The next comment page starts at its first unshown comment with
    /// `size` per page.
    pub(super) fn resume_page_size(&mut self, size: usize) {
        self.next_page_size = Some(size);
    }

    pub(super) fn next_page_size(&self) -> Option<usize> {
        self.next_page_size
    }
}
