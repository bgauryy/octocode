//! Full-line reads for match windows and the binaries a walk skipped.
use super::*;
use octocode_runtime_base::contracts::batch_limit;
use octocode_runtime_base::id::{MAX_INPUT_ARRAY_ITEMS, ToolId, numbered};
use std::collections::BTreeSet;

/// Read only lines clipped to match windows. Several windows on the same
/// line share one range, and ranges in one file share a query.
pub(in crate::local_search) fn add_window_reads(row: &mut LocalSearchResult) {
    let mut queries = Vec::new();
    for file in &row.files {
        let lines = file
            .matches
            .iter()
            .flatten()
            .filter_map(|hit| hit.window_start.map(|_| hit.line))
            .collect::<BTreeSet<_>>();
        let ranges = lines
            .into_iter()
            .map(|line| json!({"startLine": line, "endLine": line}))
            .collect::<Vec<_>>();
        queries.extend(
            ranges
                .chunks(MAX_INPUT_ARRAY_ITEMS)
                .map(|ranges| json!({"path": file.path, "ranges": ranges})),
        );
    }
    if queries.is_empty() {
        return;
    }
    let tool = ToolId::LocalFetch;
    let limit = batch_limit(tool).unwrap_or(queries.len());
    let calls = row.next.get_or_insert_with(|| json!({}));
    for (index, batch) in queries.chunks(limit).enumerate() {
        calls[numbered("readWindow", index)] =
            Continuation::related_input(tool, json!({"queries": batch}))
                .required()
                .build();
    }
}

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
