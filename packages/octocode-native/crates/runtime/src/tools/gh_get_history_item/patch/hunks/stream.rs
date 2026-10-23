//! A patch's diff-line stream: its hunks parsed, and widened with the
//! head text between and around them.
use super::{DIFF_CONTEXT_LINES, hunk_side};

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
