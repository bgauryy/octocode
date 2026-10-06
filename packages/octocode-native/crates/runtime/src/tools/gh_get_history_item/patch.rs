//! Patch views: `matchString` hit runs (widened from the head text), the
//! numbered diff, and the shared patch char window over a file page.
use super::HistoryItemRequest;
use super::inventory::*;
use super::util::{needle, str_at};
use crate::tools::result::remove_nulls;
use serde_json::{Value, json};
use std::collections::HashMap;

/// File text at the pull request's head (`sourceSha`): a `matchString` view
/// widens its hit runs past GitHub's diff context from it. A path maps to
/// `None` when its read failed; its clipped runs are then flagged.
#[derive(Debug, Default)]
pub(super) struct HeadSources(HashMap<String, Option<Vec<String>>>);

impl HeadSources {
    pub(super) fn insert(&mut self, path: String, lines: Option<Vec<String>>) {
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
}

/// Diff lines kept around each `matchString` hit (`contextLines`).
pub(super) fn match_context(query: &HistoryItemRequest) -> usize {
    query.match_context().unwrap_or(MATCH_CONTEXT_LINES)
}

/// Whether a `matchString` view of `patch` needs file text beyond its hunks
/// to hold `context` lines around every hit.
pub(super) fn head_text_needed(patch: &str, needle: &str, context: usize) -> bool {
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
            return PatchView {
                text: number_patch(&found.text),
                narrowed: true,
                clipped: found.clipped,
            };
        }
    }
    PatchView {
        text: number_patch(value),
        narrowed: false,
        clipped: Vec::new(),
    }
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
    let sep = crate::tools::numbered::SEPARATOR;
    let mut out = String::with_capacity(patch.len() + patch.len() / 8);
    let (mut old, mut new) = (0usize, 0usize);
    for line in patch.split_inclusive('\n') {
        if line.starts_with("@@") {
            let start = |sign: char| {
                line.split(' ')
                    .find_map(|side| side.strip_prefix(sign))
                    .and_then(|side| side.split(',').next())
                    .and_then(|start| start.parse().ok())
                    .unwrap_or(0)
            };
            old = start('-');
            new = start('+');
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
/// omitted; `next.readFullPatches` re-reads the whole patches.
pub(super) const MATCH_CONTEXT_LINES: usize = 10;

/// Context lines GitHub puts around each change of a diff.
pub(super) const DIFF_CONTEXT_LINES: usize = 3;

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
    let lower = text.to_lowercase();
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
    // Byte offsets carry over only when lowercasing kept every length.
    let hits = if lower.len() == text.len() && !needle.is_empty() {
        lower
            .match_indices(needle)
            .map(|(at, _)| (at, at + needle.len()))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
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
            let mut sides = rest.split(' ');
            let start = |side: Option<&str>| {
                side.and_then(|s| s.split(',').next())
                    .and_then(|n| n.trim_start_matches(['-', '+']).parse().ok())
                    .unwrap_or(0)
            };
            old = start(sides.next());
            new = start(sides.next());
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
    let (segments, cut_edges) = match head {
        Some(head) => match with_head_text(hunks, head) {
            Ok(stream) => (vec![stream], vec![(false, false)]),
            Err(hunks) => split_hunks(hunks),
        },
        None => split_hunks(hunks),
    };
    let mut out = String::new();
    let mut clipped: Vec<(usize, usize)> = Vec::new();
    for (lines, (cut_above, cut_below)) in segments.iter().zip(cut_edges) {
        let hits = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.diff && line.text.to_lowercase().contains(needle))
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
            let side = |pick: fn(&DiffLine<'_>) -> Option<usize>,
                        after: fn(&DiffLine<'_>) -> usize| {
                let count = run.iter().filter(|line| pick(line).is_some()).count();
                let start = run
                    .iter()
                    .find_map(pick)
                    .unwrap_or_else(|| run.first().map_or(0, after).saturating_sub(1));
                (start, count)
            };
            let (old_start, old_count) = side(|l| l.old, |l| l.new.unwrap_or(0));
            let (new_start, new_count) = side(|l| l.new, |l| l.old.unwrap_or(0));
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            let heading = headings.get(lines[first_hit].hunk).copied().unwrap_or("");
            out.push_str(&format!(
                "@@ -{old_start},{old_count} +{new_start},{new_count} @@{heading}\n"
            ));
            for line in run {
                out.push_str(&clip_line(&line.text, needle));
            }
        }
    }
    (!out.is_empty()).then(|| HunkMatch {
        text: out,
        clipped: clipped_ranges(clipped),
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
    let max = crate::tools::id::query_limits::gh_get_file_content::RANGES_MAX_ITEMS;
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

/// A page of changed files sharing one patch char window.
pub(super) struct PatchPage {
    pub(super) rows: Vec<Value>,
    /// Files whose patch is not fully delivered yet, in page order.
    pub(super) unfinished: Vec<String>,
    /// The page-stream `offset` that continues this window; `None` once
    /// every patch on the page is delivered.
    pub(super) cursor: Option<usize>,
    /// Files this window starts whose `matchString` context the hunks cut
    /// short (no head text), with the new-side windows they wanted.
    pub(super) clipped: Vec<(String, Vec<String>)>,
}

/// Shape a page of changed files. Patches are packed whole, in file order,
/// into one char window over the page's concatenated patch stream: a window
/// ends at the last file it holds whole, so small patches arrive complete;
/// a file no window holds whole is cut, at the last line end the window
/// holds. `offset` is the stream position, so the next window repeats the
/// same file page and completed files are not re-emitted. The first window
/// also lists binary and empty files (metadata only).
pub(super) fn shape_patch_page(
    files: Vec<Value>,
    include_patch: bool,
    query: &HistoryItemRequest,
) -> PatchPage {
    shape_patch_page_reserving(files, include_patch, query, 0)
}

/// [`shape_patch_page`] with `reserve` chars of the window left to other
/// evidence the same row carries (a pull request's `fileSummary`), so the
/// row still fits one response page. The window keeps at least a quarter
/// of itself, so a walk always moves.
pub(super) fn shape_patch_page_reserving(
    files: Vec<Value>,
    include_patch: bool,
    query: &HistoryItemRequest,
    reserve: usize,
) -> PatchPage {
    if !include_patch {
        let rows = files
            .iter()
            .map(|file| {
                let mut row = file_metadata(file);
                inventory_patch_flag(&mut row, file);
                remove_nulls(&mut row);
                row
            })
            .collect();
        return PatchPage {
            rows,
            unfinished: Vec::new(),
            cursor: None,
            clipped: Vec::new(),
        };
    }
    let shaped = files
        .iter()
        .map(|file| {
            file.get("patch").and_then(Value::as_str).map(|patch| {
                history_patch_view(patch, str_at(file, "/filename").unwrap_or(""), query)
            })
        })
        .collect::<Vec<_>>();
    // A `matchString` view narrowed to matching hunks names the whole
    // patch's size: the row marker selects the lossless re-read
    // (`next.readFullPatches`).
    let narrowed = files
        .iter()
        .zip(&shaped)
        .map(|(file, view)| {
            let patch = str_at(file, "/patch")?;
            view.as_ref()
                .is_some_and(|view| view.narrowed)
                .then(|| patch.chars().count())
        })
        .collect::<Vec<_>>();
    let mut clipped = Vec::with_capacity(shaped.len());
    let views = shaped
        .into_iter()
        .map(|view| {
            let view = view.map(|view| {
                clipped.push(view.clipped);
                view.text
            });
            if view.is_none() {
                clipped.push(Vec::new());
            }
            view
        })
        .collect::<Vec<_>>();
    let lengths = views
        .iter()
        .map(|view| view.as_deref().map_or(0, |v| v.chars().count()))
        .collect::<Vec<_>>();
    let total = lengths.iter().sum::<usize>();
    let offset = query.char_offset().unwrap_or(0).min(total);
    let window = match (needle(query), query.char_length()) {
        // A literal search returns many short hit runs: their row headers
        // are small, so the hits may fill the page share without the fixed
        // metadata reserve.
        (Some(_), None) => literal_patch_window(query.auto_page_chars),
        _ => patch_window(query.char_length(), query.auto_page_chars),
    };
    let window = window.saturating_sub(reserve).max(window / 4).max(1);
    // Stream start of every file.
    let starts = lengths
        .iter()
        .scan(0usize, |acc, len| {
            let start = *acc;
            *acc += len;
            Some(start)
        })
        .collect::<Vec<_>>();
    let end = window_end(&views, &starts, &lengths, offset, window);
    let cursor = (0..files.len())
        .any(|i| lengths[i] > 0 && starts[i] + lengths[i] > end)
        .then_some(end);
    let stream = PatchStream {
        views: &views,
        narrowed: &narrowed,
        clipped: &clipped,
        starts: &starts,
        lengths: &lengths,
        offset,
        end,
    };
    let (rows, unfinished, clipped) = stream.rows(&files);
    PatchPage {
        rows,
        unfinished,
        cursor,
        clipped,
    }
}

/// One window over a file page's patch stream.
pub(super) struct PatchStream<'a> {
    pub(super) views: &'a [Option<String>],
    /// Whole-patch sizes of `matchString` views narrowed to hit hunks.
    pub(super) narrowed: &'a [Option<usize>],
    /// New-side windows each view's context could not reach.
    pub(super) clipped: &'a [Vec<String>],
    pub(super) starts: &'a [usize],
    pub(super) lengths: &'a [usize],
    pub(super) offset: usize,
    pub(super) end: usize,
}

impl PatchStream<'_> {
    /// The rows this window shows, the files it leaves unfinished, and the
    /// clipped files it starts.
    #[allow(clippy::type_complexity)]
    pub(super) fn rows(
        &self,
        files: &[Value],
    ) -> (Vec<Value>, Vec<String>, Vec<(String, Vec<String>)>) {
        let (offset, end) = (self.offset, self.end);
        let first_window = offset == 0;
        let mut rows = Vec::new();
        let mut unfinished = Vec::new();
        let mut clipped = Vec::new();
        for (i, (file, view)) in files.iter().zip(self.views).enumerate() {
            let (start, len) = (self.starts[i], self.lengths[i]);
            let mut row = file_metadata(file);
            match view {
                // Binary or oversized files have no patch: nothing to window, so
                // they are reported once, on the first window.
                None => {
                    if !first_window {
                        continue;
                    }
                    match missing_patch_reason(file) {
                        // A pure rename changes no content: its diff is empty.
                        None => row["patch"] = json!(""),
                        Some(reason) => {
                            row["isPartial"] = json!(true);
                            row["terminalLimit"] = json!(true);
                            row["patchUnavailable"] = json!(reason);
                        }
                    }
                }
                Some(_) if len == 0 => {
                    if !first_window {
                        continue;
                    }
                    row["patch"] = json!("");
                }
                Some(view) => {
                    if start + len <= offset {
                        continue; // delivered by an earlier window
                    }
                    let local_start = offset.saturating_sub(start);
                    let local_end = end.saturating_sub(start).min(len);
                    if local_end < len {
                        unfinished.push(str_at(file, "/filename").unwrap_or("").to_owned());
                    }
                    // Files not reached yet are counted (`unfinished`), not
                    // shown as empty placeholder rows.
                    if local_end <= local_start {
                        continue;
                    }
                    if let Some(full) = self.narrowed[i] {
                        row["fullPatchChars"] = json!(full);
                    }
                    if local_start == 0 && !self.clipped[i].is_empty() {
                        row["contextClipped"] = json!(true);
                        clipped.push((
                            str_at(file, "/filename").unwrap_or("").to_owned(),
                            self.clipped[i].clone(),
                        ));
                    }
                    let text = view
                        .chars()
                        .skip(local_start)
                        .take(local_end - local_start)
                        .collect::<String>();
                    row["patch"] = json!(text);
                    let cut = local_end < len;
                    if cut || local_start > 0 {
                        // Per-file coordinates; the page-stream cursor that
                        // `offset` continues rides `PatchPage::cursor`.
                        row["patchPagination"] = json!({"offset":local_start,"length":local_end - local_start,"totalChars":len,"hasMore":cut});
                    }
                }
            }
            remove_nulls(&mut row);
            rows.push(row);
        }
        (rows, unfinished, clipped)
    }
}

/// Where a window that starts at stream position `offset` ends: the last
/// file end it holds whole, unless the next file is larger than a whole
/// window (it would never fit, so it starts at once); a cut inside a file
/// ends after the last line end the window holds, when one does.
pub(super) fn window_end(
    views: &[Option<String>],
    starts: &[usize],
    lengths: &[usize],
    offset: usize,
    window: usize,
) -> usize {
    let total = lengths.iter().sum::<usize>();
    let stream_end = offset.saturating_add(window).min(total);
    if stream_end == total {
        return total;
    }
    let boundary = (0..lengths.len())
        .map(|i| starts[i] + lengths[i])
        .filter(|&file_end| file_end > offset && file_end <= stream_end)
        .max();
    let next_overflows = |at: usize| {
        (0..lengths.len())
            .find(|&i| starts[i] == at && lengths[i] > 0)
            .is_some_and(|i| lengths[i] > window)
    };
    if let Some(at) = boundary.filter(|&at| !next_overflows(at)) {
        return at;
    }
    let Some(cut) =
        (0..lengths.len()).find(|&i| starts[i] < stream_end && starts[i] + lengths[i] > stream_end)
    else {
        return stream_end;
    };
    let from = offset.max(starts[cut]) - starts[cut];
    let to = stream_end - starts[cut];
    views[cut]
        .as_deref()
        .and_then(|view| {
            view.chars()
                .take(to)
                .enumerate()
                .skip(from)
                .filter(|(_, c)| *c == '\n')
                .last()
        })
        .map_or(stream_end, |(at, _)| starts[cut] + at + 1)
}

/// Shape a commit or comparison file page (see [`shape_patch_page`]):
/// the file rows and the page-stream `offset` that continues them.
pub(super) fn shape_files(
    files: Vec<Value>,
    include_patch: bool,
    query: &HistoryItemRequest,
) -> (Value, Option<usize>) {
    let mut page = shape_patch_page(files, include_patch, query);
    for row in &mut page.rows {
        compact_file_header(row);
    }
    (Value::Array(page.rows), page.cursor)
}

/// Name a commit/compare page's patch-stream cursor on its `filePagination`
/// (`nextPatchOffset`), the offset `next.continuePatch` carries.
pub(super) fn attach_patch_cursor(files_pagination: &mut Value, cursor: Option<usize>) {
    if let (Some(cursor), Some(page)) = (cursor, files_pagination.as_object_mut()) {
        page.insert("nextPatchOffset".into(), json!(cursor));
    }
}

/// Patch characters one call carries across a page's files by default,
/// derived from the effective automatic response page
/// (`output.pagination.defaultCharLength`, 1k–50k). Rendered text prints
/// patches verbatim (no escaping), so the default window takes 4/5 of the
/// page less a fixed reserve for the row header and metadata, never below
/// 2/5 of it: a default window plus row metadata fits one response page.
/// An explicit `length` is honoured up to that one-row budget: a larger
/// window would split the row into response `rowPart`s whose patch cursor
/// rides only the first part, so following it would skip the unread parts.
/// A bigger window comes with a bigger response page (`responseLength`).
pub(super) const PATCH_DEFAULT_SHARE: (usize, usize) = (4, 5);

pub(super) const PATCH_DEFAULT_RESERVE: usize = 5_000;

/// Page assumed when the runtime did not supply one (direct callers, tests).
pub(super) const FALLBACK_AUTO_PAGE: usize = 20_000;

pub(super) fn auto_page(auto_page: Option<usize>) -> usize {
    auto_page
        .filter(|page| *page > 0)
        .unwrap_or(FALLBACK_AUTO_PAGE)
}

/// The default window of a `matchString` view: the page share without the
/// fixed reserve.
pub(super) fn literal_patch_window(auto_page_chars: Option<usize>) -> usize {
    let page = auto_page(auto_page_chars);
    let (num, den) = PATCH_DEFAULT_SHARE;
    (page * num / den).max(1)
}

/// The warning of a patch read whose explicit `length` was clamped to
/// one response page; `None` when it fit.
pub(super) fn clamp_warning(query: &HistoryItemRequest) -> Option<String> {
    let length = query.char_length()?;
    let window = patch_window(Some(length), query.auto_page_chars);
    (window < length).then(|| {
        format!(
            "length {length} exceeds one response page; patch windows hold {window} chars. Follow next.continuePatch for the rest."
        )
    })
}

/// Append `text` to a response's `warnings` (success rows keep warnings;
/// prose hints are for empty and error rows).
pub(super) fn push_warning(out: &mut Value, text: String) {
    match out.get_mut("warnings").and_then(Value::as_array_mut) {
        Some(warnings) => warnings.push(json!(text)),
        None => out["warnings"] = json!([text]),
    }
}

/// The patch window of one row: most of its response page. An explicit
/// `length` is clamped to that budget, so one row's window always fits one
/// response page.
pub(super) fn patch_window(char_length: Option<usize>, auto_page_chars: Option<usize>) -> usize {
    let page = auto_page(auto_page_chars);
    let (num, den) = PATCH_DEFAULT_SHARE;
    let budget = (page * num / den)
        .min(page.saturating_sub(PATCH_DEFAULT_RESERVE))
        .max(page * 2 / 5)
        .max(1);
    char_length.map_or(budget, |length| length.clamp(1, budget))
}

#[cfg(test)]
mod tests {
    use super::super::MAX_COLLECTION_PAGE;
    use super::super::filter::*;
    use super::super::promotion::promote_pr_continuations;
    use super::super::window::WindowState;
    use super::*;
    use serde_json::Map;

    fn patch_query(offset: usize) -> HistoryItemRequest {
        patch_request(json!({"offset":offset,"length":2}))
    }

    fn patch_request(fields: serde_json::Value) -> HistoryItemRequest {
        let base = json!({
            "operation":"pullRequest",
            "mainGoal": "test", "reasoning":"test",
            "owner":"a",
            "repo":"b",
            "number":1,
            "minify":"none"
        });
        HistoryItemRequest::from_row(super::super::util::merge(base, fields))
            .expect("patch query fixture should be valid")
    }

    fn file(name: &str, patch: &str) -> Value {
        json!({"filename":name,"status":"modified","patch":patch})
    }

    fn window(fields: Value) -> HistoryItemRequest {
        let base = json!({
            "operation":"commit","mainGoal":"test","reasoning":"test",
            "owner":"a","repo":"b","ref":"abc","sections":["patches"]
        });
        HistoryItemRequest::from_row(super::super::util::merge(base, fields))
            .expect("commit window fixture should be valid")
    }

    /// Follow a commit page's `nextOffset` cursor to the end and return
    /// (calls, per-file reassembled patches, files reported patchless).
    fn follow_commit_page(
        files: &[Value],
        char_length: Option<usize>,
    ) -> (usize, HashMap<String, String>, Vec<String>) {
        let mut offset = 0usize;
        let mut calls = 0;
        let mut patches = HashMap::<String, String>::new();
        let mut patchless = Vec::new();
        loop {
            calls += 1;
            assert!(calls < 1_000, "cursor did not advance");
            let mut fields = json!({"offset":offset});
            if let Some(length) = char_length {
                fields["length"] = json!(length);
            }
            let page = shape_patch_page(files.to_vec(), true, &window(fields));
            for row in &page.rows {
                let name = str_at(row, "/filename").unwrap_or("").to_owned();
                if row.get("patchUnavailable").is_some() {
                    patchless.push(name.clone());
                }
                if let Some(text) = row.get("patch").and_then(Value::as_str) {
                    patches.entry(name).or_default().push_str(text);
                }
            }
            let Some(next) = page.cursor else {
                return (calls, patches, patchless);
            };
            assert!(next > offset, "cursor must advance");
            offset = next;
        }
    }

    /// D5: the page budget is not split evenly across files. Small patches
    /// arrive whole, a patch larger than the window continues inside the same
    /// file, and the call count is the stream length over the window.
    #[test]
    fn commit_patches_pack_whole_files_and_continue_inside_a_large_one() {
        let mut files = (0..29)
            .map(|i| file(&format!("small{i}.rs"), &"s".repeat(300)))
            .collect::<Vec<_>>();
        files.insert(3, file("big.rs", &"b".repeat(35_000)));
        let first = shape_patch_page(files.clone(), true, &window(json!({})));
        // The first three small files arrive whole, not as 266-char slices.
        for row in &first.rows[..3] {
            assert_eq!(row["patch"].as_str().map(str::len), Some(300), "{row}");
            assert!(row.get("patchPagination").is_none(), "{row}");
        }
        let window_chars = patch_window(None, None);
        let big = &first.rows[3];
        assert_eq!(big["patchPagination"]["offset"], 0);
        assert_eq!(big["patchPagination"]["length"], window_chars - 900);
        assert_eq!(first.cursor, Some(window_chars));
        // Files not reached yet ride the continuation, not placeholder rows.
        assert_eq!(first.rows.len(), 4);
        assert_eq!(first.unfinished.len(), 27);

        let second = shape_patch_page(files.clone(), true, &window(json!({"offset":window_chars})));
        // Completed files are not re-emitted; the big patch continues in place.
        assert_eq!(second.rows.len(), 1);
        assert_eq!(second.rows[0]["filename"], "big.rs");
        assert_eq!(
            second.rows[0]["patchPagination"]["offset"],
            window_chars - 900
        );

        let total: usize = 29 * 300 + 35_000;
        let (calls, patches, _) = follow_commit_page(&files, None);
        // Windows end at whole files, so the walk takes at most one call
        // more than the stream over the window.
        assert!(calls <= total.div_ceil(window_chars) + 1, "{calls}");
        assert_eq!(patches["big.rs"], "b".repeat(35_000));
        for i in 0..29 {
            assert_eq!(patches[&format!("small{i}.rs")], "s".repeat(300));
        }
    }

    #[test]
    fn single_file_and_three_hundred_file_commits_stay_lossless() {
        let one = vec![file("only.rs", &"x".repeat(20_000))];
        let (calls, patches, _) = follow_commit_page(&one, None);
        assert_eq!(calls, 20_000usize.div_ceil(patch_window(None, None)));
        assert_eq!(patches["only.rs"], "x".repeat(20_000));

        let many = (0..300)
            .map(|i| file(&format!("f{i}.rs"), &format!("+{i}\n")))
            .collect::<Vec<_>>();
        let page = shape_patch_page(many.clone(), true, &window(json!({})));
        assert_eq!(page.rows.len(), 300);
        assert!(page.unfinished.is_empty());
        assert!(page.cursor.is_none());
        let (calls, patches, _) = follow_commit_page(&many, Some(50));
        assert!(calls > 1);
        for (i, source) in many.iter().enumerate() {
            assert_eq!(
                patches[&format!("f{i}.rs")],
                source["patch"].as_str().unwrap_or("")
            );
        }
    }

    #[test]
    fn binary_files_are_reported_once_and_never_block_the_cursor() {
        let files = vec![
            file("a.rs", &"a".repeat(10)),
            json!({"filename":"logo.png","status":"added"}),
            file("b.rs", &"b".repeat(10)),
            json!({"filename":"tail.bin","status":"added"}),
        ];
        let (calls, patches, patchless) = follow_commit_page(&files, Some(4));
        assert_eq!(calls, 5);
        assert_eq!(patchless, ["logo.png", "tail.bin"]);
        assert_eq!(patches["a.rs"], "a".repeat(10));
        assert_eq!(patches["b.rs"], "b".repeat(10));
        let only_binary = vec![json!({"filename":"logo.png","status":"added"})];
        let page = shape_patch_page(only_binary, true, &window(json!({})));
        assert_eq!(page.rows[0]["patchUnavailable"], "binary");
        assert!(page.unfinished.is_empty());
    }

    #[test]
    fn missing_patches_name_why_github_sent_none() {
        let file = |name: &str, status: &str, additions: u64| json!({"filename":name,"status":status,"additions":additions,"deletions":0});
        assert_eq!(
            missing_patch_reason(&file("src/checker.ts", "modified", 9)),
            Some("tooLarge")
        );
        assert_eq!(
            missing_patch_reason(&file("app/favicon.ICO", "modified", 0)),
            Some("binary")
        );
        assert_eq!(
            missing_patch_reason(&file("src/core.ts", "modified", 0)),
            Some("omitted")
        );
        assert_eq!(
            missing_patch_reason(&file("src/new.rs", "renamed", 0)),
            None
        );
    }

    /// A pull-request patch hop is the same file page at the page-stream
    /// cursor: the page names no path list, only how many files are
    /// unfinished, and following the cursor rebuilds every patch once.
    #[test]
    fn pull_request_patch_hops_continue_the_page_stream() {
        let files = vec![file("short.rs", "ABCD"), file("long.rs", "abcdefgh")];
        let mut offset = 0;
        let mut rebuilt = HashMap::<String, String>::new();
        for calls in 1.. {
            assert!(calls < 100, "cursor did not advance");
            let mut row = json!({});
            let mut pagination = Map::new();
            shape_pr_files(
                &mut row,
                &mut pagination,
                files.clone(),
                WindowState::COMPLETE,
                &patch_query(offset),
                None,
                "all",
                None,
            );
            for file in row["files"].as_array().into_iter().flatten() {
                let path = file["path"].as_str().unwrap_or("").to_owned();
                rebuilt
                    .entry(path)
                    .or_default()
                    .push_str(file["patch"].as_str().unwrap_or(""));
            }
            let Some(patches) = pagination.get("patches") else {
                break;
            };
            assert!(patches.get("files").is_none(), "{patches}");
            assert!(patches["unfinishedFiles"].as_u64() > Some(0), "{patches}");
            let next = patches["nextOffset"].as_u64().unwrap_or(0) as usize;
            assert!(next > offset, "{patches}");
            offset = next;
        }
        assert_eq!(rebuilt["short.rs"], "ABCD");
        assert_eq!(rebuilt["long.rs"], "abcdefgh");
    }

    #[test]
    fn windows_end_at_a_file_boundary_unless_one_file_overflows() {
        let files = vec![file("a.rs", "AB"), file("b.rs", "CDEF"), file("c.rs", "G")];
        let query = |offset: usize| window(json!({"offset":offset,"length":4}));
        let shown = |page: &PatchPage| {
            page.rows
                .iter()
                .map(|row| row["patch"].as_str().unwrap_or("").to_owned())
                .collect::<Vec<_>>()
        };
        // a.rs fits; b.rs does not fit the rest of the window, so it starts
        // on the next call instead of being cut.
        let first = shape_patch_page(files.clone(), true, &query(0));
        assert_eq!(shown(&first), ["AB"]);
        assert_eq!(first.unfinished, ["b.rs", "c.rs"]);
        assert_eq!(first.cursor, Some(2));
        let second = shape_patch_page(files, true, &query(2));
        assert_eq!(shown(&second), ["CDEF"]);
        assert_eq!(second.cursor, Some(6));
        // A first file larger than the window is cut at the window end.
        let big = vec![file("big.rs", "abcdefgh"), file("c.rs", "G")];
        let cut = shape_patch_page(big, true, &query(0));
        assert_eq!(shown(&cut), ["abcd"]);
        assert_eq!(cut.unfinished, ["big.rs", "c.rs"]);
        assert_eq!(cut.cursor, Some(4));
        // A file no window holds whole starts at once after the whole ones.
        let early = vec![file("a.rs", "AB"), file("big.rs", "abcdefgh")];
        let started = shape_patch_page(early, true, &query(0));
        assert_eq!(shown(&started), ["AB", "ab"]);
        assert_eq!(started.unfinished, ["big.rs"]);
        assert_eq!(started.cursor, Some(4));
    }

    /// A patch cut inside a file ends at the last line end the window holds,
    /// so no window splits a diff line; a window with no line end in it is
    /// cut at its end.
    #[test]
    fn a_cut_patch_ends_at_a_line_boundary() {
        let lines = vec![file("a.rs", "+one\n+two\n+three\n")];
        let page = shape_patch_page(lines.clone(), true, &window(json!({"length":12})));
        assert_eq!(page.rows[0]["patch"], "+one\n+two\n");
        assert_eq!(page.cursor, Some(10));
        let rest = shape_patch_page(lines, true, &window(json!({"offset":10,"length":12})));
        assert_eq!(rest.rows[0]["patch"], "+three\n");
        assert_eq!(rest.cursor, None);
        let long = shape_patch_page(
            vec![file("a.rs", "abcdefgh")],
            true,
            &window(json!({"length":4})),
        );
        assert_eq!(long.cursor, Some(4));
    }

    #[test]
    fn selected_missing_path_is_distinct_from_a_valid_selection() {
        let query = patch_query(0);
        let files = vec![json!({
            "filename":"src/lib.rs",
            "status":"modified",
            "patch":"abcd"
        })];
        let shape = |selection: Value| {
            let mut row = json!({});
            let mut pagination = Map::new();
            let no_match = shape_pr_files(
                &mut row,
                &mut pagination,
                files.clone(),
                WindowState::COMPLETE,
                &query,
                selection.as_object(),
                "selected",
                None,
            )
            .no_selected_match;
            (row, no_match)
        };

        let (valid, valid_no_match) = shape(json!({"files":["src/lib.rs"]}));
        assert!(!valid_no_match);
        assert_eq!(valid["files"][0]["path"], "src/lib.rs");

        let (missing, missing_no_match) = shape(json!({"files":["src/missing.rs"]}));
        assert!(missing_no_match);
        assert!(missing.get("files").is_none());

        let incomplete_state = WindowState {
            exhausted: false,
            ..WindowState::COMPLETE
        };
        let mut incomplete = json!({});
        let mut incomplete_pagination = Map::new();
        assert!(
            !shape_pr_files(
                &mut incomplete,
                &mut incomplete_pagination,
                files.clone(),
                incomplete_state,
                &query,
                json!({"files":["src/missing.rs"]}).as_object(),
                "selected",
                None,
            )
            .no_selected_match
        );

        let mut output = json!({"pullRequests":[missing]});
        if missing_no_match {
            output["status"] = json!("empty");
            output["hints"] = json!(["copy a changed file path"]);
        }
        assert_eq!(output["status"], "empty");
        assert!(output.get("errorCode").is_none());
        assert!(
            output["hints"]
                .as_array()
                .is_some_and(|hints| !hints.is_empty())
        );
    }

    #[test]
    fn patch_pagination_counts_unfinished_files_and_continues_the_same_page() {
        let query = patch_query(0);
        let mut row = json!({});
        let mut pagination = Map::new();
        let files = vec![
            json!({"filename":"a.rs","status":"modified","patch":"ABCD"}),
            json!({"filename":"next.rs","status":"modified","patch":"x"}),
            json!({"filename":"b.rs","status":"modified","patch":"abcdef"}),
        ];
        shape_pr_files(
            &mut row,
            &mut pagination,
            files,
            WindowState::COMPLETE,
            &query,
            None,
            "all",
            None,
        );
        assert_eq!(
            pagination["patches"]["unfinishedFiles"], 3,
            "{pagination:?}"
        );
        assert_eq!(pagination["patches"]["nextOffset"], 2);
        let request: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"o","repo":"r","number":5,
            "sections":["patches"],"filePage":2
        }))
        .expect("query");
        let mut out = json!({"pullRequests":[{"contentPagination":{
            "patches": pagination["patches"].clone()
        }}]});
        promote_pr_continuations(&mut out, &request);
        let next = &out["next"]["continuePatch"]["query"]["queries"][0];
        assert_eq!(next["sections"], json!(["patches"]), "{out}");
        assert!(next.get("include").is_none(), "{next}");
        assert_eq!(next["filePage"], 2, "{next}");
        assert_eq!(next["offset"], 2);
        assert_eq!(
            out["next"].as_object().map(|next| next.len()),
            Some(1),
            "{out}"
        );
        crate::contracts::validate_query("ghGetHistoryItem", next.clone())
            .expect("continuePatch is a valid ghGetHistoryItem query");
    }

    /// A patch window fills most of the automatic response page: a 100k-char
    /// patch reads in three calls at the default 50k page, not thirteen.
    #[test]
    fn patch_window_is_one_budget_for_the_whole_page() {
        assert_eq!(patch_window(None, None), 15_000);
        assert_eq!(patch_window(Some(2), None), 2);
        assert_eq!(patch_window(None, Some(50_000)), 40_000);
        assert_eq!(100_071usize.div_ceil(patch_window(None, Some(50_000))), 3);
        // An explicit length wins up to the budget.
        assert_eq!(patch_window(Some(9_000), None), 9_000);
        assert_eq!(patch_window(Some(90_000), None), 15_000);
    }

    /// An explicit `length` above one response page is clamped to the
    /// page's one-row budget: the row never splits into response parts whose
    /// patch cursor would skip the unread parts.
    #[test]
    fn explicit_char_length_is_clamped_to_one_response_page() {
        assert_eq!(patch_window(Some(80_000), Some(20_000)), 15_000);
        assert_eq!(patch_window(Some(50_000), None), 15_000);
        assert_eq!(patch_window(Some(100_000), Some(50_000)), 40_000);
        assert_eq!(patch_window(Some(9_000), Some(50_000)), 9_000);
        assert_eq!(patch_window(Some(150_000), Some(1_000)), 400);
        let patch = "+x\n".repeat(30_000);
        let mut query = window(json!({"length":80_000}));
        query.auto_page_chars = Some(20_000);
        let page = shape_patch_page(vec![file("big.rs", &patch)], true, &query);
        let row = &page.rows[0];
        assert_eq!(
            row["patch"].as_str().map(|p| p.chars().count()),
            Some(15_000)
        );
        assert_eq!(row["patchPagination"]["length"], 15_000);
        assert_eq!(page.cursor, Some(15_000));
    }

    /// A walk with an explicit `length` far above the response page reads
    /// every patch exactly once: each window fits one page, and following
    /// the cursor never skips or repeats a character of a multi-file stream.
    #[test]
    fn oversized_char_length_walk_reads_every_patch_exactly_once() {
        let files = (0..6)
            .map(|i| {
                file(
                    &format!("f{i}.rs"),
                    &format!("+{i}\n").repeat(4_000 + i * 3_000),
                )
            })
            .collect::<Vec<_>>();
        let total = files
            .iter()
            .map(|f| f["patch"].as_str().map_or(0, |p| p.chars().count()))
            .sum::<usize>();
        for length in [25_000usize, 40_000, 80_000, 100_000] {
            let mut offset = 0usize;
            let mut read = HashMap::<String, String>::new();
            let mut calls = 0;
            loop {
                calls += 1;
                assert!(calls < 100, "cursor did not advance");
                let mut query = window(json!({"offset":offset,"length":length}));
                query.auto_page_chars = Some(20_000);
                let page = shape_patch_page(files.clone(), true, &query);
                let mut taken = 0;
                for row in &page.rows {
                    let name = str_at(row, "/filename").unwrap_or("").to_owned();
                    let text = row["patch"].as_str().unwrap_or("");
                    let have = read.entry(name).or_default();
                    let at = row["patchPagination"]["offset"].as_u64().unwrap_or(0);
                    assert_eq!(at as usize, have.chars().count(), "gap or repeat: {row}");
                    have.push_str(text);
                    taken += text.chars().count();
                }
                assert!(
                    taken <= patch_window(None, Some(20_000)),
                    "{length}: {taken}"
                );
                match page.cursor {
                    Some(next) => {
                        assert_eq!(next, offset + taken, "{length}");
                        offset = next;
                    }
                    None => break,
                }
            }
            // Windows end at whole files or line ends: at most one extra
            // call per file over the bare stream.
            assert!(
                calls <= total.div_ceil(patch_window(None, Some(20_000))) + files.len(),
                "{length}: {calls}"
            );
            for source in &files {
                let name = source["filename"].as_str().unwrap_or("");
                assert_eq!(
                    read.get(name).map(String::as_str),
                    source["patch"].as_str(),
                    "{length} {name}"
                );
            }
        }
    }

    /// A patch row's `patchPagination` speaks per-file coordinates only; the
    /// page-stream cursor that `offset` continues rides the page.
    #[test]
    fn patch_rows_report_per_file_offsets_and_the_page_cursor_separately() {
        let files = vec![file("a.rs", &"a".repeat(3)), file("b.rs", &"b".repeat(20))];
        let first = shape_patch_page(files.clone(), true, &window(json!({"length":10})));
        let b = &first.rows[1]["patchPagination"];
        assert_eq!(b["offset"], 0);
        assert_eq!(b["length"], 7);
        assert!(b.get("nextOffset").is_none(), "{b}");
        assert_eq!(first.cursor, Some(10));
        let second = shape_patch_page(files, true, &window(json!({"offset":10,"length":10})));
        let b = &second.rows[0]["patchPagination"];
        assert_eq!(b["offset"], 7);
        assert_eq!(b["length"], 10);
        assert_eq!(second.cursor, Some(20));
        let (rows, cursor) = shape_files(
            vec![file("a.rs", "aaa"), file("b.rs", &"b".repeat(20))],
            true,
            &window(json!({"length":10})),
        );
        assert_eq!(rows[1]["patchPagination"]["length"], 7);
        assert_eq!(cursor, Some(10));
    }

    /// `matchString` narrows a patch to the matching lines plus context under
    /// recomputed hunk headers; a path-only match keeps the whole patch.
    #[test]
    fn match_string_patch_view_keeps_only_matching_hunks() {
        let body = (1..=20)
            .map(|n| format!(" line {n}\r\n"))
            .collect::<String>();
        let patch = format!(
            "@@ -1,22 +1,22 @@ fn main\r\n{body}-old Needle\r\n+new needle\r\n{body}@@ -80,3 +80,3 @@\n x\n-y\n+z"
        );
        let view = matching_hunks(&patch, "needle", 3, None)
            .expect("a line matches")
            .text;
        assert_eq!(
            view,
            "@@ -18,4 +18,4 @@ fn main\n line 18\r\n line 19\r\n line 20\r\n-old Needle\r\n+new needle\r\n line 1\r\n line 2\r\n line 3\r\n"
                .replace("@@ -18,4 +18,4 @@", "@@ -18,7 +18,7 @@")
        );
        assert!(matching_hunks(&patch, "absent", 3, None).is_none());
        // contextLines 0: only the hit lines, under one header per run.
        assert_eq!(
            matching_hunks(&patch, "needle", 0, None)
                .map(|found| found.text)
                .as_deref(),
            Some("@@ -21,1 +21,1 @@ fn main\n-old Needle\r\n+new needle\r\n")
        );
        // A generated one-line diff keeps only the text around each hit.
        let long = format!(
            "@@ -1 +1 @@\n+{}PointerEvent{}\r\n",
            "a".repeat(1_000),
            "b".repeat(1_000)
        );
        let clipped = matching_hunks(&long, "pointerevent", 3, None)
            .expect("hit")
            .text;
        assert_eq!(
            clipped,
            format!(
                "@@ -0,0 +1,1 @@\n+[… 850 chars …]{}PointerEvent{}[… 850 chars …]\r\n",
                "a".repeat(150),
                "b".repeat(150)
            )
        );
        let query = patch_request(json!({"matchString":"NEEDLE","contextLines":3}));
        let page = shape_patch_page(vec![file("src/a.rs", &patch)], true, &query);
        assert_eq!(page.rows[0]["patch"], number_patch(&view));
        assert_eq!(page.rows[0]["fullPatchChars"], patch.chars().count());
    }

    /// Every patch view numbers its new side: kept and added lines carry
    /// their new-file line, removed lines and `\ No newline` markers a bare
    /// tab; headers (and their enclosing-symbol heading) stay verbatim, and
    /// dropping the gutter restores the raw patch.
    #[test]
    fn patches_number_the_new_side_and_keep_hunk_headers() {
        let patch = "@@ -84,4 +84,4 @@ cfg_io_util! {\n \n-    old\r\n+    new\r\n     const X: usize = 1;\n@@ -1 +0,0 @@\n-gone\n\\ No newline at end of file\n@@ -0,0 +7,2 @@ impl A\n+a\n+b";
        let numbered = number_patch(patch);
        assert_eq!(
            numbered,
            "@@ -84,4 +84,4 @@ cfg_io_util! {\n84\t \n85\t-    old\r\n85\t+    new\r\n86\t     const X: usize = 1;\n@@ -1 +0,0 @@\n1\t-gone\n\t\\ No newline at end of file\n@@ -0,0 +7,2 @@ impl A\n7\t+a\n8\t+b"
        );
        let raw = numbered
            .split_inclusive('\n')
            .map(|line| match line.split_once('\t') {
                Some((_, text)) if !line.starts_with("@@") => text,
                _ => line,
            })
            .collect::<String>();
        assert_eq!(raw, patch);
        // Not a hunk stream (a line-filtered `+N:` view, empty): unchanged.
        assert_eq!(number_patch("+3: x"), "+3: x");
        assert_eq!(number_patch(""), "");
    }

    /// A patch-reading page that does not fit one response lists every file
    /// of the page first: status, change counts, hunks, and why a file has
    /// no patch. A page that fits shows the patches alone.
    #[test]
    fn unfinished_patch_pages_open_with_a_per_file_summary() {
        let files = vec![
            json!({"filename":"src/a.rs","status":"modified","additions":2,"deletions":1,"sha":"1",
                   "patch":"@@ -1,2 +1,3 @@ fn a\n x\n+y\n@@ -9 +10 @@\n-p\n+q"}),
            json!({"filename":"src/b.rs","status":"added","additions":900,"deletions":0,"sha":"2"}),
            json!({"filename":"README.md","status":"renamed","additions":1,"deletions":0,"sha":"3",
                   "previous_filename":"OLD.md","patch":"@@ -1 +1,2 @@\n x\n+y"}),
        ];
        let summary = |fields: Value| {
            let mut row = json!({});
            let mut pagination = Map::new();
            shape_pr_files(
                &mut row,
                &mut pagination,
                files.clone(),
                WindowState::COMPLETE,
                &patch_request(fields),
                None,
                "all",
                None,
            );
            row.get("fileSummary").cloned()
        };
        assert_eq!(
            summary(json!({"length":20})),
            Some(json!([
                {"src/": ["M +2 -1 2 hunks a.rs", "A +900 -0 !tooLarge b.rs"]},
                "R +1 -0 1 hunk README.md <- OLD.md"
            ]))
        );
        assert_eq!(summary(json!({})), None);
        // A continuation window does not repeat it.
        assert_eq!(summary(json!({"offset":5,"length":20})), None);
    }

    /// Code hunks are never minified: the default PR view (`minify`
    /// omitted or `standard`) returns every context line, so no row is
    /// marked for a re-read.
    #[test]
    fn patch_rows_are_never_minified() {
        let long = "@@ -1,41 +1,41 @@\n".to_owned()
            + &(1..=40)
                .map(|n| format!(" ctx {n}\n"))
                .chain(["-old\n".to_owned(), "+new\n".to_owned()])
                .collect::<String>();
        for query in [
            patch_request(json!({"minify":"standard"})),
            patch_request(json!({})),
        ] {
            let page = shape_patch_page(vec![file("big.rs", &long)], true, &query);
            assert_eq!(page.rows[0]["patch"], number_patch(&long));
            assert!(!number_patch(&long).contains("\n...\n"));
            assert!(
                page.rows[0].get("fullPatchChars").is_none(),
                "{:?}",
                page.rows
            );
        }
    }

    /// A hunk with GitHub's 3 context lines around a hit at line 53 of a
    /// 100-line file.
    fn hunk_at_53() -> (String, Vec<String>) {
        let patch = "@@ -50,7 +50,7 @@ fn f\n line 50\n line 51\n line 52\n-old 53\n+new needle 53\n line 54\n line 55\n line 56\n".to_owned();
        let head = (1..=100)
            .map(|n| {
                if n == 53 {
                    "new needle 53".to_owned()
                } else {
                    format!("line {n}")
                }
            })
            .collect();
        (patch, head)
    }

    /// contextLines past the hunk's own context: with the head text the run
    /// reaches into the unchanged lines around the hunk (new and old line
    /// numbers kept), so 3 and 10 no longer read the same lines.
    #[test]
    fn match_context_past_the_hunk_widens_from_the_head_text() {
        let (patch, head) = hunk_at_53();
        let found = matching_hunks(&patch, "needle", 10, Some(&head)).expect("hit");
        assert!(found.clipped.is_empty());
        assert!(
            found
                .text
                .starts_with("@@ -44,20 +44,20 @@ fn f\n line 44\n"),
            "{}",
            found.text
        );
        assert!(found.text.ends_with(" line 63\n"), "{}", found.text);
        assert!(found.text.contains("-old 53\n+new needle 53\n"));
        let narrow = matching_hunks(&patch, "needle", 3, Some(&head)).expect("hit");
        assert_ne!(narrow.text, found.text);
        // A head that disagrees with the diff is not this diff's head.
        let mut stale = head.clone();
        stale[52] = "something else".into();
        let fallback = matching_hunks(&patch, "needle", 10, Some(&stale)).expect("hit");
        assert_eq!(fallback.clipped, ["43-63"]);
    }

    /// GitHub ends a patch without a final newline: context widened from
    /// the head text past the last hunk starts on its own line, so every
    /// later line keeps its number (byte-exact numbered view).
    #[test]
    fn head_context_after_the_last_hunk_keeps_line_boundaries() {
        let (patch, head) = hunk_at_53();
        let patch = patch.trim_end_matches('\n');
        let found = matching_hunks(patch, "needle", 6, Some(&head)).expect("hit");
        assert_eq!(
            number_patch(&found.text),
            "@@ -48,12 +48,12 @@ fn f\n48\t line 48\n49\t line 49\n50\t line 50\n51\t line 51\n52\t line 52\n53\t-old 53\n53\t+new needle 53\n54\t line 54\n55\t line 55\n56\t line 56\n57\t line 57\n58\t line 58\n59\t line 59\n"
        );
    }

    /// Without the head text, a run the hunk cuts short is never silently
    /// clipped: it names the new-side window it wanted, and the page flags
    /// the file and lists it for `next.expandContext`.
    #[test]
    fn match_context_past_the_hunk_without_head_text_is_flagged() {
        let (patch, _) = hunk_at_53();
        let found = matching_hunks(&patch, "needle", 10, None).expect("hit");
        assert_eq!(found.clipped, ["43-63"]);
        assert!(head_text_needed(&patch, "needle", 10));
        assert!(!head_text_needed(&patch, "needle", 3));
        let query = patch_request(json!({"matchString":"needle"}));
        let page = shape_patch_page(vec![file("src/a.rs", &patch)], true, &query);
        assert_eq!(page.rows[0]["contextClipped"], true, "{:?}", page.rows);
        assert_eq!(
            page.clipped,
            vec![("src/a.rs".to_owned(), vec!["43-63".to_owned()])]
        );
        let mut sources = HeadSources::default();
        sources.insert("src/a.rs".into(), Some(hunk_at_53().1));
        let widened = HistoryItemRequest {
            head_sources: Some(std::sync::Arc::new(sources)),
            ..query
        };
        let page = shape_patch_page(vec![file("src/a.rs", &patch)], true, &widened);
        assert!(page.clipped.is_empty());
        assert!(page.rows[0].get("contextClipped").is_none());
    }

    /// `matchString` keeps 10 context lines around each hit by default
    /// and pages every hit file of the PR at once, not 30 per call.
    #[test]
    fn match_string_defaults_to_ten_context_lines_and_one_page_of_hit_files() {
        let patch = "@@ -1,3 +1,3 @@\n a\n-old miri\n+new miri\n b\n";
        let query = patch_request(json!({"matchString":"miri"}));
        let view = history_patch_view(patch, "a.rs", &query);
        assert_eq!(
            (view.text.as_str(), view.narrowed, view.clipped.len()),
            (
                "@@ -1,3 +1,3 @@\n1\t a\n2\t-old miri\n2\t+new miri\n3\t b\n",
                true,
                0
            )
        );
        let hits_only = patch_request(json!({"matchString":"miri","contextLines":0}));
        assert_eq!(
            history_patch_view(patch, "a.rs", &hits_only).text,
            "@@ -2,1 +2,1 @@\n2\t-old miri\n2\t+new miri\n"
        );
        assert_eq!(
            file_page_size(&query, true),
            super::super::window::MAX_FILE_BATCHES * MAX_COLLECTION_PAGE
        );
        let explicit = patch_request(json!({"matchString":"miri","pageSize":5}));
        assert_eq!(file_page_size(&explicit, true), 5);
    }

    /// A patch page emits only rows that carry patch text (or say why a
    /// file has none); files not reached yet ride the continuation, not
    /// empty placeholder rows. PR rows carry one compact `stat`.
    #[test]
    fn patch_pages_emit_no_placeholder_rows_and_compact_headers() {
        let files = vec![
            json!({"filename":"a.rs","status":"modified","additions":2,"deletions":1,"patch":"A".repeat(10)}),
            json!({"filename":"b.rs","status":"added","additions":9,"deletions":0,"patch":"B".repeat(10)}),
            json!({"filename":"c.rs","status":"modified","additions":1,"deletions":1,"patch":"C".repeat(10)}),
        ];
        let page = shape_patch_page(
            files.clone(),
            true,
            &patch_request(json!({"offset":0,"length":10})),
        );
        assert_eq!(page.rows.len(), 1, "{:?}", page.rows);
        assert_eq!(page.unfinished, ["b.rs", "c.rs"]);
        assert_eq!(page.cursor, Some(10));
        let mut row = json!({});
        let mut pagination = Map::new();
        shape_pr_files(
            &mut row,
            &mut pagination,
            files,
            WindowState::COMPLETE,
            &patch_request(json!({"offset":0,"length":15})),
            None,
            "all",
            None,
        );
        // b.rs does not fit the rest of the window: it starts on the next
        // page whole instead of as a cut placeholder.
        assert_eq!(
            row["files"],
            json!([{"path":"a.rs","stat":"M +2 -1","patch":"AAAAAAAAAA"}])
        );
        assert_eq!(pagination["patches"]["nextOffset"], 10);
        assert_eq!(pagination["patches"]["unfinishedFiles"], 2);
    }

    /// The review pick names every source file by churn within
    /// one budget, skipping tests, docs and lockfiles.
    #[test]
    fn review_selection_packs_source_files_by_churn() {
        let f =
            |name: &str, changed: u64| json!({"filename":name,"additions":changed,"deletions":0});
        let files = vec![
            f(".changeset/x.md", 8),
            f("pydantic/json_schema.py", 40),
            f("tests/test_counter.py", 400),
            f("pydantic/_known_annotated_metadata.py", 12),
            f("pydantic/fields.py", 90),
            f("uv.lock", 900),
            f("pyproject.toml", 2),
            f("docs/img/diagram.png", 0),
        ];
        assert_eq!(
            review_selection(&files, 15_000),
            [
                "pydantic/fields.py",
                "pydantic/json_schema.py",
                "pydantic/_known_annotated_metadata.py",
                "pyproject.toml"
            ]
        );
        // A tight budget leads with the files that fit whole; the larger
        // ones follow (read through continuePatch), never dropped.
        assert_eq!(
            review_selection(&files, 2_000),
            [
                "pydantic/json_schema.py",
                "pydantic/fields.py",
                "pydantic/_known_annotated_metadata.py",
                "pyproject.toml"
            ]
        );
        assert!(review_selection(&[f("tests/a_test.py", 3)], 15_000).is_empty());
        // Tests named by their language's convention stay out of the review
        // pick wherever they live.
        let polyglot = vec![
            f("pkg/cmd/discussion/client/client_test.go", 3_842),
            f("pkg/cmd/discussion/create/create.go", 120),
            f("src/test/java/com/acme/WidgetTest.java", 300),
            f("src/main/java/com/acme/Latest.java", 10),
            f("spec/models/user_spec.rb", 80),
            f("app/test_api.py", 60),
        ];
        assert_eq!(
            review_selection(&polyglot, 15_000),
            [
                "pkg/cmd/discussion/create/create.go",
                "src/main/java/com/acme/Latest.java"
            ]
        );
        // A literal search fills the page share without the metadata reserve.
        assert_eq!(literal_patch_window(None), 16_000);
        assert_eq!(literal_patch_window(Some(20_000)), 16_000);
    }

    /// Commit rows use the PR header (`path` + `stat`), and a
    /// commit's `files` scope takes paths and globs like a PR's.
    #[test]
    fn commit_rows_are_compact_and_files_scope_them() {
        let (rows, _) = shape_files(
            vec![
                json!({"filename":"src/a.rs","status":"added","additions":3,"deletions":0,"patch":"+a"}),
            ],
            true,
            &window(json!({})),
        );
        assert_eq!(
            rows,
            json!([{"path":"src/a.rs","stat":"A +3 -0","patch":"+a"}])
        );
        let scoped = HistoryItemRequest::from_row(json!({
            "operation":"commit","mainGoal":"g","reasoning":"r","owner":"o","repo":"r",
            "ref":"abc","include":["*.md","src/"]
        }))
        .expect("commit files");
        let scope = PathScope::from_query(&scoped)
            .expect("valid")
            .expect("scope");
        let names = ["src/a.rs", "README.md", "lib/b.rs"]
            .into_iter()
            .filter(|name| scope.matches(&json!({"filename": name})))
            .collect::<Vec<_>>();
        assert_eq!(names, ["src/a.rs", "README.md"]);
    }

    fn inventory_request(fields: Value) -> HistoryItemRequest {
        patch_request(super::super::util::merge(
            json!({"sections":["files"]}),
            fields,
        ))
    }

    fn listed(name: &str, status: &str, additions: u64, deletions: u64, patch: bool) -> Value {
        let mut file = json!({"sha":"1","filename":name,"status":status,
            "additions":additions,"deletions":deletions});
        if patch {
            file["patch"] = json!("@@ -1 +1 @@");
        }
        file
    }

    fn inventory(files: Vec<Value>, query: &HistoryItemRequest) -> (Value, Map<String, Value>) {
        let mut row = json!({});
        let mut pagination = Map::new();
        let scope = InventoryFilter::from_query(query).expect("valid file filter");
        shape_pr_files(
            &mut row,
            &mut pagination,
            files,
            WindowState::COMPLETE,
            query,
            None,
            "none",
            scope.as_ref(),
        );
        (row, pagination)
    }

    /// A patch-free inventory row is one compact string; consecutive files in
    /// one directory share a `{"dir/": [...]}` group so the prefix is written
    /// once. Order, patchless reasons and rename origins survive.
    #[test]
    fn inventory_rows_are_compact_and_group_consecutive_directory_files() {
        let mut renamed = listed("src/new.rs", "renamed", 0, 0, false);
        renamed["previous_filename"] = json!("lib/old.rs");
        let files = vec![
            listed(".eslintrc", "modified", 1, 1, true),
            listed("src/a.ts", "added", 8, 0, true),
            listed("src/checker.ts", "modified", 39_550, 39_342, false),
            renamed,
            listed("src/ui/logo.png", "added", 0, 0, false),
            listed("src/z.ts", "removed", 0, 5, true),
            listed("docs/omitted.md", "modified", 0, 0, false),
        ];
        let (row, pagination) = inventory(files, &inventory_request(json!({})));
        assert_eq!(
            row["files"],
            json!([
                "M +1 -1 .eslintrc",
                {"src/": [
                    "A +8 -0 a.ts",
                    "M +39550 -39342 !tooLarge checker.ts",
                    "R +0 -0 new.rs <- lib/old.rs"
                ]},
                "A +0 -0 !binary src/ui/logo.png",
                "D +0 -5 src/z.ts",
                "M +0 -0 !omitted docs/omitted.md"
            ])
        );
        assert_eq!(pagination["files"]["totalItems"], 7);
    }

    /// `include`/`status`/`minChanges` narrow the inventory by status, path glob or prefix and
    /// change count; totals count matches only. A file GitHub sent without
    /// line counts is not "small": it passes `minChanges`.
    #[test]
    fn file_filter_narrows_by_status_path_and_change_count() {
        let files = || {
            vec![
                listed("src/a.ts", "modified", 3, 1, true),
                listed("src/deep/b.ts", "added", 40, 0, true),
                listed("src/c.rs", "modified", 50, 0, true),
                listed("docs/readme.md", "modified", 9, 9, true),
                listed("src/huge.ts", "modified", 0, 0, false),
            ]
        };
        let names = |fields: Value| {
            let (row, pagination) = inventory(files(), &inventory_request(fields));
            let mut out = Vec::new();
            for item in row["files"].as_array().into_iter().flatten() {
                match item {
                    Value::String(row) => out.push(row.rsplit(' ').next().unwrap_or("").to_owned()),
                    Value::Object(group) => {
                        for (dir, rows) in group {
                            for row in rows.as_array().into_iter().flatten() {
                                let name =
                                    row.as_str().unwrap_or("").rsplit(' ').next().unwrap_or("");
                                out.push(format!("{dir}{name}"));
                            }
                        }
                    }
                    _ => {}
                }
            }
            assert_eq!(pagination["files"]["totalItems"], out.len());
            out
        };
        // "**" spans zero or more directories; "*" stays in one segment.
        assert_eq!(
            names(json!({"include":["src/**/*.ts"]})),
            ["src/a.ts", "src/deep/b.ts", "src/huge.ts"]
        );
        assert_eq!(
            names(json!({"include":["src/*.ts"]})),
            ["src/a.ts", "src/huge.ts"]
        );
        assert_eq!(
            names(json!({"include":["*.ts"]})),
            ["src/a.ts", "src/deep/b.ts", "src/huge.ts"]
        );
        assert_eq!(
            names(json!({"include":["src/deep", "docs/"]})),
            ["src/deep/b.ts", "docs/readme.md"]
        );
        assert_eq!(names(json!({"status":["added"]})), ["src/deep/b.ts"]);
        assert_eq!(
            names(json!({"include":["*.ts"],"status":["modified"],"minChanges":10})),
            ["src/huge.ts"]
        );
        let invalid = inventory_request(json!({"include":["src/[a"]}));
        assert!(InventoryFilter::from_query(&invalid).is_err());
    }

    /// An omitted pageSize sizes a patch-free inventory to the response page
    /// and a patch read to one provider batch (one continuation stream walks
    /// every patch of such a PR); an explicit one may exceed a provider
    /// batch only without patches.
    #[test]
    fn inventory_page_size_fills_the_response_page() {
        let mut query = inventory_request(json!({}));
        query.auto_page_chars = Some(50_000);
        assert_eq!(file_page_size(&query, false), 833);
        assert_eq!(
            file_page_size(&query, true),
            super::super::window::PROVIDER_BATCH
        );
        let small = inventory_request(json!({"pageSize":5}));
        assert_eq!(file_page_size(&small, true), 5);
        query.auto_page_chars = Some(1_000);
        assert_eq!(file_page_size(&query, false), MAX_COLLECTION_PAGE);
        let explicit = inventory_request(json!({"pageSize":1000}));
        assert_eq!(file_page_size(&explicit, false), 1_000);
        assert_eq!(file_page_size(&explicit, true), MAX_COLLECTION_PAGE);
        let files = (0..700)
            .map(|i| listed(&format!("d{}/f{i}.rs", i % 2), "modified", 1, 1, true))
            .collect::<Vec<_>>();
        let (row, pagination) = inventory(files, &explicit);
        assert_eq!(row["files"].as_array().map(Vec::len), Some(700));
        assert_eq!(pagination["files"]["hasMore"], false);
    }

    /// H4: at `defaultCharLength` 1000 the patch window shrinks with the page,
    /// so a page of patches still fits one automatic response page.
    #[test]
    fn patch_window_derives_from_the_effective_auto_page() {
        assert_eq!(patch_window(None, Some(1_000)), 400);
        let mut query = patch_request(json!({"offset":0}));
        query.auto_page_chars = Some(1_000);
        let patch = "+x\n".repeat(2_000);
        let page = shape_patch_page(
            vec![json!({"filename":"a.rs","status":"modified","patch":patch})],
            true,
            &query,
        );
        let shaped = &page.rows[0];
        let text = shaped["patch"].as_str().expect("patch");
        assert!(text.encode_utf16().count() <= 400, "{}", text.len());
        assert_eq!(shaped["patchPagination"]["hasMore"], true);
    }
}
