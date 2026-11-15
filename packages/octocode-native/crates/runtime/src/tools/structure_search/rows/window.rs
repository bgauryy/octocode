//! One window of a walk's listing: its entries ranked, and rows built and
//! redacted only for the entries the window keeps.
use super::{EntryKind, Lines, Row, content_size};
use crate::security::ContentSecurity;
use octocode_contract_types::StructureSearchSort;
use octocode_engine::types::FileSystemEntry;

/// A walk's entries (the listed directory left out) ranked by a sort. Rows are
/// built only for the window that lists them. A row ranks as its entry does,
/// since path and name order rank by what is delivered, so every path is
/// redacted first for those.
pub(in crate::tools::structure_search) struct Ranking {
    order: Vec<usize>,
    delivered: Vec<Option<String>>,
    /// The sort's key, whose value a names-only row states.
    key: Option<octocode_contract_types::StructureSearchSortAnyOrderBy>,
}

impl Ranking {
    /// `entries` ranked by `sort`, by their line counts from `lines` (the
    /// walk's entries, in the same order).
    pub(in crate::tools::structure_search) fn of(
        entries: &[FileSystemEntry],
        lines: &[Option<Lines>],
        (root, security): (&std::path::Path, &ContentSecurity),
        sort: &StructureSearchSort,
    ) -> Self {
        let mut order = (0..entries.len())
            .filter(|&at| !Row::is_root_dir(&entries[at], root))
            .collect::<Vec<_>>();
        let mut delivered = vec![None; entries.len()];
        if matches!(
            super::super::files::sort_key(sort),
            Some(
                octocode_contract_types::StructureSearchSortAnyOrderBy::Path
                    | octocode_contract_types::StructureSearchSortAnyOrderBy::Name
            )
        ) {
            for &at in &order {
                delivered[at] = Some(Row::listed_path(&entries[at], root, security));
            }
        }
        rank(&mut order, sort, entries, lines, &delivered);
        Self {
            order,
            delivered,
            key: super::super::files::sort_key(sort),
        }
    }

    /// The entries in listing order, by their index in the walk.
    pub(in crate::tools::structure_search) fn order(&self) -> &[usize] {
        &self.order
    }

    /// The rows of the ranked entries in `window`.
    pub(in crate::tools::structure_search) fn rows(
        &mut self,
        entries: &[FileSystemEntry],
        lines: &[Option<Lines>],
        (root, security): (&std::path::Path, &ContentSecurity),
        window: std::ops::Range<usize>,
    ) -> Vec<Row> {
        let available = self.order.len();
        self.order[window.start.min(available)..window.end.min(available)]
            .iter()
            .map(|&at| {
                let path = self.delivered[at]
                    .take()
                    .unwrap_or_else(|| Row::listed_path(&entries[at], root, security));
                Row::of(
                    &entries[at],
                    path,
                    lines.get(at).copied().flatten(),
                    self.key,
                )
            })
            .collect()
    }
}

/// The rows of `entries`, already in listing order by `sort`, each with its
/// line count from `lines` (empty when none was counted).
pub(in crate::tools::structure_search) fn rows_of(
    entries: &[FileSystemEntry],
    lines: &[Option<Lines>],
    (root, security): (&std::path::Path, &ContentSecurity),
    sort: &StructureSearchSort,
) -> Vec<Row> {
    let key = super::super::files::sort_key(sort);
    entries
        .iter()
        .enumerate()
        .map(|(at, entry)| {
            let path = Row::listed_path(entry, root, security);
            Row::of(entry, path, lines.get(at).copied().flatten(), key)
        })
        .collect()
}

/// Feed `writer` what `entry`'s row shows: its name and kind, and what the walk
/// read of its size, time and lines.
pub(in crate::tools::structure_search) fn digest_entry(
    writer: &mut octocode_engine::digest::HashWriter,
    entry: &FileSystemEntry,
    lines: Option<Lines>,
) {
    writer.update(entry.relative_path.as_bytes());
    writer.update([0, EntryKind::from(entry.entry_type) as u8]);
    writer.update(entry.size.unwrap_or(-1).to_le_bytes());
    writer.update(entry.modified_ms().unwrap_or(-1.0).to_le_bytes());
    writer.update([Lines::class(lines)]);
    writer.update(Lines::count(lines).to_le_bytes());
    // What a same-size edit with its time restored moves.
    if let Some(stamp) = entry.stamp {
        writer.update(stamp.changed_ns.to_le_bytes());
        writer.update(stamp.inode.to_le_bytes());
    }
}

/// Rank entries by `sort`; ties and traversal order keep the walk order (the
/// sort is stable).
/// An entry without the key (no content size, no line count) has nothing to
/// rank by: after the ones that have it, in either direction. `delivered` holds
/// each entry's path for path and name order.
fn rank(
    ranked: &mut [usize],
    sort: &StructureSearchSort,
    entries: &[FileSystemEntry],
    lines: &[Option<Lines>],
    delivered: &[Option<String>],
) {
    use octocode_contract_types::{SortOrder, StructureSearchSortAnyOrderBy as Key};
    let StructureSearchSort::AnyOrder(octocode_contract_types::StructureSearchSortAnyOrder {
        by,
        order,
    }) = sort
    else {
        return;
    };
    let directed = |compared: std::cmp::Ordering| {
        if *order == SortOrder::Desc {
            compared.reverse()
        } else {
            compared
        }
    };
    let line = |at: usize| lines.get(at).copied().flatten();
    let not_file = |at: usize| EntryKind::from(entries[at].entry_type) != EntryKind::File;
    let modified = |at: usize| entries[at].modified_ms().unwrap_or(0.0);
    let name = |at: usize| {
        delivered[at]
            .as_deref()
            .map(|path| path.rsplit_once('/').map_or(path, |(_, name)| name))
    };
    ranked.sort_by(|&a, &b| match by {
        Key::TotalLines => Lines::class(line(a))
            .cmp(&Lines::class(line(b)))
            .then_with(|| directed(Lines::count(line(a)).cmp(&Lines::count(line(b))))),
        Key::SizeBytes => not_file(a).cmp(&not_file(b)).then_with(|| {
            directed(
                content_size(&entries[a])
                    .unwrap_or(0)
                    .cmp(&content_size(&entries[b]).unwrap_or(0)),
            )
        }),
        Key::Name => directed(name(a).cmp(&name(b))),
        Key::Path => directed(delivered[a].cmp(&delivered[b])),
        Key::ModifiedAt => directed(modified(a).total_cmp(&modified(b))),
    });
}

#[cfg(test)]
mod tests;
