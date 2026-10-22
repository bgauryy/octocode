//! Where a patch's hunks sit in another version of their file: the
//! distinctive added lines that find them by text, and the old-side line
//! windows that read them in the parent.
use super::hunks::DIFF_CONTEXT_LINES;

/// Lines `readParent` reads around each hunk's old-side span, so the
/// window shows the enclosing code beyond the diff context.
pub(in crate::tools::gh_get_history_item) const PARENT_WINDOW_PAD: usize = 10;

/// Ranges one `ghGetFileContent` read takes (the contract's `maxItems`).
pub(in crate::tools::gh_get_history_item) const PARENT_WINDOW_RANGES: usize = 10;

/// Literals one `ghGetFileContent` `matchString` list takes (the contract's
/// `maxItems`).
pub(in crate::tools::gh_get_history_item) const MERGE_MATCH_LINES: usize = 10;

/// Trimmed added lines this long locate a hunk well; when any hunk has
/// one, shorter picks (`}`, `x = 1;`) are dropped as matching too much.
pub(in crate::tools::gh_get_history_item) const DISTINCTIVE_LINE_MIN: usize = 12;

/// Longest line a merge read searches for: a minified or generated line is
/// not a locator.
pub(in crate::tools::gh_get_history_item) const DISTINCTIVE_LINE_MAX: usize = 200;

/// One added line per hunk that locates it in another version of the file:
/// the hunk's longest trimmed `+` line of at most [`DISTINCTIVE_LINE_MAX`]
/// chars with three or more letters or digits and no redaction placeholder;
/// when any pick reaches [`DISTINCTIVE_LINE_MIN`] chars the shorter ones
/// are dropped. Duplicates once, at most [`MERGE_MATCH_LINES`].
pub(in crate::tools::gh_get_history_item) fn distinctive_added_lines(patch: &str) -> Vec<String> {
    let mut picks: Vec<String> = Vec::new();
    let mut best: Option<&str> = None;
    let flush = |best: &mut Option<&str>, picks: &mut Vec<String>| {
        if let Some(line) = best.take()
            && !picks.iter().any(|pick| pick == line)
        {
            picks.push(line.to_owned());
        }
    };
    for line in patch.lines() {
        if line.starts_with("@@") {
            flush(&mut best, &mut picks);
            continue;
        }
        if line.starts_with("+++") {
            continue;
        }
        let Some(text) = line.strip_prefix('+').map(str::trim) else {
            continue;
        };
        let chars = text.chars().count();
        let qualifies = chars <= DISTINCTIVE_LINE_MAX
            && text.chars().filter(|c| c.is_alphanumeric()).count() >= 3
            && !text.contains("[REDACTED");
        if qualifies && best.is_none_or(|current| current.chars().count() < chars) {
            best = Some(text);
        }
    }
    flush(&mut best, &mut picks);
    if picks
        .iter()
        .any(|pick| pick.chars().count() >= DISTINCTIVE_LINE_MIN)
    {
        picks.retain(|pick| pick.chars().count() >= DISTINCTIVE_LINE_MIN);
    }
    picks.truncate(MERGE_MATCH_LINES);
    picks
}

/// The old-side span (`-a,b`) of every hunk header, padded by
/// [`PARENT_WINDOW_PAD`] lines and merged where they touch; past
/// [`PARENT_WINDOW_RANGES`] spans, the closest neighbours merge. Old-side
/// lines are context and `-` lines, so the ranges read the parent as the
/// diff numbers it. A pure addition (`-a,0`) has no old-side lines. A whole
/// hunk that opens with the diff's context but ends with less reaches the
/// end of the file, so its span is not padded past it.
pub(in crate::tools::gh_get_history_item) fn parent_windows(patch: &str) -> Vec<String> {
    let mark = '-';
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
            start.saturating_sub(PARENT_WINDOW_PAD).max(1),
            start + count - 1 + if at_end { 0 } else { PARENT_WINDOW_PAD },
        );
        match spans.last_mut() {
            Some(last) if span.0 <= last.1 + 1 => last.1 = last.1.max(span.1),
            _ => spans.push(span),
        }
    }
    while spans.len() > PARENT_WINDOW_RANGES {
        let closest = (1..spans.len())
            .min_by_key(|&i| spans[i].0 - spans[i - 1].1)
            .unwrap_or(1);
        spans[closest - 1].1 = spans[closest].1;
        spans.remove(closest);
    }
    spans
        .into_iter()
        .map(|(start, end)| format!("{start}-{end}"))
        .collect()
}
