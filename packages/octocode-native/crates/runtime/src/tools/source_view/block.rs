//! `block:true`: widen a line window to its enclosing declaration, so a read
//! that starts at (or inside) a function, class, or heading section returns
//! through its end instead of stopping mid-body and forcing a re-read. Spans
//! come from the same declaration outline behind `view:"symbols"` and
//! clasify locate; a file type without one keeps the requested window.

mod recovery;

use super::types::LineRange;
use crate::tools::declarations_cache::{Declared, all_declarations};
pub(super) use recovery::outline_recovery_warning;
use recovery::recovered_parse;

/// Largest declaration a block read returns in one window.
pub const BLOCK_MAX_LINES: usize = 400;

/// The multi-line declarations the engine outlines, in outline order.
pub(crate) fn declarations(content: &str, path: &str) -> Option<Vec<Declared>> {
    let declared: Vec<Declared> = all_declarations(content, path)?
        .into_iter()
        // A one-line declaration is not a block to widen to.
        .filter(|declared| declared.end > declared.start)
        .collect();
    (!declared.is_empty()).then_some(declared)
}

/// A file's outline as a block read widens with it: the multi-line
/// declarations, and the lines of the one-line ones (nothing to widen to,
/// which is not the same as no declaration at all).
struct Blocks {
    declared: Vec<Declared>,
    one_line: Vec<usize>,
}

impl Blocks {
    /// `None` when the file type has no outline or the outline is empty.
    fn of(content: &str, path: &str) -> Option<Self> {
        let (declared, one_line): (Vec<Declared>, Vec<Declared>) = all_declarations(content, path)?
            .into_iter()
            .partition(|declared| declared.end > declared.start);
        if declared.is_empty() && one_line.is_empty() {
            return None;
        }
        let mut one_line: Vec<usize> = one_line.iter().map(|declared| declared.start).collect();
        one_line.sort_unstable();
        one_line.dedup();
        Some(Self { declared, one_line })
    }

    fn spans(&self) -> Vec<(usize, usize)> {
        self.declared.iter().map(|d| (d.start, d.end)).collect()
    }

    fn is_one_line(&self, line: usize) -> bool {
        self.one_line.binary_search(&line).is_ok()
    }
}

/// `lines` as a warning names them: `line 4` or `lines 4, 9`.
fn named_lines(lines: &[usize]) -> String {
    let list = lines
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    match lines {
        [_] => format!("line {list}"),
        _ => format!("lines {list}"),
    }
}

/// Whether a source line is part of the doc comment or attribute run that
/// leads a declaration: a comment in the comment syntax of the file's
/// grammar, an attribute (`#[…]`) or a decorator (`@…`).
fn leads_declaration(path: &str, line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("#[")
        || line.starts_with('@')
        || octocode_engine::signatures::is_comment_line(path, line).unwrap_or(false)
}

/// Declaration spans for a block read: each widened up over the doc
/// comments and attributes directly above it, so the window holds the
/// declaration as written. Document headings keep their own spans.
fn block_spans(content: &str, path: &str, blocks: &Blocks) -> Vec<(usize, usize)> {
    let lines: Vec<&str> = content.lines().collect();
    blocks
        .spans()
        .into_iter()
        .map(|span| with_docs(&lines, path, span))
        .collect()
}

/// `span` widened up over the doc comment and attribute run directly above
/// it; a document heading keeps its span.
fn with_docs(lines: &[&str], path: &str, (start, end): (usize, usize)) -> (usize, usize) {
    if matches!(
        crate::content::classify_file_type(path),
        Some(crate::content::FileType::Doc)
    ) {
        return (start, end);
    }
    let mut first = start;
    while first > 1
        && lines
            .get(first - 2)
            .is_some_and(|line| leads_declaration(path, line))
    {
        first -= 1;
    }
    (first, end)
}

/// The smallest declaration containing `line`.
fn innermost(spans: &[(usize, usize)], line: usize) -> Option<(usize, usize)> {
    spans
        .iter()
        .filter(|(start, end)| *start <= line && line <= *end)
        .min_by_key(|(start, end)| end - start)
        .copied()
}

fn no_outline(content: &str, path: &str) -> String {
    format!(
        "block: no declaration outline for {path}; returned the requested lines.{}",
        recovered_parse(content, path).unwrap_or_default()
    )
}

/// Widen each range to the innermost declaration containing its first line.
/// A declaration over [`BLOCK_MAX_LINES`] keeps the range start and reads at
/// most that many lines of it; the lines it stops before go to `rest`.
pub fn widen_ranges(
    content: &str,
    path: &str,
    ranges: Vec<LineRange>,
    warnings: &mut Vec<String>,
    rest: &mut Vec<LineRange>,
) -> Vec<LineRange> {
    let Some(blocks) = Blocks::of(content, path) else {
        warnings.push(no_outline(content, path));
        return ranges;
    };
    let spans = block_spans(content, path, &blocks);
    let (mut unenclosed, mut one_line) = (vec![], vec![]);
    let widened = ranges
        .into_iter()
        .map(|range| {
            let Some((first, last)) = innermost(&spans, range.start) else {
                if blocks.is_one_line(range.start) {
                    one_line.push(range.start);
                } else {
                    unenclosed.push(range.start);
                }
                return range;
            };
            let start = first.min(range.start);
            let end = last.max(range.end);
            if end + 1 - start <= BLOCK_MAX_LINES {
                return LineRange { start, end };
            }
            let end = range.end.max(last.min(range.start + BLOCK_MAX_LINES - 1));
            if end < last {
                warnings.push(format!(
                    "block: declaration {first}-{last} exceeds {BLOCK_MAX_LINES} lines; returned {}-{end}.",
                    range.start,
                ));
                rest.push(LineRange {
                    start: end + 1,
                    end: last,
                });
            }
            LineRange {
                start: range.start,
                end,
            }
        })
        .collect();
    if !one_line.is_empty() {
        warnings.push(format!(
            "block: {} {} a one-line declaration, nothing to widen; returned the requested lines.",
            named_lines(&one_line),
            if one_line.len() == 1 {
                "holds"
            } else {
                "each hold"
            },
        ));
    }
    if !unenclosed.is_empty() {
        warnings.push(format!(
            "block: no declaration encloses {}; returned the requested lines.{}",
            named_lines(&unenclosed),
            recovered_parse(content, path).unwrap_or_default()
        ));
    }
    widened
}

/// The innermost declaration containing `line`.
fn innermost_declared(declared: &[Declared], line: usize) -> Option<&Declared> {
    declared
        .iter()
        .filter(|d| d.start <= line && line <= d.end)
        .min_by_key(|d| d.end - d.start)
}

/// Whether `hit` is a declaration's own head: its first line or its name
/// line.
fn declares(declared: &[Declared], hit: usize) -> bool {
    innermost_declared(declared, hit).is_some_and(|d| d.start == hit || d.line == hit)
}

/// Replace each match window with the innermost declaration containing the
/// matched line, when that declaration fits [`BLOCK_MAX_LINES`]; a larger
/// one keeps the window and goes to `oversized_spans`. When some hit is a
/// declaration's own head, only those hits widen: a call site of the
/// declared name keeps its window instead of reading its caller whole.
/// Each widened declaration is named in `blocks`.
pub fn widen_matches(
    content: &str,
    path: &str,
    hits: &[usize],
    windows: Vec<LineRange>,
    warnings: &mut Vec<String>,
    oversized_spans: &mut Vec<LineRange>,
    blocks: &mut Vec<super::types::DeclaredBlock>,
) -> Vec<LineRange> {
    let Some(outline) = Blocks::of(content, path) else {
        warnings.push(no_outline(content, path));
        return windows;
    };
    let spans = outline.spans();
    let declared = &outline.declared;
    let declaration_first = hits.iter().any(|&hit| declares(declared, hit));
    let lines: Vec<&str> = content.lines().collect();
    let mut oversized = 0;
    let mut unenclosed = 0;
    let mut one_line = 0;
    let mut call_sites = 0;
    let widened = hits
        .iter()
        .zip(windows)
        .map(|(&hit, window)| match innermost(&spans, hit) {
            Some(_) if declaration_first && !declares(declared, hit) => {
                call_sites += 1;
                window
            }
            // A hit on the declaration's head reads it as written, docs
            // included; a hit inside reads the declaration it sits in.
            Some((start, end)) if end + 1 - start <= BLOCK_MAX_LINES => {
                if let Some(owner) = innermost_declared(declared, hit)
                    && !owner.name.is_empty()
                    && !blocks.iter().any(|block| block.line == owner.line)
                {
                    blocks.push(super::types::DeclaredBlock {
                        symbol_name: owner.name.clone(),
                        line: owner.line,
                        end_line: owner.end,
                    });
                }
                let (start, end) = if hit == start {
                    with_docs(&lines, path, (start, end))
                } else {
                    (start, end)
                };
                LineRange { start, end }
            }
            Some((start, end)) => {
                oversized += 1;
                oversized_spans.push(LineRange { start, end });
                window
            }
            None if outline.is_one_line(hit) => {
                one_line += 1;
                window
            }
            None => {
                unenclosed += 1;
                window
            }
        })
        .collect();
    // `matchedLines` names every hit; the warning carries the count.
    if call_sites > 0 {
        warnings.push(format!(
            "block: {call_sites} match(es) are not declaration heads; they keep their context window, and only the declarations matched by name widen."
        ));
    }
    if one_line > 0 {
        warnings.push(format!(
            "block: {one_line} match(es) are one-line declarations, nothing to widen; those keep their match context window."
        ));
    }
    if unenclosed > 0 {
        warnings.push(format!(
            "block: {unenclosed} match(es) sit outside any declaration; those keep their match context window.{}",
            recovered_parse(content, path).unwrap_or_default()
        ));
    }
    if oversized > 0 {
        warnings.push(format!(
            "block: {oversized} match(es) sit in declarations over {BLOCK_MAX_LINES} lines; those keep their context window."
        ));
    }
    widened
}

#[cfg(test)]
mod tests;
