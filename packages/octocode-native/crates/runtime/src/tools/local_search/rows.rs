//! Match rows: ranking, projection, and context-window merging.

use super::types::*;

/// Order one clipped file's rows for paging: by lexical hit rank, then
/// distinct text before repeats, stable by line.
pub(super) fn rank_file_rows(matches: &mut Vec<octocode_engine::types::RipgrepMatch>) {
    matches.sort_by_key(|matched| std::cmp::Reverse(matched.rank.unwrap_or(1)));
    let mut seen = std::collections::HashSet::new();
    let (distinct, repeats): (Vec<_>, Vec<_>) = std::mem::take(matches)
        .into_iter()
        .partition(|matched| seen.insert(matched.value.trim().to_owned()));
    matches.extend(distinct);
    matches.extend(repeats);
}

pub(super) fn project_match(
    matched: &octocode_engine::types::RipgrepMatch,
    max_chars: Option<usize>,
) -> SearchMatch {
    // matchOnly display cap: clip the exact span to the public display bound.
    let cut = max_chars.and_then(|limit| {
        matched
            .value
            .char_indices()
            .nth(limit)
            .map(|(byte, _)| (byte, limit))
    });
    if let Some((byte, chars)) = cut {
        return SearchMatch {
            line: matched.line,
            column: Some(crate::tools::num::one_based_column(matched.column)),
            value: matched.value[..byte].into(),
            match_lines: None,
            count: matched.count,
            truncated: true,
            original_chars: Some(matched.value.chars().count()),
            returned_chars: Some(chars),
            enclosing: None,
            declaration: None,
        };
    }
    // Content-view path: the engine already clipped the assembled snippet to
    // maxSnippetChars and reported the pre-truncation length via
    // `original_chars`; surface it as the same truncation indicator.
    let truncated = matched.original_chars.is_some();
    SearchMatch {
        line: matched.line,
        column: Some(crate::tools::num::one_based_column(matched.column)),
        value: matched.value.clone(),
        match_lines: None,
        count: matched.count,
        truncated,
        original_chars: matched.original_chars.map(|chars| chars as usize),
        returned_chars: truncated.then(|| matched.value.chars().count()),
        enclosing: None,
        // The engine's lexical rank 3 is a declared name: the row an
        // lspSearch anchor takes. Only those rows carry the mark.
        declaration: (matched.rank == Some(3)).then_some(true),
    }
}

/// Source line range and lines of a content-view row's ±`context` window, or
/// `None` when the value is not a plain, untruncated window (clipped,
/// redacted, grouped, or a shape the window arithmetic cannot account for).
/// The engine joins up to `context` contiguous lines on each side of the match
/// line, clamped at the file start/end.
pub(super) fn context_window(matched: &SearchMatch, context: u32) -> Option<(u32, Vec<&str>)> {
    if matched.truncated || matched.count.is_some() || matched.line == 0 {
        return None;
    }
    let lines: Vec<&str> = matched.value.split('\n').collect();
    let before = context.min(matched.line - 1);
    let after = u32::try_from(lines.len()).ok()?.checked_sub(before + 1)?;
    (after <= context).then_some((matched.line - before, lines))
}

/// Merge rows whose context windows overlap or touch into one block, so each
/// source line is emitted once, numbered. A merged block keeps the first row's
/// `line`/`column`, and `matchedLines` lists every matched line it holds. Rows
/// merge only when both windows are plain and their shared lines are
/// byte-identical, so a clipped or redacted window is never spliced.
/// Merges overlapping windows while the joined block stays within the
/// `max_chars` (`matchContentLength`) budgets of the rows it joins, so a
/// block is never larger than those rows returned separately.
pub(super) fn merge_context_windows(
    rows: Vec<SearchMatch>,
    context: u32,
    max_chars: usize,
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
        head.value = block
            .lines
            .iter()
            .zip(block.start..)
            .map(|(line, number)| format!("{number}{}{line}", crate::tools::numbered::SEPARATOR))
            .collect::<Vec<_>>()
            .join("\n");
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
mod merge_tests {
    use super::*;

    fn row(line: u32, value: &str) -> SearchMatch {
        SearchMatch {
            line,
            column: None,
            value: value.into(),
            match_lines: None,
            count: None,
            truncated: false,
            original_chars: None,
            returned_chars: None,
            enclosing: None,
            declaration: None,
        }
    }

    #[test]
    fn splices_only_when_shared_lines_agree() {
        // Rows 5 and 6 with ±1 context share lines 5..=6.
        let merged =
            merge_context_windows(vec![row(5, "l4\nl5\nl6"), row(6, "l5\nl6\nl7")], 1, 500);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].value, "4\tl4\n5\tl5\n6\tl6\n7\tl7");
        assert_eq!(merged[0].match_lines, Some(vec![5, 6]));
        // A window ending on a blank line keeps that line numbered.
        let blank = merge_context_windows(vec![row(5, "l4\nl5\n")], 1, 500);
        assert_eq!(blank[0].value, "4\tl4\n5\tl5\n6\t");
        // A redacted/rewritten shared line keeps both rows verbatim.
        let apart = merge_context_windows(
            vec![row(5, "l4\nl5\nl6"), row(6, "l5\n[REDACTED]\nl7")],
            1,
            500,
        );
        assert_eq!(apart.len(), 2);
        assert!(apart.iter().all(|m| m.match_lines.is_none()));
        // A value whose line count cannot be a ±1 window is never spliced.
        let odd = merge_context_windows(vec![row(5, "l4\nl5\nl6"), row(6, "one line")], 1, 500);
        assert_eq!(odd.len(), 2);
        assert_eq!(odd[1].value, "one line");
        // Disjoint windows (gap at line 7..) stay separate rows.
        let gap = merge_context_windows(vec![row(2, "l1\nl2\nl3"), row(9, "l8\nl9\nl10")], 1, 500);
        assert_eq!(gap.len(), 2);
        // A merged block may use the budgets of the rows it replaces (here
        // 2 × 8 chars), so it is never larger than those rows unmerged.
        let within = merge_context_windows(vec![row(5, "l4\nl5\nl6"), row(6, "l5\nl6\nl7")], 1, 8);
        assert_eq!(within.len(), 1, "two 8-char rows merge into 11 chars");
        let capped = merge_context_windows(vec![row(5, "l4\nl5\nl6"), row(6, "l5\nl6\nl7")], 1, 5);
        assert_eq!(
            capped.len(),
            2,
            "a merge must not exceed the merged rows' matchContentLength budgets"
        );
    }

    #[test]
    fn wide_overlapping_windows_merge_up_to_the_rows_combined_budget() {
        // Three ±3 windows over 6-char lines (48 chars each): any two merged
        // exceed one row's 60-char cap, all three fit their combined budget.
        let lines: Vec<String> = (1..=20).map(|n| format!("line{n:02}")).collect();
        let window = |line: u32| {
            let lo = line.saturating_sub(3).max(1) as usize;
            let hi = (line + 3).min(20) as usize;
            lines[lo - 1..hi].join("\n")
        };
        let rows = vec![row(5, &window(5)), row(7, &window(7)), row(9, &window(9))];
        let merged = merge_context_windows(rows, 3, 60);
        assert_eq!(merged.len(), 1, "{merged:?}");
        let numbered = (2..=12)
            .map(|n| format!("{n}\tline{n:02}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(merged[0].value, numbered);
        assert_eq!(merged[0].match_lines, Some(vec![5, 7, 9]));
    }
}
