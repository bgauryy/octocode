//! `block:true`: widen a line window to its enclosing declaration, so a read
//! that starts at (or inside) a function or class returns
//! through its end instead of stopping mid-body and forcing a re-read. Spans
//! come from the same declaration outline behind `view:"symbols"` and
//! clasify locate; a file type without one keeps the requested window.

mod recovery;

use super::types::{FetchCall, LineRange, NextCalls, local_fetch_query};
use crate::declarations::{self, Declared, all_declarations};
pub(super) use recovery::outline_recovery_warning;
use recovery::recovered_parse;

/// A file's outline as a block read widens with it: the multi-line
/// declarations, and the lines of the one-line ones (nothing to widen to,
/// which is not the same as no declaration at all).
struct Blocks {
    declared: Vec<Declared>,
    one_line: Vec<usize>,
}

impl Blocks {
    /// `None` when the file type has no outline or the outline is empty.
    /// A Markdown file's declarations are its sections ([`sections`]).
    fn of(content: &str, path: &str) -> Option<Self> {
        let (declared, one_line): (Vec<Declared>, Vec<Declared>) = all_declarations(content, path)
            .or_else(|| sections(content, path))?
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

/// A Markdown file's sections, the outline `view:"symbols"` lists
/// ([`octocode_runtime_base::content::markdown_headings`]): each heading
/// through the line before the next heading of the same or a higher level,
/// else the file's last line, so a section holds its subsections.
fn sections(content: &str, path: &str) -> Option<Vec<Declared>> {
    let headings = octocode_runtime_base::content::markdown_headings(content, path)?;
    let last = content.lines().count();
    let sections = headings
        .iter()
        .enumerate()
        .map(|(at, heading)| Declared {
            name: heading.text.clone(),
            line: heading.line,
            start: heading.line,
            end: headings[at + 1..]
                .iter()
                .find(|next| next.level <= heading.level)
                .map_or(last, |next| next.line - 1),
        })
        .collect();
    Some(sections)
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
/// declaration as written. Document files skip this source-comment expansion.
fn block_spans(content: &str, path: &str, blocks: &Blocks) -> Vec<(usize, usize)> {
    let lines: Vec<&str> = content.lines().collect();
    blocks
        .spans()
        .into_iter()
        .map(|span| with_docs(&lines, path, span))
        .collect()
}

/// `span` widened up over the doc comment and attribute run directly above
/// it. Document files keep the provided span.
fn with_docs(lines: &[&str], path: &str, (start, end): (usize, usize)) -> (usize, usize) {
    if matches!(
        octocode_runtime_base::content::classify_file_type(path),
        Some(octocode_runtime_base::content::FileType::Doc)
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

/// The smallest declaration span containing `line`.
fn innermost(spans: &[(usize, usize)], line: usize) -> Option<(usize, usize)> {
    declarations::innermost_by(spans, line, |&span| span).copied()
}

/// The columns of a line's leading whitespace (a tab counts as one).
fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Whether a line only closes a block: `end`, `}`, `)`, `]` (a closer may
/// carry a trailing `;`, `,` or `.method`).
fn closes_block(line: &str) -> bool {
    let text = line.trim();
    text.strip_prefix("end")
        .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
        || text.starts_with(['}', ')', ']'])
}

/// Whether `edge`, a line at or left of a block head's indent, does not end
/// the block but carries its body on: a lone opening brace on its own line
/// (Allman style: `{` under the head), or a closer that opens again
/// (`) {` after a wrapped parameter list, `} else {`). A `)` closer that
/// the next non-blank line answers with such a brace (`)` then `{`) carries
/// on too.
fn body_follows(lines: &[&str], edge: usize) -> bool {
    let text = lines[edge].trim();
    if text.starts_with('{') {
        return !text.contains('}');
    }
    if text.starts_with(['}', ')']) && text.ends_with('{') {
        return true;
    }
    text.starts_with(')')
        && lines[edge + 1..]
            .iter()
            .find(|line| !line.trim().is_empty())
            .is_some_and(|next| next.trim() == "{")
}

/// The 1-based last line of the block that starts at line `start`, judged by
/// indentation alone: the first later non-blank line indented no deeper than
/// the head ends it (a closing `end`/`}` is included, any other line is not).
/// A line at that indent that only opens the body (an Allman `{`, see
/// [`body_follows`]) does not end it: the scan goes on to the line that
/// closes the body. `None` when no later line ends it, so the block's edge is
/// unknown.
fn indented_end(lines: &[&str], start: usize) -> Option<usize> {
    let head = lines.get(start.checked_sub(1)?)?;
    let depth = indent_of(head);
    let mut from = start;
    loop {
        let offset = lines[from..]
            .iter()
            .position(|line| !line.trim().is_empty() && indent_of(line) <= depth)?;
        let edge = from + offset;
        if body_follows(lines, edge) {
            from = edge + 1;
            continue;
        }
        let end = if closes_block(lines[edge]) {
            edge + 1
        } else {
            // The block stops before the edge line and its blank lead-in.
            (start..=edge)
                .rev()
                .find(|line| !lines[line - 1].trim().is_empty())?
        };
        return Some(end.max(start));
    }
}

/// A block read of a file with no declaration outline (no grammar for its
/// language). Each window whose start line has a later line at its indent or
/// less is widened to that indentation-derived edge, and the warning says the
/// edge is an estimate, not parsed. Windows with no such line stay as
/// requested; for them the related read (the `read` slot, published in
/// `related[]`) reads the same file from their first line through its last
/// line, paged, so the declaration that starts there is read to its end
/// whatever spells it.
fn no_outline(
    content: &str,
    path: &str,
    starts: &[usize],
    mut windows: Vec<LineRange>,
    warnings: &mut Vec<String>,
    next: &mut NextCalls,
) -> Vec<LineRange> {
    let lines: Vec<&str> = content.lines().collect();
    let last = lines.len();
    let mut unbounded = vec![];
    let mut estimated = vec![];
    for (window, &start) in windows.iter_mut().zip(starts) {
        match indented_end(&lines, start) {
            Some(end) => {
                window.end = window.end.max(end);
                estimated.push((window.start, window.end));
            }
            None => unbounded.push(start),
        }
    }
    let read = unbounded
        .iter()
        .min()
        .filter(|first| **first <= last)
        .and_then(|first| local_fetch_query::ranges_lead(path, &[(*first as u64, last as u64)]));
    if !estimated.is_empty() {
        let spans: Vec<String> = estimated
            .iter()
            .map(|(first, end)| format!("{first}-{end}"))
            .collect();
        warnings.push(format!(
            "block: no declaration outline for {path}; the block edge is derived from indentation (an estimate, not parsed): lines {}.",
            spans.join(", ")
        ));
    }
    if !unbounded.is_empty() {
        warnings.push(format!(
            "block: no declaration outline for {path}; returned the requested lines{}.{}",
            if read.is_some() {
                "; the related read reads on to the file's end"
            } else {
                ""
            },
            recovered_parse(content, path, None).unwrap_or_default()
        ));
    }
    if let Some(read) = read {
        // A window with no edge is cut short of its declaration.
        next.read = Some(FetchCall::needed(read));
    }
    windows
}

/// Widen each range to the innermost declaration containing its first line,
/// whatever its length: the read's pages bound what one response holds.
/// Each widened declaration is named in `named`.
pub(super) fn widen_ranges(
    content: &str,
    path: &str,
    ranges: Vec<LineRange>,
    warnings: &mut Vec<String>,
    named: &mut Vec<super::types::DeclaredBlock>,
    next: &mut NextCalls,
) -> Vec<LineRange> {
    let Some(blocks) = Blocks::of(content, path) else {
        let starts: Vec<usize> = ranges.iter().map(|range| range.start).collect();
        return no_outline(content, path, &starts, ranges, warnings, next);
    };
    let spans = block_spans(content, path, &blocks);
    let owners: Vec<(&Declared, (usize, usize))> =
        blocks.declared.iter().zip(spans.iter().copied()).collect();
    let (mut unenclosed, mut one_line) = (vec![], vec![]);
    let widened = ranges
        .into_iter()
        .map(|range| {
            let Some(&(owner, (first, last))) =
                declarations::innermost_by(&owners, range.start, |&(_, span)| span)
            else {
                if blocks.is_one_line(range.start) {
                    one_line.push(range.start);
                } else {
                    unenclosed.push(range.start);
                }
                return range;
            };
            name_block(named, owner, first);
            LineRange {
                start: first.min(range.start),
                end: last.max(range.end),
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
            recovered_parse(content, path, None).unwrap_or_default()
        ));
    }
    widened
}

/// Name `owner` in `blocks`, once: a declaration a block read widened to,
/// shown from line `first` (its doc comments and attributes, when above its
/// name line).
fn name_block(blocks: &mut Vec<super::types::DeclaredBlock>, owner: &Declared, first: usize) {
    if !owner.name.is_empty() && !blocks.iter().any(|block| block.line == owner.line) {
        blocks.push(super::types::DeclaredBlock {
            symbol_name: owner.name.clone(),
            line: owner.line,
            end_line: owner.end,
            doc_start_line: (first < owner.line).then_some(first),
        });
    }
}

/// Whether `hit` is a declaration's own head: a line from its first line
/// (its decorators or attributes) through its name line.
fn declares(declared: &[Declared], hit: usize) -> bool {
    declarations::innermost(declared, hit).is_some_and(|d| d.start <= hit && hit <= d.line)
}

/// The warning of a block read whose `contextLines` sized nothing: every
/// match was widened to its declaration.
pub(super) const CONTEXT_UNUSED: &str =
    "contextLines unused: block:true read each match's whole declaration.";

/// Why the call-site `lines` of a block read kept their windows (another
/// match is a declaration head), named with the read that widens them.
fn call_site_warning(lines: &[usize]) -> String {
    match lines {
        [line] => format!(
            "block: line {line} kept its window: it is inside a declaration, not its head, and other matches are heads; a ranges read of it with block:true reads that declaration."
        ),
        _ => format!(
            "block: {} kept their windows: each is inside a declaration, not its head, and other matches are heads; a ranges read of each with block:true reads its declaration.",
            named_lines(lines)
        ),
    }
}

/// Replace each match window with the innermost declaration containing the
/// matched line, whatever its length: the read's pages bound what one
/// response holds. When some hit is a declaration's own head, only those
/// hits widen: a call site of the declared name keeps its window instead of
/// reading its caller whole. Each widened declaration is named in `blocks`.
pub(super) fn widen_matches(
    content: &str,
    path: &str,
    hits: &[usize],
    windows: Vec<LineRange>,
    warnings: &mut Vec<String>,
    blocks: &mut Vec<super::types::DeclaredBlock>,
    next: &mut NextCalls,
) -> Vec<LineRange> {
    let Some(outline) = Blocks::of(content, path) else {
        return no_outline(content, path, hits, windows, warnings, next);
    };
    let spans = outline.spans();
    let declared = &outline.declared;
    let declaration_first = hits.iter().any(|&hit| declares(declared, hit));
    let lines: Vec<&str> = content.lines().collect();
    // A window wider than its hit line holds requested context.
    let context_asked = hits
        .iter()
        .zip(&windows)
        .any(|(&hit, window)| window.start != hit || window.end != hit);
    let mut unenclosed = vec![];
    let mut one_line = 0;
    let mut call_sites = vec![];
    let widened = hits
        .iter()
        .zip(windows)
        .map(|(&hit, window)| match innermost(&spans, hit) {
            Some(_) if declaration_first && !declares(declared, hit) => {
                call_sites.push(hit);
                window
            }
            // A hit on the declaration's head and a hit inside it read the
            // same lines: the declaration as written, docs included, as a
            // `ranges` block read of any of its lines does.
            Some(span) => {
                let (start, end) = with_docs(&lines, path, span);
                if let Some(owner) = declarations::innermost(declared, hit) {
                    name_block(blocks, owner, start);
                }
                LineRange { start, end }
            }
            None if outline.is_one_line(hit) => {
                one_line += 1;
                window
            }
            None => {
                unenclosed.push(hit);
                window
            }
        })
        .collect();
    // Every hit reads its whole declaration, so no context window is left
    // for `contextLines` to size.
    if context_asked && call_sites.len() + one_line + unenclosed.len() == 0 {
        warnings.push(CONTEXT_UNUSED.to_owned());
    }
    // A call site's window is a choice this read made: it names the lines
    // and the read that widens them.
    if !call_sites.is_empty() {
        warnings.push(call_site_warning(&call_sites));
    }
    // A one-line declaration's window already holds it whole, and `blocks`
    // against `matchedLines` shows which hits kept their windows. Only a
    // parse the parser recovered from, before an unenclosed hit, bounds what
    // the read may claim: the outline may miss the declaration enclosing it.
    if let Some(&last) = unenclosed.iter().max()
        && let Some(note) = recovered_parse(content, path, Some(last))
    {
        warnings.push(format!(
            "block: no declaration encloses {}; {} context window.{note}",
            named_lines(&unenclosed),
            if unenclosed.len() == 1 {
                "it kept its"
            } else {
                "each kept its"
            }
        ));
    }
    widened
}

#[cfg(test)]
mod tests;
