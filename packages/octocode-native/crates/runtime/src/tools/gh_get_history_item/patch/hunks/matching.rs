//! A `matchString` patch view: the diff lines holding the needle, their
//! context runs, preserving complete line text.
use super::stream::{DiffLine, parse_hunks, split_hunks, with_head_text};

/// A `matchString` patch view and the windows its context could not reach.
pub(in crate::tools::gh_get_history_item::patch) struct HunkMatch {
    pub(in crate::tools::gh_get_history_item::patch) text: String,
    pub(in crate::tools::gh_get_history_item::patch) clipped: Vec<(usize, usize)>,
    /// Line indices in `text` of the hit lines, ascending.
    pub(in crate::tools::gh_get_history_item::patch) hit_lines: Vec<usize>,
}

/// A `matchString` patch view: only the diff lines containing `needle`
/// (lowercase) plus `context` lines around them, each run under a
/// recomputed `@@ -a,b +c,d @@` header (the original section heading kept),
/// line text verbatim. With `head` (the file at the PR head) a run reaches
/// past its hunk into unchanged lines; without it, a run its hunk cuts short
/// of `context` names the new-side window it wanted (`clipped`). `None`
/// when no diff line matches (the file matched by path), so the caller
/// keeps the whole patch.
pub(in crate::tools::gh_get_history_item::patch) fn matching_hunks(
    patch: &str,
    needle: &crate::tools::gh_get_history_item::matcher::Matcher,
    context: (usize, usize),
    head: Option<&[String]>,
) -> Option<HunkMatch> {
    let hunks = parse_hunks(patch);
    let headings: Vec<&str> = hunks.iter().map(|hunk| hunk.heading).collect();
    let starts: Vec<(usize, usize)> = hunks
        .iter()
        .map(|hunk| (hunk.old_start, hunk.new_start))
        .collect();
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
    for (lines, (cut_above, cut_below)) in segments.iter().zip(cut_edges) {
        let is_hit = |line: &DiffLine<'_>| line.diff && needle.matches(&line.text);
        let hits = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| is_hit(line))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let mut runs: Vec<(usize, usize, usize)> = Vec::new();
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
                _ => runs.push((from, to, hit)),
            }
        }
        for (from, to, first_hit) in runs {
            let run = &lines[from..=to];
            // A side with no line in the run starts, as in a unified diff,
            // at its own line before the run: the last one above it, else
            // the next one below less one, else (no line on that side in
            // the hunk) the hunk header's own start.
            let side = |pick: fn(&DiffLine<'_>) -> Option<usize>,
                        header: fn(&(usize, usize)) -> usize| {
                let count = run.iter().filter(|line| pick(line).is_some()).count();
                let start = run.iter().find_map(pick).unwrap_or_else(|| {
                    lines[..from]
                        .iter()
                        .rev()
                        .find_map(pick)
                        .or_else(|| {
                            lines[to + 1..]
                                .iter()
                                .find_map(pick)
                                .map(|next| next.saturating_sub(1))
                        })
                        .unwrap_or_else(|| starts.get(lines[first_hit].hunk).map_or(0, header))
                });
                (start, count)
            };
            let (old_start, old_count) = side(|l| l.old, |start| start.0);
            let (new_start, new_count) = side(|l| l.new, |start| start.1);
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            let heading = headings.get(lines[first_hit].hunk).copied().unwrap_or("");
            out.push_str(&format!(
                "@@ -{old_start},{old_count} +{new_start},{new_count} @@{heading}\n"
            ));
            line_index += 1;
            for line in run {
                if is_hit(line) {
                    hit_lines.push(line_index);
                }
                out.push_str(&line.text);
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
