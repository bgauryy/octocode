//! Page text a classifier judges and a line reader addresses: HTML parsed
//! with html5ever (the browser algorithm, so unclosed and misnested markup
//! nests as a browser shows it) and rendered as one line per block, in
//! document order. Headings keep their level as `#` marks, list items a `- `
//! marker, table rows their cells joined by ` | `, and preformatted blocks
//! their exact lines inside ``` fences. Scripts, styles, templates, inline
//! SVG, navigation and footers carry no page text and are dropped.
//!
//! A fragment selects one anchored section: the element whose `id` (or an
//! `<a name>`) equals it. A heading anchor, or an anchor inside or right
//! before a heading, selects through the next heading of the same or a
//! higher level; any other anchor selects its own element's text.
mod dom;
mod markup;
mod sink;
mod walk;

use dom::Dom;
use walk::Walker;

/// Why a page or its anchored section yields no text to judge.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Failure {
    /// The page has no text; `scripted` when it has scripts, which usually
    /// means it builds its content in a browser.
    Empty { scripted: bool },
    /// No element carries the fragment as its anchor.
    MissingAnchor,
    /// This many elements carry the fragment as their anchor.
    AmbiguousAnchor(usize),
    /// The anchored element holds no text.
    EmptySection,
    /// `stop` ended the parse.
    Stopped,
}

/// Input the parser takes between `stop` checks. Tree building is
/// quadratic in nesting depth, so a hostile page is bounded by the call's
/// deadline, checked this often, rather than by its size alone.
const PARSE_CHUNK_BYTES: usize = 8 * 1024;

/// The extracted text of `source` (whole, or the section `fragment`
/// anchors), newline-terminated. `stop` is polled while parsing.
pub(super) fn html(
    source: &str,
    fragment: Option<&str>,
    stop: &dyn Fn() -> bool,
) -> Result<String, Failure> {
    let dom = Dom::parse(source, stop).ok_or(Failure::Stopped)?;
    let body = dom.body();
    let Some(fragment) = fragment else {
        let lines = Walker::new(&dom, None).walk(body);
        if lines.lines.is_empty() {
            return Err(Failure::Empty {
                scripted: dom.has_script(),
            });
        }
        return Ok(join(&lines.lines));
    };
    let anchors = dom.anchors(fragment);
    let target = match anchors.as_slice() {
        [] => return Err(Failure::MissingAnchor),
        [target] => *target,
        many => return Err(Failure::AmbiguousAnchor(many.len())),
    };
    let heading = dom.heading_at_or_above(target);
    let walked = Walker::new(&dom, Some(heading.unwrap_or(target))).walk(body);
    let lines = &walked.lines;
    let (start, end) = walked.span.unwrap_or((0, 0));
    let section = match heading {
        Some(_) => section_from(lines, start),
        // An empty anchor marks the heading that follows it.
        None if start == end && lines.get(start).is_some_and(|line| line.level.is_some()) => {
            section_from(lines, start)
        }
        None => &lines[start.min(lines.len())..end.min(lines.len())],
    };
    if section.is_empty() {
        return Err(Failure::EmptySection);
    }
    Ok(join(section))
}

/// Plain or Markdown text as served, with line endings normalized.
pub(super) fn plain(source: &str) -> Result<String, Failure> {
    let mut text = source.replace("\r\n", "\n").replace('\r', "\n");
    if text.trim().is_empty() {
        return Err(Failure::Empty { scripted: false });
    }
    if !text.ends_with('\n') {
        text.push('\n');
    }
    Ok(text)
}

/// The heading section starting at line `start`: through the line before
/// the next heading of the same or a higher level.
fn section_from(lines: &[Line], start: usize) -> &[Line] {
    let Some(level) = lines.get(start).and_then(|line| line.level) else {
        return &[];
    };
    let end = lines[start + 1..]
        .iter()
        .position(|line| line.level.is_some_and(|other| other <= level))
        .map_or(lines.len(), |offset| start + 1 + offset);
    &lines[start..end]
}

fn join(lines: &[Line]) -> String {
    let mut text = String::new();
    for line in lines {
        text.push_str(&line.text);
        text.push('\n');
    }
    text
}

struct Line {
    text: String,
    /// A heading line's level.
    level: Option<u8>,
}

#[cfg(test)]
mod tests;
