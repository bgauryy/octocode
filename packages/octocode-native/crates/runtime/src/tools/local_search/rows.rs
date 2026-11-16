//! Match rows: projection and context-window merging.

use super::types::*;
use octocode_engine::num::saturating_u32;
use octocode_engine::types::SearchContext;

/// The row a page shows for `matched`. `span_columns`: the view's rows are
/// spans, each stating its column
/// ([`shows_span_columns`](super::local_search_query::shows_span_columns)).
pub(super) fn project_match(
    matched: &octocode_engine::types::TextSearchMatch,
    span_columns: bool,
) -> SearchMatch {
    let (mut row, value) = project_shape(matched, span_columns);
    row.value = value.to_owned();
    row
}

/// A row as [`project_match`] shows it, its value left empty, and the value
/// it shows: a page cut prices the row from both without copying the value.
/// A row whose value is its lines whole states no column; a span does, and
/// so does a window of a long line, in a view of any shape.
pub(super) fn project_shape(
    matched: &octocode_engine::types::TextSearchMatch,
    span_columns: bool,
) -> (SearchMatch, &str) {
    let row = SearchMatch {
        line: matched.line,
        column: Some(crate::num::one_based_column(matched.column)),
        value: String::new(),
        window_start: matched.window_start,
        byte_offset: matched.byte_offset,
        first_line: matched.first_line,
        end_line: matched.last_line.filter(|&last| last > matched.line),
        match_lines: None,
        count: matched.count,
        enclosing: None,
        declaration: matched.is_declaration().then_some(true),
    };
    let row = SearchMatch {
        column: row
            .column
            .filter(|_| span_columns || matched.window_start.is_some()),
        ..row
    };
    (row, &matched.value)
}

/// First source line and lines of a content-view row's context window, or
/// `None` when the value is not a plain window (redacted, grouped, or lines
/// that disagree with the window the search reports). The window starts where
/// the search says the value starts ([`SearchMatch::first_line`]): the
/// contiguous lines it joined, which stop at the file's edges and at an
/// absent neighbour.
pub(super) fn context_window(
    matched: &SearchMatch,
    context: SearchContext,
) -> Option<(u32, Vec<&str>)> {
    let span = matched.end_line.map_or(Some(1), |last| {
        last.checked_sub(matched.line)?.checked_add(1)
    })?;
    let shape = WindowShape {
        line: matched.line,
        first_line: matched.first_line,
        span,
        counted: matched.count.is_some(),
        partial: matched.window_start.is_some(),
    };
    shape.window(&matched.value, context)
}

/// What places a row's value in its source: its match line, the line the
/// search says the value starts at, how many lines the match spans, and
/// whether the value is a count, or part of a long line, rather than lines.
struct WindowShape {
    line: u32,
    first_line: u32,
    span: u32,
    counted: bool,
    partial: bool,
}

impl WindowShape {
    /// `value`'s first source line and lines when it is a plain window
    /// ([`context_window`]).
    fn window(self, value: &str, context: SearchContext) -> Option<(u32, Vec<&str>)> {
        if self.counted || self.partial || self.line == 0 || self.first_line == 0 {
            return None;
        }
        let lines: Vec<&str> = value.split('\n').collect();
        let before = self.line.checked_sub(self.first_line)?;
        let after = u32::try_from(lines.len())
            .ok()?
            .checked_sub(before.checked_add(self.span)?)?;
        (before <= context.before && after <= context.after).then_some((self.first_line, lines))
    }
}

/// The source lines a hit's row holds: `first..=last` of its context window,
/// or its matched lines alone when its value is not a plain window (it is
/// then shown as it is and numbers no line). `line..=matched_last` are the
/// lines the match covers (`line` itself for a one-line match).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HitWindow {
    pub(super) line: u32,
    pub(super) matched_last: u32,
    pub(super) first: u32,
    pub(super) last: u32,
}

impl HitWindow {
    /// The window `matched`'s value holds, as the search joined it.
    pub(super) fn of(
        matched: &octocode_engine::types::TextSearchMatch,
        context: SearchContext,
    ) -> Self {
        let last_matched = matched.last_line.unwrap_or(matched.line).max(matched.line);
        let shape = WindowShape {
            line: matched.line,
            first_line: matched.first_line,
            span: last_matched - matched.line + 1,
            counted: matched.count.is_some(),
            partial: matched.window_start.is_some(),
        };
        match shape.window(&matched.value, context) {
            Some((first, lines)) => Self {
                line: matched.line,
                matched_last: last_matched,
                first,
                last: first.saturating_add(saturating_u32(lines.len())) - 1,
            },
            None => Self {
                line: matched.line,
                matched_last: last_matched,
                first: matched.line,
                last: last_matched,
            },
        }
    }
}

/// Merge rows whose context windows overlap or touch into one block, so each
/// source line is emitted once, numbered. A merged block keeps the first row's
/// `line`/`column`, and `matchedLines` lists every hit line it holds. Rows
/// merge only when both windows are plain and their shared lines are
/// byte-identical, so a redacted window is never spliced.
///
/// `hits` are the windows of every hit of the file (sorted by line, one per
/// line, any page, [`HitWindow::of`]): a window shows only the lines its hit
/// owns ([`owned_lines`]), so a hit or context line that another page's row
/// owns is never shown twice across pages.
/// The lines `[low, high]` of `window` its hit owns among the file's `hits`:
/// a line only one window holds is that hit's; a line two windows hold
/// belongs to the nearer hit, the earlier on a tie. The lines a match
/// covers are always its hit's, and distance is measured from them. Each
/// line a window holds is owned by exactly one hit that holds it, so pages showing disjoint hits
/// show disjoint lines, and none is left out.
pub(super) fn owned_lines(window: HitWindow, hits: &[HitWindow]) -> (u32, u32) {
    let HitWindow {
        line,
        matched_last,
        first,
        last,
    } = window;
    let (mut low, mut high) = (first, last);
    let at = hits.partition_point(|hit| hit.line < line);
    if let Some(previous) = at.checked_sub(1).and_then(|i| hits.get(i)) {
        // Lines past the previous window, or nearer this hit.
        let past = previous.last.saturating_add(1);
        let gap = line.saturating_sub(previous.matched_last);
        let nearer = previous.matched_last + gap / 2 + 1;
        low = low.max(past.min(nearer));
    }
    if let Some(next) = hits[at..].iter().find(|hit| hit.line > line) {
        // Lines before the next window, or nearer this hit.
        let before = next.first.saturating_sub(1);
        let gap = next.line.saturating_sub(matched_last);
        let nearer = matched_last + gap / 2;
        high = high.min(before.max(nearer));
    }
    (low.min(line), high.max(matched_last))
}

pub(super) fn merge_context_windows(
    rows: Vec<SearchMatch>,
    context: SearchContext,
    hits: &[HitWindow],
) -> Vec<SearchMatch> {
    struct Block {
        head: SearchMatch,
        start: u32,
        lines: Vec<String>,
        match_lines: Vec<u32>,
    }
    // Every window is numbered `<line>\t<text>`, so a cited
    // line never has to be counted from `line`.
    fn flush(block: Block) -> SearchMatch {
        let mut head = block.head;
        head.first_line = block.start;
        // One number per window line, blank lines included (a window can end
        // on an empty line, which a split-based numberer would drop).
        let mut value = String::new();
        for (index, (line, number)) in block.lines.iter().zip(block.start..).enumerate() {
            if index > 0 {
                value.push('\n');
            }
            crate::response::numbered::push_numbered(&mut value, number, line);
        }
        head.value = value;
        if block.match_lines.len() > 1 {
            head.match_lines = Some(block.match_lines);
        }
        head
    }
    let mut out = Vec::with_capacity(rows.len());
    let mut open: Option<Block> = None;
    for row in rows {
        let Some((start, lines)) = context_window(&row, context) else {
            out.extend(open.take().map(flush));
            out.push(row);
            continue;
        };
        // The lines this hit owns of its window ([`owned_lines`]).
        let (start, lines) = {
            let end = start.saturating_add(saturating_u32(lines.len())); // exclusive
            let window = HitWindow {
                line: row.line,
                matched_last: row.end_line.unwrap_or(row.line).max(row.line),
                first: start,
                last: end - 1,
            };
            let (low, high) = owned_lines(window, hits);
            let from = low.max(start);
            let to = high.saturating_add(1).min(end).max(from);
            (
                from,
                lines[(from - start) as usize..(to - start) as usize].to_vec(),
            )
        };
        // A match spanning several lines stands alone: its window is its
        // own, numbered, its extent stated by its `endLine`.
        if row.end_line.is_some() {
            let lines = lines.into_iter().map(str::to_owned).collect();
            let line = row.line;
            out.extend(open.take().map(flush));
            out.push(flush(Block {
                head: row,
                start,
                lines,
                match_lines: vec![line],
            }));
            continue;
        }
        if let Some(block) = open.as_mut() {
            let end = block
                .start
                .saturating_add(saturating_u32(block.lines.len())); // exclusive
            let last_match = block.match_lines.last().copied().unwrap_or(0);
            let shared_agrees = (start..end.min(start.saturating_add(saturating_u32(lines.len()))))
                .all(|n| block.lines[(n - block.start) as usize] == lines[(n - start) as usize]);
            let fresh = (end.saturating_sub(start) as usize).min(lines.len());
            if row.line > last_match && start >= block.start && start <= end && shared_agrees {
                block
                    .lines
                    .extend(lines.iter().skip(fresh).map(|l| (*l).to_owned()));
                block.match_lines.push(row.line);
                block.head.declaration = block.head.declaration.or(row.declaration);
                continue;
            }
        }
        let lines = lines.into_iter().map(str::to_owned).collect();
        let line = row.line;
        out.extend(
            open.replace(Block {
                head: row,
                start,
                lines,
                match_lines: vec![line],
            })
            .map(flush),
        );
    }
    out.extend(open.map(flush));
    out
}

#[cfg(test)]
mod tests;
