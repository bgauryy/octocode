//! One page of a scan as its own small dense scan ([`PageView`]): the files
//! the page shows, each holding only its shown rows, with what each needs of
//! its rows on other pages ([`EntryFacts`]) and the scan's totals.
//!
//! A fresh page builds it from the whole scan once the page is cut
//! ([`PageView::dense`]); a continuation of a seekable stored scan reads it
//! from the page's frames alone ([`super::packed::Packed::page`]). Every
//! later stage (verification, projection, cursor, settle) reads only the
//! view, so a stored page and the fresh page of the same call are one code
//! path and show the same row.

use super::layout::{Layout, MAX_MORE_LINES};
use super::rows::HitWindow;
use octocode_engine::num::saturating_u32;
use octocode_engine::types::{TextSearchFile, TextSearchMatch, TextSearchResult};
use std::cmp::Reverse;

/// The page's files and rows: `parsed` holds the page's files in page
/// order, each with only its shown rows (in rank order), and `layout.shown`
/// names every row of each, `(i, 0..rows)`. `facts[i]` is what file `i`'s
/// entry needs of the rows the page does not hold.
pub(super) struct PageView {
    pub(super) parsed: TextSearchResult,
    pub(super) layout: Layout,
    pub(super) facts: Vec<EntryFacts>,
    pub(super) totals: Totals,
}

/// What one file entry of a page needs of its file's rows on other pages:
/// its pagination (`total`, the shown rank range, the lines of later rows)
/// and, when context windows merge, the windows of other rows that border
/// a shown row (see [`neighbours`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct EntryFacts {
    /// The file's rows on every page.
    pub(super) total: u32,
    /// The shown rows' rank range in the file, within `total`.
    pub(super) start: u32,
    pub(super) end: u32,
    /// The first [`MAX_MORE_LINES`] distinct lines of the file's rows past
    /// `end`, ascending, and how many distinct lines follow them.
    pub(super) later: Vec<u32>,
    pub(super) unlisted: Option<u32>,
    /// Windows of the file's rows outside the shown range that a shown
    /// row's owned lines depend on, one per line.
    pub(super) neighbours: Vec<HitWindow>,
}

/// What the whole scan holds, for the page's cursor and totals.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Totals {
    /// Files matched.
    pub(super) files: u32,
    /// Some file has more rows than one match page.
    pub(super) over_matches_per: bool,
}

impl PageView {
    /// The page `layout` cut from the whole scan `parsed`, and each page
    /// file's index in `parsed`.
    pub(super) fn dense(parsed: TextSearchResult, mut layout: Layout) -> (Self, Vec<usize>) {
        let TextSearchResult { mut files, stats } = parsed;
        let totals = Totals {
            files: saturating_u32(files.len()),
            over_matches_per: files
                .iter()
                .any(|file| saturating_u32(file.matches.len()) > layout.matches_per),
        };
        let shown = std::mem::take(&mut layout.shown);
        let mut page = Vec::with_capacity(shown.len());
        let mut facts = Vec::with_capacity(shown.len());
        let mut indexes = Vec::with_capacity(shown.len());
        for (index, rows) in shown {
            let Some(file) = files.get_mut(index) else {
                continue;
            };
            let total = file.matches.len();
            let end = rows.end.min(total);
            let start = rows.start.min(end);
            let windows = layout
                .merge_context
                .map(|context| windows_of(&file.matches, context));
            facts.push(entry_facts(
                &file.matches,
                windows.as_deref(),
                start..end,
                &mut later_lines(&file.matches, &[end]).into_iter(),
            ));
            let mut entry = header_of(file);
            entry.matches = file.matches.drain(start..end).collect();
            layout.shown.push((page.len(), 0..entry.matches.len()));
            page.push(entry);
            indexes.push(index);
        }
        (
            Self {
                parsed: TextSearchResult { files: page, stats },
                layout,
                facts,
                totals,
            },
            indexes,
        )
    }
}

/// `file` without its rows (its path and counts moved out of it).
pub(super) fn header_of(file: &mut TextSearchFile) -> TextSearchFile {
    TextSearchFile {
        term_counts: std::mem::take(&mut file.term_counts),
        path: std::mem::take(&mut file.path),
        match_count: file.match_count,
        matches: Vec::new(),
        source: file.source,
    }
}

/// The facts of the entry showing `rows[shown]` (see [`EntryFacts`]);
/// `windows` are every row's windows when context merges, and `later` yields
/// the entry's [`later_lines`].
pub(super) fn entry_facts(
    rows: &[TextSearchMatch],
    windows: Option<&[HitWindow]>,
    shown: std::ops::Range<usize>,
    later: &mut impl Iterator<Item = (Vec<u32>, Option<u32>)>,
) -> EntryFacts {
    let (later, unlisted) = later.next().unwrap_or_default();
    EntryFacts {
        total: saturating_u32(rows.len()),
        start: saturating_u32(shown.start),
        end: saturating_u32(shown.end),
        later,
        unlisted,
        neighbours: windows.map_or_else(Vec::new, |windows| {
            neighbours(windows, &by_line(windows), shown)
        }),
    }
}

/// Each row's context window, as its stored value holds it.
pub(super) fn windows_of(
    rows: &[TextSearchMatch],
    context: octocode_engine::types::SearchContext,
) -> Vec<HitWindow> {
    rows.iter().map(|row| HitWindow::of(row, context)).collect()
}

/// Row indexes of `windows` ordered by line (stable).
pub(super) fn by_line(windows: &[HitWindow]) -> Vec<usize> {
    let mut order = (0..windows.len()).collect::<Vec<_>>();
    order.sort_by_key(|&row| windows[row].line);
    order
}

/// For each of `ends` (any order), the first [`MAX_MORE_LINES`] distinct
/// lines of `rows[end..]`, ascending, and how many distinct lines follow
/// them. One backward pass serves every end, so a file cut into thousands
/// of entries costs one walk of its rows.
pub(super) fn later_lines(
    rows: &[TextSearchMatch],
    ends: &[usize],
) -> Vec<(Vec<u32>, Option<u32>)> {
    let mut out = vec![(Vec::new(), None); ends.len()];
    let mut order = (0..ends.len()).collect::<Vec<_>>();
    order.sort_by_key(|&at| Reverse(ends[at]));
    let mut seen = std::collections::HashSet::new();
    let mut lowest = std::collections::BTreeSet::new();
    let mut next = rows.len();
    for at in order {
        let end = ends[at].min(rows.len());
        while next > end {
            next -= 1;
            let line = rows[next].line;
            if seen.insert(line) {
                lowest.insert(line);
                if lowest.len() > MAX_MORE_LINES {
                    lowest.pop_last();
                }
            }
        }
        let unlisted = seen.len() - lowest.len();
        out[at] = (
            lowest.iter().copied().collect(),
            (unlisted > 0).then(|| saturating_u32(unlisted)),
        );
    }
    out
}

/// The order windows at one line are chosen in: the one covering most
/// lines first (see `project_files`).
fn window_key(window: &HitWindow) -> (u32, Reverse<u32>, u32, u32) {
    (
        window.line,
        Reverse(window.matched_last),
        window.first,
        window.last,
    )
}

/// The windows of rows outside `shown` that decide which lines each shown
/// row owns ([`super::rows::owned_lines`]): at each shown row's line, and at
/// the nearest line before and after it held by a row outside `shown`, the
/// first such row's window by [`window_key`]. Owned lines read only the
/// nearest window before and after a row among every row of the file, one
/// window per line; these are the windows that can be that nearest one, so
/// the page decides ownership as if it held every row.
/// `by_line` is [`by_line`] of `windows`.
pub(super) fn neighbours(
    windows: &[HitWindow],
    by_line: &[usize],
    shown: std::ops::Range<usize>,
) -> Vec<HitWindow> {
    let group = |line: u32| {
        (
            by_line.partition_point(|&row| windows[row].line < line),
            by_line.partition_point(|&row| windows[row].line <= line),
        )
    };
    let other = |(low, high): (usize, usize)| {
        by_line[low..high]
            .iter()
            .filter(|&&row| !shown.contains(&row))
            .map(|&row| windows[row])
            .min_by_key(window_key)
    };
    let mut lines = windows[shown.clone()]
        .iter()
        .map(|window| window.line)
        .collect::<Vec<_>>();
    lines.sort_unstable();
    lines.dedup();
    let mut out = Vec::new();
    // Nearest other line before each shown line: a line whose rows are
    // all shown is itself a shown line, whose answer is already known.
    let mut before = std::collections::HashMap::<u32, Option<HitWindow>>::new();
    for &line in &lines {
        let (low, _) = group(line);
        out.extend(other(group(line)));
        let found = low.checked_sub(1).and_then(|at| {
            let previous = windows[by_line[at]].line;
            other(group(previous)).or_else(|| before.get(&previous).copied().flatten())
        });
        before.insert(line, found);
        out.extend(found);
    }
    let mut after = std::collections::HashMap::<u32, Option<HitWindow>>::new();
    for &line in lines.iter().rev() {
        let (_, high) = group(line);
        let found = by_line.get(high).and_then(|&row| {
            let following = windows[row].line;
            other(group(following)).or_else(|| after.get(&following).copied().flatten())
        });
        after.insert(line, found);
        out.extend(found);
    }
    out.sort_unstable_by_key(window_key);
    out.dedup_by_key(|window| window.line);
    out
}

/// A seekable scan's page: the view, the rows of it whose value redaction
/// changed, and the page files whose key-block scan a page re-checks.
pub(super) struct StoredPage {
    pub(super) view: PageView,
    pub(super) redacted: super::verify::Redacted,
    /// Page files (by index) that had a key-block scan.
    pub(super) key_scans: Vec<usize>,
}

impl PageView {
    /// The page `query` asks for of the seekable scan `packed`, read from
    /// that page's frames alone; `None` when `packed` has no pages or a
    /// frame is not exactly its encoding, or `cancelled`.
    pub(super) fn stored(
        packed: &super::packed::Packed,
        query: &octocode_contract_types::LocalSearchQuery,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<StoredPage> {
        let index = packed.index()?;
        let page = super::local_search_query::page(query).max(1) as usize;
        let entries = index.pages.get(page - 1).cloned().unwrap_or_default();
        let mut files = Vec::with_capacity(entries.len());
        let mut facts = Vec::with_capacity(entries.len());
        let mut shown = Vec::with_capacity(entries.len());
        let mut redacted = super::verify::Redacted::new();
        let mut key_scans = Vec::new();
        for at in entries {
            let entry = packed.entry(at, cancelled)?;
            for row in &entry.redacted {
                let hit = entry.file.matches.get(*row as usize)?;
                redacted.insert((entry.file.path.clone(), hit.line, hit.column));
            }
            if entry.key_scan {
                key_scans.push(files.len());
            }
            shown.push((files.len(), 0..entry.file.matches.len()));
            facts.push(entry.facts);
            files.push(entry.file);
        }
        Some(StoredPage {
            view: Self {
                parsed: TextSearchResult {
                    files,
                    stats: packed.stats().clone(),
                },
                layout: Layout::from_index(query, index, shown),
                facts,
                totals: Totals {
                    files: index.total_files,
                    over_matches_per: index.over_matches_per,
                },
            },
            redacted,
            key_scans,
        })
    }
}

/// `parsed` (ordered and ranked as `layout` cut it) as a seekable scan: a
/// frame per entry of every page of the cut, each with its facts, which
/// rows redaction changed (`redacted`) and whether its file had a key-block
/// scan (`key_scans`, by path). `None` for a list or grid layout (kept
/// whole), past `max_raw` raw bytes, or once `cancelled`.
pub(super) fn encode_pages(
    parsed: &TextSearchResult,
    layout: &Layout,
    (redacted, key_scans): (&super::verify::Redacted, &std::collections::HashSet<String>),
    max_raw: usize,
    cancelled: &dyn Fn() -> bool,
) -> Option<super::packed::Encoded> {
    use super::packed::{FrameWriter, PageIndex};
    let files = &parsed.files;
    // Every page's entries, in page order: only a streamed cut is stored
    // by page (see [`PageIndex`]).
    let pages = layout
        .pages
        .as_deref()
        .filter(|_| layout.streamed && !layout.list)?;
    // Each file's later lines at every end its entries take, in entry order.
    let mut ends = vec![Vec::new(); files.len()];
    for (index, rows) in pages.iter().flatten() {
        ends.get_mut(*index)?
            .push(rows.end.min(files[*index].matches.len()));
    }
    let mut later = files
        .iter()
        .zip(&ends)
        .map(|(file, ends)| later_lines(&file.matches, ends).into_iter())
        .collect::<Vec<_>>();
    // Windows (and their line order) of each file whose rows merge.
    let mut windows = vec![None; files.len()];
    let mut out = FrameWriter::new(files.len(), max_raw);
    let mut ranges = Vec::with_capacity(pages.len());
    for page in pages {
        let first = out.len();
        for (index, rows) in page {
            let file = &files[*index];
            let end = rows.end.min(file.matches.len());
            let start = rows.start.min(end);
            let shown = &file.matches[start..end];
            let merged = layout.merge_context.map(|context| {
                &*windows[*index].get_or_insert_with(|| {
                    let rows = windows_of(&file.matches, context);
                    let order = by_line(&rows);
                    (rows, order)
                })
            });
            let facts = EntryFacts {
                total: saturating_u32(file.matches.len()),
                start: saturating_u32(start),
                end: saturating_u32(end),
                later: Vec::new(),
                unlisted: None,
                neighbours: merged.map_or_else(Vec::new, |(rows, order)| {
                    neighbours(rows, order, start..end)
                }),
            };
            let (later, unlisted) = later[*index].next().unwrap_or_default();
            let flags = shown
                .iter()
                .enumerate()
                .filter(|(_, hit)| redacted.contains(&(file.path.clone(), hit.line, hit.column)))
                .map(|(row, _)| saturating_u32(row))
                .collect::<Vec<_>>();
            out.entry(
                (*index, file),
                shown,
                &EntryFacts {
                    later,
                    unlisted,
                    ..facts
                },
                (&flags, key_scans.contains(&file.path)),
                cancelled,
            )?;
        }
        ranges.push(first..out.len());
    }
    let index = PageIndex {
        pages: ranges,
        total_files: saturating_u32(files.len()),
        total_rows: saturating_u32(files.iter().map(|file| file.matches.len()).sum::<usize>()),
        matches_per: layout.matches_per,
        over_matches_per: files
            .iter()
            .any(|file| saturating_u32(file.matches.len()) > layout.matches_per),
    };
    Some(out.finish(index, parsed.stats.clone()))
}

#[cfg(test)]
mod tests;
