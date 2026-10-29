//! Source geometry shared by section assessment and declaration-aware readers.
use std::ops::Range;
use std::path::Path;

use octocode_engine::text::LineIndex;

/// Where a source's declarations put chunk edges: the windows of bounded
/// declarations (each one chunk), and the lines where any declaration
/// starts (with its documentation) or ends.
#[derive(Default)]
pub(crate) struct LineEdges {
    pub(crate) windows: Vec<Range<usize>>,
    pub(crate) starts: std::collections::HashSet<usize>,
    pub(crate) ends: std::collections::HashSet<usize>,
}

pub(crate) fn line_edges(text: &str, path: &Path, max_chars: usize) -> LineEdges {
    let Some(declarations) =
        crate::tools::declarations_cache::all_declarations(text, &path.to_string_lossy())
    else {
        return LineEdges::default();
    };
    let mut prefix = vec![0usize];
    for line in text.split_inclusive('\n') {
        prefix.push(
            prefix
                .last()
                .copied()
                .unwrap_or_default()
                .saturating_add(line.chars().count()),
        );
    }
    let first_line = |declaration: &crate::tools::declarations_cache::Declared| {
        declaration
            .doc_start
            .unwrap_or(declaration.start)
            .min(declaration.start)
    };
    let starts = declarations.iter().map(first_line).collect();
    let ends = declarations
        .iter()
        .map(|declaration| declaration.end)
        .collect();
    let mut ranges = declarations
        .iter()
        .filter(|declaration| {
            matches!(
                declaration.kind.as_str(),
                "function" | "method" | "constructor" | "property"
            )
        })
        .filter_map(|declaration| {
            let start = first_line(declaration);
            (start > 0
                && declaration.end >= start
                && declaration.end < prefix.len()
                && prefix[declaration.end] - prefix[start - 1] <= max_chars)
                .then_some(start..declaration.end + 1)
        })
        .collect::<Vec<_>>();
    ranges.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| b.end.cmp(&a.end)));
    let mut windows: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if windows.last().is_none_or(|last| last.end <= range.start) {
            windows.push(range);
        }
    }
    LineEdges {
        windows,
        starts,
        ends,
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Section {
    pub target: Range<usize>,
    pub context: Vec<Range<usize>>,
    pub kind: String,
    pub name: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct Declaration {
    pub bytes: Range<usize>,
    pub heading: Range<usize>,
    pub kind: String,
    pub name: String,
}

/// Parser columns use UTF-16. Convert through the same index as graph/LSP.
pub(crate) fn declarations(source: &str, path: &Path) -> Vec<Declaration> {
    let Some(document) =
        crate::tools::declarations_cache::file_document(source, &path.to_string_lossy())
    else {
        return Vec::new();
    };
    let index = LineIndex::tree_sitter(source);
    let mut declarations = document
        .declarations
        .iter()
        .filter_map(|declaration| {
            let position = |point: &octocode_engine::graph::GraphPosition| {
                index.position_to_byte(source, point.line, point.character) as usize
            };
            let start = position(&declaration.range.start);
            let start = declaration
                .doc_line
                .map_or(start, |line| {
                    index.position_to_byte(source, line, 0) as usize
                })
                .min(start);
            let end = position(&declaration.range.end);
            if start >= end
                || end > source.len()
                || !source.is_char_boundary(start)
                || !source.is_char_boundary(end)
            {
                return None;
            }
            let heading_end = position(&declaration.selection_range.end).clamp(start, end);
            Some(Declaration {
                bytes: start..end,
                heading: start..heading_end,
                kind: declaration.kind.clone(),
                name: declaration.name.clone(),
            })
        })
        .collect::<Vec<_>>();
    declarations.sort_by_key(|declaration| (declaration.bytes.start, declaration.bytes.end));
    declarations
}

/// Every selected byte belongs to exactly one target. A declaration is kept
/// whole when it fits; oversized containers expose nested declarations and
/// their intervening source. Text fallback cuts only at UTF-8 boundaries.
pub(crate) fn visit<E>(
    source: &str,
    path: &Path,
    selected: &[Range<usize>],
    target_chars: usize,
    mut emit: impl FnMut(Section) -> Result<bool, E>,
) -> Result<bool, E> {
    let declarations = declarations(source, path);
    let target_chars = target_chars.max(1);
    let mut cursor = 0;
    let mut active = Vec::new();
    for selection in selected {
        let mut offset = selection.start;
        while offset < selection.end {
            while let Some(declaration) = declarations
                .get(cursor)
                .filter(|declaration| declaration.bytes.start <= offset)
            {
                active.push(declaration);
                cursor += 1;
            }
            active.retain(|declaration| declaration.bytes.end > offset);
            let declaration_start = declarations
                .get(cursor)
                .filter(|declaration| {
                    declaration.bytes.start <= selection.end
                        && trivia_end(source, offset, declaration.bytes.start, target_chars)
                            == declaration.bytes.start
                })
                .map_or(offset, |declaration| declaration.bytes.start);
            let declaration = active
                .iter()
                .copied()
                .chain(
                    declarations[cursor..]
                        .iter()
                        .take_while(|declaration| declaration.bytes.start == declaration_start),
                )
                .filter(|declaration| {
                    declaration.bytes.start == declaration_start
                        && declaration.bytes.end <= selection.end
                        && source[offset..declaration.bytes.end]
                            .chars()
                            .take(target_chars.saturating_add(1))
                            .count()
                            <= target_chars
                })
                .max_by_key(|declaration| declaration.bytes.end);
            let (end, kind, name) = if let Some(declaration) = declaration {
                let next = declarations[cursor..]
                    .iter()
                    .find(|next| next.bytes.start >= declaration.bytes.end)
                    .map_or(selection.end, |next| next.bytes.start.min(selection.end));
                let used = source[offset..declaration.bytes.end].chars().count();
                // Closing delimiters before the next declaration end a
                // container this one is the last of: they travel with it.
                let rest = &source[declaration.bytes.end..next];
                let closes = rest.contains(['}', ')', ']'])
                    && rest
                        .chars()
                        .all(|c| c.is_whitespace() || matches!(c, '}' | ')' | ']' | ';' | ','))
                    && rest.chars().count() <= target_chars - used;
                (
                    if closes {
                        next
                    } else {
                        whitespace_end(source, declaration.bytes.end, next, target_chars - used)
                    },
                    declaration.kind.clone(),
                    Some(declaration.name.clone()),
                )
            } else {
                let mut next = declarations
                    .get(cursor)
                    .map_or(selection.end, |declaration| {
                        declaration.bytes.start.min(selection.end)
                    });
                // Attributes before a declaration too large to keep whole
                // open its heading instead of standing alone.
                if next < selection.end
                    && trivia_end(source, offset, next, target_chars) == next
                    && !declarations[cursor..]
                        .iter()
                        .take_while(|declaration| declaration.bytes.start == next)
                        .any(|declaration| {
                            declaration.bytes.end <= selection.end
                                && source[next..declaration.bytes.end].chars().count()
                                    <= target_chars
                        })
                {
                    next = declarations[cursor..]
                        .iter()
                        .find(|declaration| declaration.bytes.start > next)
                        .map_or(selection.end, |declaration| {
                            declaration.bytes.start.min(selection.end)
                        });
                }
                let end = text_end(source, offset, next, target_chars);
                let kind = if end < next { "fragment" } else { "text" };
                (end, kind.to_owned(), None)
            };
            let target = offset..end;
            let context = context_ranges(source, selection, &target, &active, target_chars / 4);
            if !emit(Section {
                target,
                context,
                kind,
                name,
            })? {
                return Ok(false);
            }
            offset = end;
        }
    }
    Ok(true)
}

fn whitespace_end(source: &str, start: usize, end: usize, chars: usize) -> usize {
    source[start..end]
        .char_indices()
        .take(chars)
        .take_while(|(_, character)| character.is_whitespace())
        .fold(start, |_, (byte, character)| {
            start + byte + character.len_utf8()
        })
}

/// The end of leading trivia from `start`: whitespace and whole lines that
/// are only an attribute or decorator (`#[..]`, `#![..]`, `@name`), which
/// belong to the declaration after them.
fn trivia_end(source: &str, start: usize, end: usize, chars: usize) -> usize {
    let mut offset = whitespace_end(source, start, end, chars);
    while offset < end {
        let line_end = source[offset..end]
            .find('\n')
            .map_or(end, |index| offset + index + 1);
        let line = source[offset..line_end].trim();
        let attribute = ((line.starts_with("#[") || line.starts_with("#!["))
            && line.ends_with(']'))
            || line.starts_with('@');
        if !attribute {
            break;
        }
        offset = whitespace_end(source, line_end, end, chars);
    }
    offset
}

fn text_end(source: &str, start: usize, end: usize, chars: usize) -> usize {
    let text = &source[start..end];
    let maximum = text
        .char_indices()
        .nth(chars)
        .map_or(end, |(byte, _)| start + byte);
    if maximum == end {
        return end;
    }
    // Prefer complete paragraphs, then complete lines, then whitespace.
    let prefix = &source[start..maximum];
    prefix
        .rfind("\n\n")
        .map(|byte| start + byte + 2)
        .or_else(|| prefix.rfind('\n').map(|byte| start + byte + 1))
        .or_else(|| {
            prefix
                .char_indices()
                .rev()
                .find(|(_, c)| c.is_whitespace())
                .map(|(byte, c)| start + byte + c.len_utf8())
        })
        .filter(|&boundary| boundary > start)
        .unwrap_or(maximum)
}

fn context_ranges(
    source: &str,
    selection: &Range<usize>,
    target: &Range<usize>,
    declarations: &[&Declaration],
    mut chars: usize,
) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    // Outer headings keep lexical ownership without copying a whole parent.
    for declaration in declarations
        .iter()
        .filter(|declaration| {
            declaration.bytes.start >= selection.start
                && declaration.bytes.start < target.start
                && declaration.bytes.end >= target.end
        })
        .rev()
    {
        let heading = declaration.heading.start..declaration.heading.end.min(target.start);
        let cost = source[heading.clone()].chars().count();
        if cost > 0
            && cost <= chars
            && !ranges.iter().any(|other: &Range<usize>| {
                other.start <= heading.start && other.end >= heading.end
            })
        {
            chars -= cost;
            ranges.push(heading);
        }
    }
    if chars > 0 && target.start > 0 {
        let start = source[selection.start..target.start]
            .char_indices()
            .rev()
            .nth(chars.saturating_sub(1))
            .map_or(selection.start, |(byte, _)| selection.start + byte);
        if start < target.start {
            ranges.push(start..target.start);
        }
    }
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(previous) = merged.last_mut()
            && previous.end >= range.start
        {
            previous.end = previous.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    merged
}

mod grouped;
pub(crate) use grouped::visit_grouped;

#[cfg(test)]
mod tests;
