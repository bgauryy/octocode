//! Paragraph joining and the line classes it depends on.

use super::blocks::{FenceState, fence_start, is_fence_close, is_thematic_break};
use super::inline::is_delimiter_row;

/// Joins consecutive paragraph lines into one line. The last output line takes
/// another join while it still classifies as a paragraph line; `run` tracks that
/// class as lines are joined, so each join costs the joined line, not the whole
/// paragraph.
pub(super) fn compact_markdown_newlines(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut fence: Option<FenceState> = None;
    // `Some` while the last output line is a paragraph line.
    let mut run: Option<ParagraphRun> = None;

    for line in lines {
        if let Some(ref active_fence) = fence {
            if is_fence_close(&line, active_fence) {
                fence = None;
            }
            run = None;
            out.push(line);
            continue;
        }

        if let Some(next_fence) = fence_start(&line) {
            fence = Some(next_fence);
            run = None;
            out.push(line);
            continue;
        }

        if line.trim().is_empty() {
            continue;
        }

        if !is_markdown_paragraph_line(&line) {
            run = None;
            out.push(line);
            continue;
        }

        if let Some(active_run) = run.as_mut()
            && let Some(prev) = out.last_mut()
        {
            let segment = line.trim();
            prev.push(' ');
            prev.push_str(segment);
            if !active_run.still_paragraph_after_join(prev, segment) {
                run = None;
            }
            continue;
        }

        run = Some(ParagraphRun::start(&line));
        out.push(line);
    }
    out
}

/// What `is_markdown_paragraph_line` says about a paragraph line as more
/// paragraph lines are joined onto it, updated per joined segment.
///
/// Joined segments are paragraph lines themselves, so they add no `|` and the
/// first line keeps deciding the leading `#`, `>`, `<` and fence checks. Only
/// three checks can flip once text is joined: a list marker completed by the
/// joining space (`1.` + ` text`), a `]:` after an opening `[`, and a thematic
/// break assembled from short marker runs (`--` + ` --`).
struct ParagraphRun {
    opens_with_bracket: bool,
    has_reference_colon: bool,
    /// Thematic-break marker and how many the joined text holds; `None` once
    /// any other non-whitespace character appears.
    thematic_marker: Option<(char, usize)>,
    joined: bool,
}

impl ParagraphRun {
    fn start(first_line: &str) -> Self {
        let t = first_line.trim();
        let thematic_marker = t
            .chars()
            .next()
            .filter(|m| matches!(m, '-' | '_' | '*'))
            .and_then(|m| marker_count(t, m).map(|count| (m, count)));
        Self {
            opens_with_bracket: t.starts_with('['),
            has_reference_colon: t.contains("]:"),
            thematic_marker,
            joined: false,
        }
    }

    /// `joined` is the line after appending ` ` and `segment`.
    fn still_paragraph_after_join(&mut self, joined: &str, segment: &str) -> bool {
        // The list marker is settled by the first line plus the joining space,
        // so it only needs checking on the first join.
        if !self.joined {
            self.joined = true;
            if is_markdown_list_item(joined) {
                return false;
            }
        }
        self.has_reference_colon |= segment.contains("]:");
        self.thematic_marker = self
            .thematic_marker
            .and_then(|(m, count)| marker_count(segment, m).map(|more| (m, count + more)));
        let reference_definition = self.opens_with_bracket && self.has_reference_colon;
        let thematic_break = self.thematic_marker.is_some_and(|(_, count)| count >= 3);
        !reference_definition && !thematic_break
    }
}

/// How many `marker` characters `text` holds, or `None` if it holds any other
/// non-whitespace character.
fn marker_count(text: &str, marker: char) -> Option<usize> {
    let mut count = 0;
    for ch in text.chars().filter(|c| !c.is_whitespace()) {
        if ch != marker {
            return None;
        }
        count += 1;
    }
    Some(count)
}

pub(super) fn is_markdown_paragraph_line(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() {
        return false;
    }
    if fence_start(t).is_some()
        || t.starts_with('#')
        || t.starts_with('>')
        || t.starts_with('<')
        || t.starts_with('[') && t.contains("]:")
        || t.contains('|')
        || is_thematic_break(t)
        || is_delimiter_row(t)
        || is_markdown_list_item(t)
    {
        return false;
    }
    true
}

fn is_markdown_list_item(line: &str) -> bool {
    let t = line.trim_start();
    if t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ ") {
        return true;
    }

    let mut saw_digit = false;
    for (idx, ch) in t.char_indices() {
        if ch.is_ascii_digit() {
            saw_digit = true;
            continue;
        }
        return saw_digit
            && matches!(ch, '.' | ')')
            && t[idx + ch.len_utf8()..].starts_with(char::is_whitespace);
    }
    false
}

pub(super) fn append_md(out: &mut Vec<String>, line: &str, preserve_blank: bool) {
    if line.trim().is_empty() {
        if preserve_blank || out.last().is_some_and(|l| !l.trim().is_empty()) {
            out.push(String::new());
        }
    } else {
        out.push(line.to_owned());
    }
}
