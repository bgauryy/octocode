//! Default-layout pages cut from one row stream.

use super::costs::RowName;
use super::entries::pagination_chars;
use super::expansion::{FileReads, RowRead};
use crate::id::ToolId;

/// Serialized sizes a streamed page is cut by.
pub(super) struct StreamCosts<'a> {
    /// A file entry around its rows.
    pub(super) file: &'a dyn Fn(&octocode_engine::types::TextSearchFile) -> usize,
    /// One match row.
    pub(super) row: &'a dyn Fn(&octocode_engine::types::TextSearchMatch) -> usize,
    /// The enclosing name a row (by file index and rank) carries.
    pub(super) name: &'a dyn Fn(usize, usize) -> RowName,
    /// What reading one row's clipped value takes (see [`FileReads`]).
    pub(super) read: &'a dyn Fn(&octocode_engine::types::TextSearchMatch) -> RowRead,
}

/// Rows one `expandValues` call runs: a page never needs more to read every
/// clipped value it shows.
pub(super) fn max_expansion_rows() -> usize {
    crate::response::result::rows_per_call(ToolId::LocalFetch)
        .min(crate::response::result::rows_per_call(ToolId::LocalSearch))
}

/// Rows one page shows: (file index, rank-order row range), in page order.
pub(super) type PageRows = Vec<(usize, std::ops::Range<usize>)>;

/// Default-layout pages, cut from one row stream: each file's first
/// `per_file` ranked rows in file order, then every file's remaining rows.
/// Each page holds as much of the stream as fits `budget` serialized chars
/// (at least one row), counting a file's `pagination` whenever its later
/// rows may fall on a later page; however many files that is, every hit
/// row it shows is named. No page shows clipped values that need more reads than one `expandValues`
/// call runs ([`max_expansion_rows`]), so each page's values are one call
/// away. `whole` puts the entire result on one page. A file's
/// rows on one page are always one contiguous rank range, and its later
/// rows follow on later pages. With `through`, the cut stops after that
/// many pages, plus a page of the one entry that opens the next.
pub(super) fn stream_pages(
    files: &[octocode_engine::types::TextSearchFile],
    per_file: usize,
    whole: bool,
    budget: usize,
    costs: StreamCosts<'_>,
    through: Option<usize>,
) -> Vec<PageRows> {
    if whole {
        return vec![
            files
                .iter()
                .enumerate()
                .map(|(index, file)| (index, 0..file.matches.len()))
                .collect(),
        ];
    }
    let first = files
        .iter()
        .enumerate()
        .map(|(index, file)| (index, 0..file.matches.len().min(per_file)));
    let rest = files
        .iter()
        .enumerate()
        .filter(|(_, file)| file.matches.len() > per_file)
        .map(|(index, file)| (index, per_file..file.matches.len()));
    let mut pages: Vec<PageRows> = Vec::new();
    let mut page: PageRows = Vec::new();
    // Each file on the page: its entry's position, and the reads its rows
    // there need, kept as rows join so a row's added reads cost no rescan.
    let mut slot = std::collections::HashMap::<usize, (usize, FileReads)>::new();
    // Files whose `pagination` this page already counts.
    let mut paged = std::collections::HashSet::<usize>::new();
    // Each file entry's own chars and its `pagination`'s, priced once: a
    // file rides several pages.
    let mut entries_chars = vec![None::<(usize, usize)>; files.len()];
    let mut used = 0usize;
    let max_reads = max_expansion_rows();
    // `expandValues` rows the page's clipped values need.
    let mut reads = 0usize;
    for (index, rows) in first.chain(rest) {
        let file = &files[index];
        let (file_chars, pagination) = *entries_chars[index]
            .get_or_insert_with(|| ((costs.file)(file), pagination_chars(file)));
        // A row-less file still takes its place as one entry.
        let entries: Vec<Option<usize>> = if rows.is_empty() {
            vec![None]
        } else {
            rows.map(Some).collect()
        };
        for row in entries {
            let more_after = row.is_some_and(|row| row + 1 < file.matches.len());
            let name = row.map_or(RowName::None, |row| (costs.name)(index, row));
            let read = row.map_or(RowRead::None, |row| (costs.read)(&file.matches[row]));
            let row_chars = row.map_or(0, |row| (costs.row)(&file.matches[row]));
            let cost = |slot: &std::collections::HashMap<usize, (usize, FileReads)>,
                        paged: &std::collections::HashSet<usize>| {
                row_chars
                    + name.chars()
                    + if slot.contains_key(&index) {
                        0
                    } else {
                        file_chars
                    }
                    + if more_after && !paged.contains(&index) {
                        pagination
                    } else {
                        0
                    }
            };
            // The reads this row adds to its file's on the page: only a
            // clipped row adds one.
            let added_reads = |slot: &std::collections::HashMap<usize, (usize, FileReads)>| {
                slot.get(&index).map_or_else(
                    || FileReads::default().added(read),
                    |(_, reads)| reads.added(read),
                )
            };
            let over_budget = used + cost(&slot, &paged) > budget;
            let over_reads = reads + added_reads(&slot) > max_reads;
            if !page.is_empty() && (over_budget || over_reads) {
                pages.push(std::mem::take(&mut page));
                if through.is_some_and(|through| pages.len() > through) {
                    // The entry that opens the next page, alone.
                    let span = row.map_or(0..0, |row| row..row + 1);
                    pages.push(vec![(index, span)]);
                    return pages;
                }
                slot.clear();
                paged.clear();
                used = 0;
                reads = 0;
            }
            used += cost(&slot, &paged);
            reads += added_reads(&slot);
            if more_after {
                paged.insert(index);
            }
            let span = row.map_or(0..0, |row| row..row + 1);
            match slot.get_mut(&index) {
                Some((position, file_reads)) => {
                    let range = &mut page[*position].1;
                    *range = range.start.min(span.start)..range.end.max(span.end);
                    file_reads.add(read);
                }
                None => {
                    let mut file_reads = FileReads::default();
                    file_reads.add(read);
                    slot.insert(index, (page.len(), file_reads));
                    page.push((index, span));
                }
            }
        }
    }
    if !page.is_empty() {
        pages.push(page);
    }
    pages
}
