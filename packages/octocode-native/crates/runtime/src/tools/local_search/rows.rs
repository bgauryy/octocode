//! Match rows: ranking, projection, and context-window merging.

use super::types::*;
use crate::num::u32_of_usize;
use octocode_engine::types::SearchContext;

/// Order a file's rows for paging: by lexical hit rank, then distinct
/// complete values before repeats, stable by line.
pub(super) fn rank_file_rows(matches: &mut Vec<octocode_engine::types::TextSearchMatch>) {
    matches.sort_by_key(|matched| std::cmp::Reverse(matched.rank.unwrap_or(1)));
    let mut seen = std::collections::HashSet::new();
    let (distinct, repeats): (Vec<_>, Vec<_>) = std::mem::take(matches)
        .into_iter()
        .partition(|matched| seen.insert(matched.value_digest));
    matches.extend(distinct);
    matches.extend(repeats);
}

pub(super) fn project_match(matched: &octocode_engine::types::TextSearchMatch) -> SearchMatch {
    let (mut row, value) = project_shape(matched);
    row.value = value.to_owned();
    row
}

/// A row as [`project_match`] shows it, its value left empty, and the value
/// it shows: a page cut prices the row from both without copying the value.
pub(super) fn project_shape(
    matched: &octocode_engine::types::TextSearchMatch,
) -> (SearchMatch, &str) {
    let row = SearchMatch {
        line: matched.line,
        column: Some(crate::num::one_based_column(matched.column)),
        value: String::new(),
        byte_offset: matched.byte_offset,
        match_lines: spanned_lines(matched),
        count: matched.count,
        enclosing: None,
        declaration: (matched.rank == Some(3)).then_some(true),
    };
    (row, &matched.value)
}

/// Every line a match spanning several lines covers (multiline mode), as
/// `matchedLines`; `None` for a one-line match.
fn spanned_lines(matched: &octocode_engine::types::TextSearchMatch) -> Option<Vec<u32>> {
    matched
        .last_line
        .filter(|&last| last > matched.line)
        .map(|last| (matched.line..=last).collect())
}

/// Source line range and lines of a content-view row's context window, or
/// `None` when the value is not a plain window (redacted, grouped, or a shape the window arithmetic cannot account for).
/// The engine joins the requested contiguous lines before and after the match,
/// clamped at the file start/end.
pub(super) fn context_window(
    matched: &SearchMatch,
    context: SearchContext,
) -> Option<(u32, Vec<&str>)> {
    if matched.count.is_some() || matched.line == 0 {
        return None;
    }
    let lines: Vec<&str> = matched.value.split('\n').collect();
    let span = matched
        .match_lines
        .as_ref()
        .map_or(Some(1), |spanned| u32::try_from(spanned.len()).ok())?;
    let before = context.before.min(matched.line - 1);
    let after = u32::try_from(lines.len())
        .ok()?
        .checked_sub(before + span)?;
    (after <= context.after).then_some((matched.line - before, lines))
}

/// Merge rows whose context windows overlap or touch into one block, so each
/// source line is emitted once, numbered. A merged block keeps the first row's
/// `line`/`column`, and `matchedLines` lists every matched line it holds. Rows
/// merge only when both windows are plain and their shared lines are
/// byte-identical, so a redacted window is never spliced.
///
/// `hits` are every hit line of the file (sorted, any page): a window shows
/// only the lines its hit owns ([`owned_lines`]), so a hit or context line
/// that another page's row owns is never shown twice across pages.
/// The lines `[low, high]` of hit `line`'s context window it owns among
/// the file's `hits` (sorted): a line within reach of two hits belongs to
/// the nearer one, the earlier on a tie. Each line of every window is owned
/// by exactly one hit, so pages showing disjoint hits show disjoint lines.
pub(super) fn owned_lines(line: u32, context: SearchContext, hits: &[u32]) -> (u32, u32) {
    let mut low = line.saturating_sub(context.before);
    let mut high = line.saturating_add(context.after);
    let at = hits.partition_point(|&hit| hit < line);
    if let Some(&previous) = at.checked_sub(1).and_then(|i| hits.get(i)) {
        // Lines past the previous hit's reach, or nearer this hit.
        let past_reach = previous.saturating_add(context.after).saturating_add(1);
        let nearer = previous + (line - previous) / 2 + 1;
        low = low.max(past_reach.min(nearer));
    }
    if let Some(&next) = hits[at..].iter().find(|&&hit| hit > line) {
        let before_reach = next.saturating_sub(context.before).saturating_sub(1);
        let nearer = line + (next - line) / 2;
        high = high.min(before_reach.max(nearer));
    }
    (low, high)
}

pub(super) fn merge_context_windows(
    rows: Vec<SearchMatch>,
    context: SearchContext,
    hits: &[u32],
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
        // A match spanning several lines stands alone: its window is its
        // own, numbered, with every line it covers a matched line.
        if let Some(spanned) = row.match_lines.clone() {
            let lines = lines.into_iter().map(str::to_owned).collect();
            out.extend(open.take().map(flush));
            out.push(flush(Block {
                head: row,
                start,
                lines,
                match_lines: spanned,
            }));
            continue;
        }
        let (start, lines) = {
            let (low, high) = owned_lines(row.line, context, hits);
            let end = start.saturating_add(u32_of_usize(lines.len())); // exclusive
            let from = low.max(start);
            let to = high.saturating_add(1).min(end).max(from);
            (
                from,
                lines[(from - start) as usize..(to - start) as usize].to_vec(),
            )
        };
        if let Some(block) = open.as_mut() {
            let end = block.start.saturating_add(u32_of_usize(block.lines.len())); // exclusive
            let last_match = block.match_lines.last().copied().unwrap_or(0);
            let shared_agrees = (start..end.min(start.saturating_add(u32_of_usize(lines.len()))))
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
