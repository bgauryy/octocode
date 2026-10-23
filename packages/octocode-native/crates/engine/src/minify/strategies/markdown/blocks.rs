//! Block-level lines: fences, thematic breaks, setext headings, generated
//! tables of contents and HTML comments.

use super::inline::{
    is_anchor_only_line, is_badge_line, is_html_break_only_line, is_image_only_line,
};
use super::paragraphs::append_md;

pub(super) struct FenceState {
    marker: char,
    length: usize,
}

pub(super) fn fence_start(line: &str) -> Option<FenceState> {
    let leading = line.len() - line.trim_start().len();
    if leading > 3 {
        return None;
    }
    let rest = line.trim_start();
    let marker = rest.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let length = rest.chars().take_while(|&c| c == marker).count();
    if length >= 3 {
        Some(FenceState { marker, length })
    } else {
        None
    }
}

pub(super) fn is_fence_close(line: &str, f: &FenceState) -> bool {
    let rest = line.trim_start();
    let count = rest.chars().take_while(|&c| c == f.marker).count();
    if count < f.length {
        return false;
    }
    rest[count..].trim().is_empty()
}

pub(super) fn is_thematic_break(line: &str) -> bool {
    let compact: String = line.trim().chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() < 3 {
        return false;
    }
    let Some(m) = compact.chars().next() else {
        return false;
    };
    (m == '-' || m == '_' || m == '*') && compact.chars().all(|c| c == m)
}

pub(super) fn setext_level(line: &str) -> Option<u8> {
    let t = line.trim();
    if t.chars().all(|c| c == '=') && !t.is_empty() {
        Some(1)
    } else if t.chars().all(|c| c == '-') && !t.is_empty() {
        Some(2)
    } else {
        None
    }
}

pub(super) fn convert_setext(out: &mut Vec<String>, level: u8) -> bool {
    let prefix = if level == 1 { "# " } else { "## " };
    let mut heading_lines: Vec<String> = Vec::new();
    while let Some(l) = out.last() {
        if l.trim().is_empty() {
            break;
        }
        if let Some(line) = out.pop() {
            heading_lines.push(line);
        }
    }
    if heading_lines.is_empty() {
        return false;
    }
    heading_lines.reverse();
    let text = heading_lines
        .iter()
        .map(|l| l.trim().to_owned())
        .collect::<Vec<_>>()
        .join(" ");
    let candidate = format!("{}{}", prefix, text);
    append_md(out, &candidate, false);
    true
}

pub(super) fn is_toc_start(line: &str) -> bool {
    regex_like_toc(line).0
}
pub(super) fn is_toc_end(line: &str) -> bool {
    regex_like_toc(line).1
}
fn regex_like_toc(line: &str) -> (bool, bool) {
    let lower = line.to_lowercase();
    let is_end =
        lower.contains("<!-- end") || lower.contains("<!-- /toc") || lower.contains("tocstop");
    let is_start = !is_end
        && (lower.contains("<!-- toc")
            || lower.contains("<!-- table of contents")
            || lower.contains("<!-- doctoc")
            || lower.contains("<!-- markdown-toc"));
    (is_start, is_end)
}

pub(super) fn strip_md_html_comment(line: &str, mut in_comment: bool) -> (String, bool) {
    let mut output = String::new();
    let mut cur = 0usize;
    let bytes = line.as_bytes();
    while cur < bytes.len() {
        if in_comment {
            if let Some(pos) = &line[cur..].find("-->") {
                cur += pos + 3;
                in_comment = false;
            } else {
                return (output, true);
            }
        } else {
            if let Some(pos) = line[cur..].find("<!--") {
                output.push_str(&line[cur..cur + pos]);
                let rest = &line[cur + pos..];
                if let Some(end) = rest.find("-->") {
                    cur += pos + end + 3;
                } else {
                    return (output, true);
                }
            } else {
                output.push_str(&line[cur..]);
                break;
            }
        }
    }
    (output, in_comment)
}

pub(super) fn is_pseudo_comment(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("[//]: #")
}

pub(super) fn is_discardable_md_line(line: &str) -> bool {
    is_badge_line(line)
        || is_image_only_line(line)
        || is_anchor_only_line(line)
        || is_html_break_only_line(line)
}
