//! One window of a walk's listing: its entries ranked, and rows built and
//! redacted only for the entries the window keeps.
use super::{EntryKind, Lines, Row, content_size};
use crate::security::ContentSecurity;
use octocode_contract_types::StructureSearchSort;
use octocode_engine::types::FileSystemEntry;

/// A walk's entries (the listed directory left out) ranked by `sort`, and the
/// rows of those in `window`, each with its line count from `lines` (the
/// walk's entries, in the same order); with how many entries it ranked. Only
/// the window's rows are built. A row ranks as its entry does, since path and name
/// order rank by what is delivered, so every path is redacted first for those.
pub(in crate::tools::structure_search) fn window_rows(
    entries: &[FileSystemEntry],
    lines: &[Option<Lines>],
    (root, security): (&std::path::Path, &ContentSecurity),
    sort: &StructureSearchSort,
    window: std::ops::Range<usize>,
) -> (Vec<Row>, usize) {
    let mut ranked = (0..entries.len())
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
        for &at in &ranked {
            delivered[at] = Some(Row::listed_path(&entries[at], root, security));
        }
    }
    rank(&mut ranked, sort, entries, lines, &delivered);
    let available = ranked.len();
    let rows = ranked[window.start.min(available)..window.end.min(available)]
        .iter()
        .map(|&at| {
            let path = delivered[at]
                .take()
                .unwrap_or_else(|| Row::listed_path(&entries[at], root, security));
            Row::of(&entries[at], path, lines.get(at).copied().flatten())
        })
        .collect();
    (rows, available)
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
        Key::LineCount => Lines::class(line(a))
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
