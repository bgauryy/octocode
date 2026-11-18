//! Where a patch's hunks sit in a version of their file: the distinctive
//! added lines that find them by text, and the line windows that read them
//! at either side of the diff.
use super::hunks::DIFF_CONTEXT_LINES;

/// Lines a change read takes around each hunk's span, so the window shows
/// the enclosing code beyond the diff context.
pub(in crate::gh_get_history_item) const HUNK_WINDOW_PAD: usize = 10;

/// Ranges one `ghGetFileContent` read takes (the contract's `maxItems`).
pub(in crate::gh_get_history_item) const HUNK_WINDOW_RANGES: usize = 10;

/// The span of every hunk header on the `mark` side (`-a,b`: the old side,
/// read in the parent; `+c,d`: the new side, read at the change's commit),
/// padded by [`HUNK_WINDOW_PAD`] lines and merged where they touch; past
/// [`HUNK_WINDOW_RANGES`] spans, the closest neighbours merge. A side's lines
/// are context and its own sign's lines, so the ranges read that version as
/// the diff numbers it. A hunk with no line on the side (`-a,0`, `+c,0`)
/// has no span: a side the diff only adds to or only removes from may have
/// none. A whole hunk that opens with the diff's context but ends with less
/// reaches the end of the file, so its span is not padded past it.
pub(in crate::gh_get_history_item) fn hunk_windows(patch: &str, mark: char) -> Vec<(usize, usize)> {
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut lines = patch.lines().peekable();
    while let Some(header) = lines.next() {
        if !header.starts_with("@@") {
            continue;
        }
        let (mut side_lines, mut leading, mut trailing) = (0usize, None, 0usize);
        while let Some(line) = lines.next_if(|line| !line.starts_with("@@")) {
            if line.starts_with(' ') {
                side_lines += 1;
                trailing += 1;
            } else if line.starts_with('+') || line.starts_with('-') {
                side_lines += usize::from(line.starts_with(mark));
                leading.get_or_insert(trailing);
                trailing = 0;
            }
        }
        let Some((start, count)) = super::hunks::hunk_side(header, mark) else {
            continue;
        };
        if count == 0 {
            continue;
        }
        // Only a whole hunk with the diff's context says where the file ends.
        let context = leading == Some(DIFF_CONTEXT_LINES) || start == 1;
        let at_end = side_lines == count && context && trailing < DIFF_CONTEXT_LINES;
        let span = (
            start.saturating_sub(HUNK_WINDOW_PAD).max(1),
            start + count - 1 + if at_end { 0 } else { HUNK_WINDOW_PAD },
        );
        match spans.last_mut() {
            Some(last) if span.0 <= last.1 + 1 => last.1 = last.1.max(span.1),
            _ => spans.push(span),
        }
    }
    while spans.len() > HUNK_WINDOW_RANGES {
        let closest = (1..spans.len())
            .min_by_key(|&i| spans[i].0 - spans[i - 1].1)
            .unwrap_or(1);
        spans[closest - 1].1 = spans[closest].1;
        spans.remove(closest);
    }
    spans
}
