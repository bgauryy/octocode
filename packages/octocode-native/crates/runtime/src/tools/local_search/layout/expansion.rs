//! `expandValues`: the reads that return a page's clipped values whole,
//! and what each read costs a page.

use super::super::handoff::{
    READ_TO_END_LINE, hit_windows, is_long_line_hit, long_line_read, passes_line_page,
};
use super::super::types::SearchFile;
use super::costs::PageCosts;
use crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS;
use crate::id::ToolId;
use crate::policy::path::PathPolicy;
use crate::response::result::Continuation;
use octocode_contract_types::LocalSearchQuery;
use octocode_engine::types::SearchContext;
use serde_json::{Value, json};

pub(super) fn expansion_read_chars(_query: &LocalSearchQuery, _multiline: bool) -> usize {
    octocode_engine::json_size::utf16_units(&json!({"expandValues": Continuation::continued(ToolId::LocalFetch,json!({"path":"","ranges":[]})).build()})).unwrap_or(0)
}

pub(crate) fn expand_values(
    paths: &PathPolicy,
    root: &std::path::Path,
    files: &[SearchFile],
    context: SearchContext,
    multiline: bool,
) -> Vec<Value> {
    let mut reads = Vec::new();
    for file in files {
        let clipped = file
            .matches
            .iter()
            .flatten()
            .filter(|matched| matched.clipped)
            .collect::<Vec<_>>();
        if clipped.is_empty() {
            continue;
        }
        let source = root.join(&file.path);
        let path = paths
            .workspace_relative(&source)
            .unwrap_or_else(|| source.to_string_lossy().into_owned());
        if multiline {
            let first = clipped
                .iter()
                .map(|matched| matched.line)
                .min()
                .unwrap_or(1)
                .saturating_sub(context.before)
                .max(1);
            reads.push(
                Continuation::continued(
                    ToolId::LocalFetch,
                    json!({"path":path,"ranges":[{"startLine":first,"endLine":READ_TO_END_LINE}]}),
                )
                .build(),
            );
            continue;
        }
        let (long, plain): (Vec<_>, Vec<_>) = clipped
            .into_iter()
            .partition(|matched| is_long_line_hit(matched));
        reads.extend(
            long.into_iter()
                .map(|matched| long_line_read(&path, matched)),
        );
        for chunk in
            hit_windows(plain.iter().map(|matched| matched.line), context).chunks(RANGES_MAX_ITEMS)
        {
            let ranges = chunk
                .iter()
                .map(|(start, end)| json!({"startLine":start,"endLine":end}))
                .collect::<Vec<_>>();
            reads.push(
                Continuation::continued(ToolId::LocalFetch, json!({"path":path,"ranges":ranges}))
                    .build(),
            );
        }
    }
    reads
}

/// The `expandValues` rows a page's rows of `file` need when their values
/// are read per file (see [`expand_values`]): one per long-line hit and one
/// per [`RANGES_MAX_ITEMS`] merged hit windows; a multiline file needs one.
pub(super) fn expansion_rows<'a>(
    rows: impl IntoIterator<Item = &'a octocode_engine::types::TextSearchMatch>,
    costs: &PageCosts,
) -> usize {
    let mut reads = FileReads::default();
    for matched in rows {
        reads.add(costs.read(matched));
    }
    reads.rows()
}

/// What reading one row's value whole takes ([`expand_values`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowRead {
    /// The value is whole.
    None,
    /// A clipped multiline value: its file is read once, however many.
    Multiline,
    /// A clipped value on a line past one read page: its own byte read.
    Long,
    /// A clipped line value: its hit lines with this context on each side.
    Window { start: u32, end: u32 },
}

impl PageCosts {
    /// What reading `matched`'s value whole takes.
    pub(super) fn read(&self, matched: &octocode_engine::types::TextSearchMatch) -> RowRead {
        if !self.clipped(matched) {
            RowRead::None
        } else if self.multiline {
            RowRead::Multiline
        } else if passes_line_page(
            // The same rows `is_long_line_hit` reads by a byte window.
            matched
                .original_bytes
                .map_or_else(|| matched.value.len(), |bytes| bytes as usize),
        ) {
            RowRead::Long
        } else {
            let (start, end) = hit_windows([matched.line], self.expand_context)[0];
            RowRead::Window { start, end }
        }
    }
}

/// The `expandValues` rows one file's rows on a page need, kept as rows
/// join: its long-line hits, its merged hit windows ([`hit_windows`]), and
/// whether a multiline value is clipped. Adding a row costs a lookup among
/// the windows, so a page cut never rescans the rows a file already shows.
#[derive(Clone, Debug, Default)]
pub(super) struct FileReads {
    multiline: bool,
    long: usize,
    /// Merged windows, start to end: disjoint and not adjacent.
    windows: std::collections::BTreeMap<u32, u32>,
}

impl FileReads {
    /// The `expandValues` rows these reads take.
    pub(super) fn rows(&self) -> usize {
        if self.multiline {
            1
        } else {
            self.long + self.windows.len().div_ceil(RANGES_MAX_ITEMS)
        }
    }

    /// The rows `read` would add, without adding it.
    pub(super) fn added(&self, read: RowRead) -> usize {
        let after = match read {
            RowRead::None => return 0,
            RowRead::Multiline => 1,
            _ if self.multiline => 1,
            RowRead::Long => self.long + 1 + self.windows.len().div_ceil(RANGES_MAX_ITEMS),
            RowRead::Window { start, end } => {
                let merged = self.touching(start, end).count();
                self.long + (self.windows.len() + 1 - merged).div_ceil(RANGES_MAX_ITEMS)
            }
        };
        after.saturating_sub(self.rows())
    }

    pub(super) fn add(&mut self, read: RowRead) {
        match read {
            RowRead::None => {}
            RowRead::Multiline => self.multiline = true,
            RowRead::Long => self.long += 1,
            RowRead::Window { start, end } => {
                let touching = self.touching(start, end).collect::<Vec<_>>();
                let mut merged = (start, end);
                for (from, to) in touching {
                    self.windows.remove(&from);
                    merged = (merged.0.min(from), merged.1.max(to));
                }
                self.windows.insert(merged.0, merged.1);
            }
        }
    }

    /// The windows `start..=end` overlaps or adjoins, as [`hit_windows`]
    /// merges them.
    fn touching(&self, start: u32, end: u32) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.windows
            .range(..=end.saturating_add(1))
            .rev()
            .take_while(move |(_, to)| to.saturating_add(1) >= start)
            .map(|(&from, &to)| (from, to))
    }
}
