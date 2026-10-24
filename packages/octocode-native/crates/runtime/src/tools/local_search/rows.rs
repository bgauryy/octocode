//! Match rows: ranking, projection, and context-window merging.

use super::types::*;
use octocode_engine::types::SearchContext;

/// Order one clipped file's rows for paging: by lexical hit rank, then
/// distinct complete values before repeats, stable by line. Display clips
/// never define duplicate identity: a wider expansion keeps the same order.
pub(super) fn rank_file_rows(matches: &mut Vec<octocode_engine::types::TextSearchMatch>) {
    matches.sort_by_key(|matched| std::cmp::Reverse(matched.rank.unwrap_or(1)));
    let mut seen = std::collections::HashSet::new();
    let (distinct, repeats): (Vec<_>, Vec<_>) = std::mem::take(matches)
        .into_iter()
        .partition(|matched| seen.insert(matched.value_digest));
    matches.extend(distinct);
    matches.extend(repeats);
}

pub(super) fn project_match(
    matched: &octocode_engine::types::TextSearchMatch,
    max_chars: Option<usize>,
) -> SearchMatch {
    let (mut row, value) = project_shape(matched, max_chars);
    row.value = value.to_owned();
    row
}

/// A row as [`project_match`] shows it, its value left empty, and the value
/// it shows: a page cut prices the row from both without copying the value.
pub(super) fn project_shape(
    matched: &octocode_engine::types::TextSearchMatch,
    max_chars: Option<usize>,
) -> (SearchMatch, &str) {
    // matchOnly display cap: clip the exact span to the public display bound.
    let cut = max_chars.and_then(|limit| {
        matched
            .value
            .char_indices()
            .nth(limit)
            .map(|(byte, _)| (byte, limit))
    });
    if let Some((byte, chars)) = cut {
        let row = SearchMatch {
            line: matched.line,
            column: Some(crate::num::one_based_column(matched.column)),
            value: String::new(),
            byte_offset: matched.byte_offset,
            match_lines: spanned_lines(matched),
            count: matched.count,
            clipped: true,
            // A value the engine already clipped keeps its full length.
            original_chars: Some(
                matched
                    .original_chars
                    .map_or_else(|| matched.value.chars().count(), |chars| chars as usize),
            ),
            original_bytes: Some(
                matched
                    .original_bytes
                    .map_or_else(|| matched.value.len(), |bytes| bytes as usize),
            ),
            returned_chars: Some(chars),
            enclosing: None,
            declaration: None,
        };
        return (row, &matched.value[..byte]);
    }
    // Content-view path: the engine already clipped the assembled snippet to
    // maxSnippetChars and reported the pre-truncation length via
    // `original_chars`; surface it as the same truncation indicator.
    let clipped = matched.original_chars.is_some();
    let row = SearchMatch {
        line: matched.line,
        column: Some(crate::num::one_based_column(matched.column)),
        value: String::new(),
        byte_offset: matched.byte_offset,
        match_lines: spanned_lines(matched),
        count: matched.count,
        clipped,
        original_chars: matched.original_chars.map(|chars| chars as usize),
        original_bytes: matched.original_bytes.map(|bytes| bytes as usize),
        returned_chars: clipped.then(|| matched.value.chars().count()),
        enclosing: None,
        // The engine's lexical rank 3 is a declared name: the row an
        // lspSearch anchor takes. Only those rows carry the mark.
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
/// `None` when the value is not a plain, untruncated window (clipped,
/// redacted, grouped, or a shape the window arithmetic cannot account for).
/// The engine joins the requested contiguous lines before and after the match,
/// clamped at the file start/end.
pub(super) fn context_window(
    matched: &SearchMatch,
    context: SearchContext,
) -> Option<(u32, Vec<&str>)> {
    if matched.clipped || matched.count.is_some() || matched.line == 0 {
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
/// byte-identical, so a clipped or redacted window is never spliced.
/// Merges overlapping windows while the joined block stays within the
/// `max_chars` (`matchContentLength`) budgets of the rows it joins, so a
/// block is never larger than those rows returned separately.
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
    max_chars: usize,
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
            let end = start + lines.len() as u32; // exclusive
            let from = low.max(start);
            let to = high.saturating_add(1).min(end).max(from);
            (
                from,
                lines[(from - start) as usize..(to - start) as usize].to_vec(),
            )
        };
        if let Some(block) = open.as_mut() {
            let end = block.start + block.lines.len() as u32; // exclusive
            let last_match = block.match_lines.last().copied().unwrap_or(0);
            let shared_agrees = (start..end.min(start + lines.len() as u32))
                .all(|n| block.lines[(n - block.start) as usize] == lines[(n - start) as usize]);
            let fresh = (end.saturating_sub(start) as usize).min(lines.len());
            let grown: usize = block
                .lines
                .iter()
                .map(|l| l.chars().count() + 1)
                .sum::<usize>()
                + lines
                    .iter()
                    .skip(fresh)
                    .map(|l| l.chars().count() + 1)
                    .sum::<usize>();
            if row.line > last_match
                && start >= block.start
                && start <= end
                && shared_agrees
                && grown.saturating_sub(1) <= max_chars.saturating_mul(block.match_lines.len() + 1)
            {
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
