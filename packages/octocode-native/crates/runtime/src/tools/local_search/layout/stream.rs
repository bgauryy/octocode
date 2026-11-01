//! Default-layout pages cut from one row stream.

use super::costs::RowName;
use super::entries::pagination_chars;

/// Serialized sizes a streamed page is cut by.
pub(super) struct StreamCosts<'a> {
    /// A file entry around its rows.
    pub(super) file: &'a dyn Fn(&octocode_engine::types::TextSearchFile) -> usize,
    /// One match row.
    pub(super) row: &'a dyn Fn(&octocode_engine::types::TextSearchMatch) -> usize,
    /// The enclosing name a row (by file index and rank) carries.
    pub(super) name: &'a dyn Fn(usize, usize) -> RowName,
}

/// Rows one page shows: (file index, rank-order row range), in page order.
pub(super) type PageRows = Vec<(usize, std::ops::Range<usize>)>;

/// Default-layout pages, cut from one row stream: each file's first
/// `per_file` ranked rows in file order, then every file's remaining rows.
/// Each page holds as much of the stream as fits `budget` serialized chars
/// (at least one row), counting a file's `pagination` whenever its later
/// rows may fall on a later page; however many files that is, every hit
/// row it shows is named. `whole` puts the entire result on one page. A file's
/// rows on one page are always one contiguous rank range, and its later
/// rows follow on later pages. With `through`, the cut stops after that
/// many pages, plus a page of the one entry that opens the next. A page
/// also ends at `limit` entries (hit rows; a row-less file is one entry),
/// at least one.
pub(super) fn stream_pages(
    files: &[octocode_engine::types::TextSearchFile],
    per_file: usize,
    whole: bool,
    (budget, limit): (usize, Option<usize>),
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
    // Each file on the page and its entry's position.
    let mut slot = std::collections::HashMap::<usize, usize>::new();
    // Files whose `pagination` this page already counts.
    let mut paged = std::collections::HashSet::<usize>::new();
    // Each file entry's own chars and its `pagination`'s, priced once: a
    // file rides several pages.
    let mut entries_chars = vec![None::<(usize, usize)>; files.len()];
    let mut used = 0usize;
    let limit = limit.unwrap_or(usize::MAX).max(1);
    let mut listed = 0usize;
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
            let row_chars = row.map_or(0, |row| (costs.row)(&file.matches[row]));
            let cost = |slot: &std::collections::HashMap<usize, usize>,
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
            let over_budget = used + cost(&slot, &paged) > budget || listed >= limit;
            if !page.is_empty() && over_budget {
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
                listed = 0;
            }
            used += cost(&slot, &paged);
            listed += 1;
            if more_after {
                paged.insert(index);
            }
            let span = row.map_or(0..0, |row| row..row + 1);
            match slot.get_mut(&index) {
                Some(position) => {
                    let range = &mut page[*position].1;
                    *range = range.start.min(span.start)..range.end.max(span.end);
                }
                None => {
                    slot.insert(index, page.len());
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
