//! The binaries a walk skipped, as the page counts them.
use super::*;

impl Found<'_> {
    /// Files skipped as binary from their leading bytes, on the first page
    /// of a directory walk: outside a text search, not a coverage gap.
    pub(in crate::local_search) fn leading_binaries(&self) -> Option<u32> {
        if !self.first_page() || self.root.is_file() {
            return None;
        }
        let total: u32 = self
            .scanned
            .skipped_binary_extensions
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|group| group.count)
            .sum();
        (total > 0).then_some(total)
    }
}
