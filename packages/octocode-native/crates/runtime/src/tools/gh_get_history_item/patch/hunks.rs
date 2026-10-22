//! Patch views: the `matchString` hit runs (widened from the head text) and
//! the numbered diff of one changed file.
use super::super::HistoryItemRequest;
use super::super::text::needle;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use std::collections::HashMap;

/// File text at the pull request's head (`headSha`): a `matchString` view
/// widens its hit runs past GitHub's diff context from it. A path maps to
/// `None` when its read failed; its clipped runs are then flagged.
#[derive(Debug, Default)]
pub(in crate::tools::gh_get_history_item) struct HeadSources(HashMap<String, Option<Vec<String>>>);

impl HeadSources {
    pub(in crate::tools::gh_get_history_item) fn insert(
        &mut self,
        path: String,
        lines: Option<Vec<String>>,
    ) {
        self.0.insert(path, lines);
    }
    pub(super) fn lines(&self, path: &str) -> Option<&[String]> {
        self.0.get(path)?.as_deref()
    }
}

/// One changed file's patch view.
pub(super) struct PatchView {
    pub(super) text: String,
    /// A `matchString` view narrowed to its hit runs.
    pub(super) narrowed: bool,
    /// New-side windows (`start-end`) the requested context reaches past
    /// the hunks without head text to fill them.
    pub(super) clipped: Vec<String>,
    /// Char offsets in `text` of the lines a `matchString` view shows as
    /// hits (diff lines holding the needle); empty for any other view.
    pub(super) hits: Vec<usize>,
}

/// Diff lines kept around each `matchString` hit (`contextLines`).
pub(in crate::tools::gh_get_history_item) fn match_context(query: &HistoryItemRequest) -> usize {
    gh_get_history_item_query::match_context(query).unwrap_or(MATCH_CONTEXT_LINES)
}

/// Whether a `matchString` view of `patch` needs file text beyond its hunks
/// to hold `context` lines around every hit.
pub(in crate::tools::gh_get_history_item) fn head_text_needed(
    patch: &str,
    needle: &str,
    context: usize,
) -> bool {
    matching_hunks(patch, needle, context, None).is_some_and(|found| !found.clipped.is_empty())
}

/// The patch view of one changed file: a `matchString` view keeps only the
/// hit runs (widened from the head text when the hunks hold too little
/// context); every other read is the whole patch. Code hunks are never
/// minified. Either view is numbered (see [`number_patch`]).
pub(super) fn history_patch_view(value: &str, path: &str, query: &HistoryItemRequest) -> PatchView {
    if let Some(needle) = needle(query) {
        let source = query
            .head_sources
            .as_deref()
            .and_then(|sources| sources.lines(path));
        if let Some(found) = matching_hunks(value, &needle, match_context(query), source) {
            let text = number_patch(&found.text);
            let hits = line_starts(&text, &found.hit_lines);
            return PatchView {
                text,
                narrowed: true,
                clipped: found.clipped,
                hits,
            };
        }
    }
    PatchView {
        text: number_patch(value),
        narrowed: false,
        clipped: Vec::new(),
        hits: Vec::new(),
    }
}

/// Char offsets of the given (sorted) line indices in `text`.
fn line_starts(text: &str, lines: &[usize]) -> Vec<usize> {
    let mut wanted = lines.iter().peekable();
    let mut starts = Vec::with_capacity(lines.len());
    let mut at = 0usize;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        while wanted.next_if(|&&want| want < index).is_some() {}
        if wanted.next_if(|&&want| want == index).is_some() {
            starts.push(at);
        }
        at += line.chars().count();
    }
    starts
}

/// The `(start, count)` of a hunk header's `mark` side (`-` or `+`): the
/// one hunk-header parser every patch view numbers its lines by.
pub(in crate::tools::gh_get_history_item) fn hunk_side(
    header: &str,
    mark: char,
) -> Option<(usize, usize)> {
    let side = header.split(' ').find_map(|part| part.strip_prefix(mark))?;
    let mut numbers = side.split(',').map(str::parse::<usize>);
    let start = numbers.next()?.ok()?;
    let count = match numbers.next() {
        Some(count) => count.ok()?,
        None => 1,
    };
    Some((start, count))
}

/// Number a unified diff on the side of each line's sign, `cat -n`-like:
/// each kept or added line opens with its new-file line number and a tab
/// (`86\t ctx`, `87\t+added`), each removed line with its old-file line
/// number (`85\t-removed`), and a `\ No newline` marker with a bare tab.
/// So `+`/` ` numbers are ranges at the change's commit and `-` numbers are
/// ranges at its parent. `@@ -a,b +c,d @@ heading` lines stay as GitHub sent
/// them (the heading is the enclosing symbol). Text that does not open with
/// a hunk header (empty, or already a line-filtered `+N:` view) is returned
/// unchanged. Removing the gutter (up to the first tab of each non-header
/// line) restores the raw patch.
pub(super) fn number_patch(patch: &str) -> String {
    if !patch.starts_with("@@") {
        return patch.to_owned();
    }
    let sep = crate::response::numbered::SEPARATOR;
    let mut out = String::with_capacity(patch.len() + patch.len() / 8);
    let (mut old, mut new) = (0usize, 0usize);
    for line in patch.split_inclusive('\n') {
        if line.starts_with("@@") {
            old = hunk_side(line, '-').map_or(0, |(start, _)| start);
            new = hunk_side(line, '+').map_or(0, |(start, _)| start);
            out.push_str(line);
            continue;
        }
        match line.as_bytes().first() {
            Some(b'\\') => {}
            Some(b'-') => {
                out.push_str(&old.to_string());
                old += 1;
            }
            Some(b'+') => {
                out.push_str(&new.to_string());
                new += 1;
            }
            _ => {
                out.push_str(&new.to_string());
                old += 1;
                new += 1;
            }
        }
        out.push(sep);
        out.push_str(line);
    }
    out
}

/// Diff lines kept around each `matchString` hit when `contextLines` is
/// omitted (contract `limits.defaultReadContextLines`);
/// `next.readFullPatches` re-reads the whole patches.
pub(super) const MATCH_CONTEXT_LINES: usize = crate::id::DEFAULT_READ_CONTEXT_LINES;

/// Context lines GitHub puts around each change of a diff.
pub(in crate::tools::gh_get_history_item) const DIFF_CONTEXT_LINES: usize = 3;

/// A `matchString` view clips diff lines longer than this (generated or
/// minified text) to the characters around each hit.
pub(super) const MATCH_LINE_CHARS: usize = 400;

/// Characters kept on each side of a hit inside a clipped line, and at the
/// start of a clipped line without one.
pub(super) const MATCH_LINE_SIDE: usize = 150;

/// Clip a long diff line for a `matchString` view: keep the diff marker, the
/// text around each hit, and its line ending; each cut becomes
/// `[… N chars …]`. Lines up to [`MATCH_LINE_CHARS`] stay verbatim.
pub(super) fn clip_line<'a>(text: &'a str, needle: &str) -> std::borrow::Cow<'a, str> {
    if text.chars().count() <= MATCH_LINE_CHARS {
        return text.into();
    }
    let body_end = text.trim_end_matches(['\r', '\n']).len();
    let floor = |mut at: usize| {
        at = at.min(body_end);
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        at
    };
    let ceil = |mut at: usize| {
        at = at.min(body_end);
        while !text.is_char_boundary(at) {
            at += 1;
        }
        at
    };
    // Hits are found in the lowercase line and mapped back to the chars
    // they came from: lowercasing moves byte offsets char by char.
    let hits = octocode_engine::text::case_fold::Folded::of(text)
        .ranges(needle)
        .into_iter()
        .map(|span| (span.start, span.end))
        .collect::<Vec<_>>();
    let mut keep: Vec<(usize, usize)> = vec![(0, ceil(1))];
    if hits.is_empty() {
        keep.push((0, ceil(MATCH_LINE_SIDE)));
    }
    for (start, end) in hits {
        let span = (
            floor(start.saturating_sub(MATCH_LINE_SIDE)),
            ceil(end + MATCH_LINE_SIDE),
        );
        match keep.last_mut() {
            Some(last) if span.0 <= last.1 => last.1 = last.1.max(span.1),
            _ => keep.push(span),
        }
    }
    let mut out = String::new();
    let mut at = 0;
    for (start, end) in keep {
        let start = start.max(at);
        if start > at {
            out.push_str(&format!("[… {} chars …]", text[at..start].chars().count()));
        }
        out.push_str(&text[start..end.max(start)]);
        at = end.max(start);
    }
    if at < body_end {
        out.push_str(&format!(
            "[… {} chars …]",
            text[at..body_end].chars().count()
        ));
    }
    out.push_str(&text[body_end..]);
    out.into()
}

/// One diff body line: its text (with its line ending), the old/new line
/// numbers it occupies (`None` on the side it is absent from), its hunk, and
/// whether the diff holds it (`false`: head text widening a hunk).
pub(super) struct DiffLine<'a> {
    pub(super) text: std::borrow::Cow<'a, str>,
    pub(super) old: Option<usize>,
    pub(super) new: Option<usize>,
    pub(super) hunk: usize,
    pub(super) diff: bool,
}

/// One `@@ -a,b +c,d @@ heading` hunk.
pub(super) struct Hunk<'a> {
    pub(super) heading: &'a str,
    pub(super) old_start: usize,
    pub(super) new_start: usize,
    pub(super) lines: Vec<DiffLine<'a>>,
}

impl Hunk<'_> {
    /// New-side line after the hunk (a side with no lines starts after
    /// its `start`).
    pub(super) fn after(&self, start: usize, side: fn(&DiffLine<'_>) -> Option<usize>) -> usize {
        let count = self
            .lines
            .iter()
            .filter(|line| side(line).is_some())
            .count();
        if count == 0 { start + 1 } else { start + count }
    }
    /// Whether the file goes on past the hunk: GitHub ends a hunk with its
    /// full trailing context unless the file ends there.
    pub(super) fn file_continues(&self) -> bool {
        let trailing = self
            .lines
            .iter()
            .rev()
            .take_while(|line| line.text.starts_with(' '))
            .count();
        trailing >= DIFF_CONTEXT_LINES && !self.lines.iter().any(|line| line.text.starts_with('\\'))
    }
}

pub(super) fn parse_hunks(patch: &str) -> Vec<Hunk<'_>> {
    let mut hunks: Vec<Hunk<'_>> = Vec::new();
    let (mut old, mut new) = (0usize, 0usize);
    for text in patch.split_inclusive('\n') {
        if let Some(rest) = text.strip_prefix("@@ -") {
            old = hunk_side(text, '-').map_or(0, |(start, _)| start);
            new = hunk_side(text, '+').map_or(0, |(start, _)| start);
            let heading = rest
                .split_once(" @@")
                .map_or("", |(_, heading)| heading)
                .trim_end_matches(['\r', '\n']);
            hunks.push(Hunk {
                heading,
                old_start: old,
                new_start: new,
                lines: Vec::new(),
            });
            continue;
        }
        let index = hunks.len().saturating_sub(1);
        let Some(hunk) = hunks.last_mut() else {
            continue;
        };
        let (at_old, at_new) = match text.as_bytes().first() {
            Some(b'+') => (None, Some(new)),
            Some(b'-') => (Some(old), None),
            Some(b'\\') => (None, None),
            _ => (Some(old), Some(new)),
        };
        old += usize::from(at_old.is_some());
        new += usize::from(at_new.is_some());
        hunk.lines.push(DiffLine {
            text: text.into(),
            old: at_old,
            new: at_new,
            hunk: index,
            diff: true,
        });
    }
    hunks
}

/// The whole new file as one diff-line stream: every hunk in place, the
/// unchanged lines between and around them from `head` (the file at the
/// PR head). `None` when `head` disagrees with a hunk's new side (not the
/// head the diff names), so the caller keeps the hunks alone.
pub(super) fn with_head_text<'a>(
    hunks: Vec<Hunk<'a>>,
    head: &[String],
) -> Result<Vec<DiffLine<'a>>, Vec<Hunk<'a>>> {
    let agrees = hunks.iter().all(|hunk| {
        hunk.new_start > 0
            && hunk.lines.iter().all(|line| match line.new {
                Some(number) => head.get(number.wrapping_sub(1)).is_some_and(|text| {
                    line.text
                        .get(1..)
                        .unwrap_or_default()
                        .trim_end_matches(['\r', '\n'])
                        == text.trim_end_matches('\r')
                }),
                None => true,
            })
    });
    if !agrees || hunks.is_empty() {
        return Err(hunks);
    }
    let unchanged = |number: usize, delta: isize, hunk: usize| DiffLine {
        text: format!(" {}\n", head[number - 1].trim_end_matches('\r')).into(),
        old: number.checked_add_signed(-delta),
        new: Some(number),
        hunk,
        diff: false,
    };
    let mut stream = Vec::new();
    let mut cursor = 1usize;
    let mut delta = 0isize;
    let last = hunks.len() - 1;
    for (index, hunk) in hunks.into_iter().enumerate() {
        let empty_new = hunk.lines.iter().all(|line| line.new.is_none());
        let gap_end = if empty_new {
            hunk.new_start + 1
        } else {
            hunk.new_start
        };
        delta = isize::try_from(hunk.new_start).unwrap_or(0)
            - isize::try_from(hunk.old_start).unwrap_or(0);
        for number in cursor..gap_end.min(head.len() + 1) {
            end_line(&mut stream);
            stream.push(unchanged(number, delta, index));
        }
        let new_after = hunk.after(hunk.new_start, |line| line.new);
        let old_after = hunk.after(hunk.old_start, |line| line.old);
        delta = isize::try_from(new_after).unwrap_or(0) - isize::try_from(old_after).unwrap_or(0);
        cursor = new_after.max(cursor);
        stream.extend(hunk.lines);
    }
    for number in cursor..=head.len() {
        end_line(&mut stream);
        stream.push(unchanged(number, delta, last));
    }
    Ok(stream)
}

/// GitHub ends a patch without a final newline: a head line appended after
/// the last diff line starts a line of its own.
fn end_line(stream: &mut [DiffLine<'_>]) {
    if let Some(line) = stream.last_mut()
        && !line.text.ends_with('\n')
    {
        line.text.to_mut().push('\n');
    }
}

/// A `matchString` patch view and the windows its context could not reach.
pub(super) struct HunkMatch {
    pub(super) text: String,
    pub(super) clipped: Vec<String>,
    /// Line indices in `text` of the hit lines, ascending.
    pub(super) hit_lines: Vec<usize>,
}

/// A `matchString` patch view: only the diff lines containing `needle`
/// (lowercase) plus `context` lines around them, each run under a
/// recomputed `@@ -a,b +c,d @@` header (the original section heading kept),
/// line text verbatim. With `head` (the file at the PR head) a run reaches
/// past its hunk into unchanged lines; without it, a run its hunk cuts short
/// of `context` names the new-side window it wanted (`clipped`). `None`
/// when no diff line matches (the file matched by path), so the caller
/// keeps the whole patch.
pub(super) fn matching_hunks(
    patch: &str,
    needle: &str,
    context: usize,
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
        let is_hit = |line: &DiffLine<'_>| {
            line.diff
                && crate::tools::gh_get_history_item::text::contains_folded(&line.text, needle)
        };
        let hits = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| is_hit(line))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let mut runs: Vec<(usize, usize, usize)> = Vec::new();
        for hit in hits {
            let (from, to) = (
                hit.saturating_sub(context),
                (hit + context).min(lines.len() - 1),
            );
            if (cut_above && hit < context) || (cut_below && hit + context >= lines.len()) {
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
                clipped.push((at.saturating_sub(context).max(1), at + context));
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
                out.push_str(&clip_line(&line.text, needle));
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

/// Each hunk as its own segment, with whether the file goes on above and
/// below it (a run that needs those lines is clipped).
pub(super) fn split_hunks(hunks: Vec<Hunk<'_>>) -> (Vec<Vec<DiffLine<'_>>>, Vec<(bool, bool)>) {
    hunks
        .into_iter()
        .map(|hunk| {
            let edges = (
                hunk.new_start > 1 && hunk.old_start > 1,
                hunk.file_continues(),
            );
            (hunk.lines, edges)
        })
        .filter(|(lines, _)| !lines.is_empty())
        .unzip()
}

/// Merged `start-end` windows; past a read's range cap, one span.
pub(super) fn clipped_ranges(mut windows: Vec<(usize, usize)>) -> Vec<String> {
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
        .into_iter()
        .map(|(start, end)| format!("{start}-{end}"))
        .collect()
}
