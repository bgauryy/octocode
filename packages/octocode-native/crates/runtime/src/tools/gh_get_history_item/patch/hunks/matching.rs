//! A `matchString` patch view: the diff lines holding the needle, their
//! context runs, preserving complete line text.
use super::stream::{DiffLine, parse_hunks, split_hunks, with_head_text};

/// A `matchString` patch view and the windows its context could not reach.
pub(in crate::tools::gh_get_history_item::patch) struct HunkMatch {
    /// The numbered view ([`super::number_patch`]'s gutter).
    pub(in crate::tools::gh_get_history_item::patch) text: String,
    pub(in crate::tools::gh_get_history_item::patch) clipped: Vec<(usize, usize)>,
    /// Line indices in `text` of the hit lines, ascending.
    pub(in crate::tools::gh_get_history_item::patch) hit_lines: Vec<usize>,
}

/// A `matchString` patch view: only the diff lines containing `needle`
/// (lowercase) plus `context` lines around them, numbered as
/// [`super::number_patch`] numbers a whole patch, line text verbatim. Each
/// run shows the hunk headers of the hunks it reads exactly as the diff
/// has them, never a header recomputed for the run: the run opens under the
/// header of its first diff line's hunk, and a later hunk's header precedes
/// that hunk's first line in the run. The shown window is the run of line
/// numbers. A hunk's header prints once: a second run in the same hunk
/// follows a `--` line. With `head` (the file at the PR head) a run reaches past its
/// hunk into unchanged lines, numbered at the head; without it, a run its
/// hunk cuts short of `context` names the new-side window it wanted
/// (`clipped`). `None` when no diff line matches (the file matched by
/// path), so the caller keeps the whole patch.
pub(in crate::tools::gh_get_history_item::patch) fn matching_hunks(
    patch: &str,
    needle: &crate::tools::gh_get_history_item::matcher::Matcher,
    context: (usize, usize),
    head: Option<&[String]>,
) -> Option<HunkMatch> {
    let hunks = parse_hunks(patch);
    let headers: Vec<&str> = hunks.iter().map(|hunk| hunk.header).collect();
    let (segments, cut_edges) = match head {
        Some(head) => match with_head_text(hunks, head) {
            Ok(stream) => (vec![stream], vec![(false, false)]),
            Err(hunks) => split_hunks(hunks),
        },
        None => split_hunks(hunks),
    };
    let mut out = String::new();
    let mut clipped: Vec<(usize, usize)> = Vec::new();
    let mut hit_lines = Vec::new();
    let mut line_index = 0usize;
    // The hunk whose header the view printed last: a hunk's header prints
    // once, and a later run in the same hunk follows a `--` line.
    let mut under = None;
    for (lines, (cut_above, cut_below)) in segments.iter().zip(cut_edges) {
        let is_hit = |line: &DiffLine<'_>| line.diff && needle.matches(&line.text);
        let hits = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| is_hit(line))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for hit in hits {
            let (from, to) = (
                hit.saturating_sub(context.0),
                (hit + context.1).min(lines.len() - 1),
            );
            if (cut_above && hit < context.0) || (cut_below && hit + context.1 >= lines.len()) {
                let at = lines[hit..]
                    .iter()
                    .find_map(|line| line.new)
                    .unwrap_or_else(|| {
                        lines[..hit]
                            .iter()
                            .rev()
                            .find_map(|line| line.new)
                            .map_or(1, |n| n + 1)
                    });
                clipped.push((at.saturating_sub(context.0).max(1), at + context.1));
            }
            match runs.last_mut() {
                Some(run) if from <= run.1 + 1 => run.1 = run.1.max(to),
                _ => runs.push((from, to)),
            }
        }
        for (from, to) in runs {
            let run = &lines[from..=to];
            // Head lines above the run's first diff line read under that
            // line's hunk header, the hunk the run opens in.
            let opening = run.iter().find(|line| line.diff).map(|line| line.hunk);
            if opening.is_some() && opening == under {
                end_line(&mut out);
                out.push_str(RUN_BREAK);
                line_index += 1;
            }
            for (at, line) in run.iter().enumerate() {
                let hunk = if line.diff {
                    Some(line.hunk)
                } else {
                    opening.filter(|_| at == 0)
                };
                if let Some(hunk) = hunk.filter(|hunk| under != Some(*hunk))
                    && let Some(header) = headers.get(hunk)
                {
                    under = Some(hunk);
                    end_line(&mut out);
                    out.push_str(header);
                    line_index += 1;
                }
                if is_hit(line) {
                    hit_lines.push(line_index);
                }
                end_line(&mut out);
                push_numbered(&mut out, line);
                line_index += 1;
            }
        }
    }
    (!out.is_empty()).then(|| HunkMatch {
        text: out,
        clipped: clipped_ranges(clipped),
        hit_lines,
    })
}

/// The line between two runs of one hunk, as `grep` separates its groups.
const RUN_BREAK: &str = "--";

/// End the view's last line before another one follows: GitHub ends a
/// patch without a final newline.
fn end_line(out: &mut String) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

/// One diff line with [`super::number_patch`]'s gutter: a removed line
/// opens with its old-file number, a `\ No newline` marker with a bare
/// separator, every other line (added, kept, or head text) with its
/// new-file number.
fn push_numbered(out: &mut String, line: &DiffLine<'_>) {
    let number = match line.text.as_bytes().first() {
        Some(b'-') => line.old,
        Some(b'\\') => None,
        _ => line.new,
    };
    if let Some(number) = number {
        out.push_str(&number.to_string());
    }
    out.push(crate::response::numbered::SEPARATOR);
    out.push_str(&line.text);
}

/// Merged inclusive line windows; past a read's range cap, one span.
fn clipped_ranges(mut windows: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    windows.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in windows {
        match merged.last_mut() {
            Some(last) if start <= last.1 + 1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    let max = crate::contracts::query_limits::gh_get_file_content::RANGES_MAX_ITEMS;
    if merged.len() > max
        && let (Some(first), Some(last)) = (merged.first().copied(), merged.last().copied())
    {
        merged = vec![(first.0, last.1)];
    }
    merged
}
