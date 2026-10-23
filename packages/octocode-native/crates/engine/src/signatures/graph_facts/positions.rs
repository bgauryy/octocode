//! Graph positions of syntax-tree nodes and the error spans of a recovered parse.

use tree_sitter::Node;

use crate::graph::{GraphPosition, GraphRange};

/// Graph positions of syntax-tree nodes: the shared [`LineIndex`] under the
/// tree-sitter rule (`\n` only), with UTF-16 columns.
///
/// [`LineIndex`]: crate::text::LineIndex
pub(super) struct NodePositions<'a> {
    content: &'a str,
    index: crate::text::LineIndex,
}

impl<'a> NodePositions<'a> {
    pub(super) fn new(content: &'a str) -> Self {
        Self {
            content,
            index: crate::text::LineIndex::tree_sitter(content),
        }
    }

    pub(super) fn range(&self, node: Node<'_>) -> GraphRange {
        GraphRange {
            start: self.position(node.start_byte()),
            end: self.position(node.end_byte()),
        }
    }

    /// A declaration's range (see [`declaration_extent`]), ending at its last character: a node that
    /// owns its line terminator (C `#define`, every `preproc_*`) would
    /// otherwise end at column 0 of the next line, which a line-based
    /// reader takes as the next declaration's line.
    pub(super) fn declaration_range(&self, node: Node<'_>) -> GraphRange {
        let start = node.start_byte();
        let trimmed = self.content.as_bytes()[start..node.end_byte()]
            .iter()
            .rposition(|byte| !matches!(byte, b'\n' | b'\r'))
            .map_or(start, |last| start + last + 1);
        GraphRange {
            start: self.position(start),
            end: self.position(trimmed),
        }
    }

    pub(super) fn position(&self, byte_offset: usize) -> GraphPosition {
        let (line, character) = self
            .index
            .byte_to_position(self.content, byte_offset as u32);
        GraphPosition { line, character }
    }
}

/// Error spans past this many collapse to the whole file.
const MAX_ERROR_SPANS: usize = 64;

/// 1-based line spans of the `ERROR` and missing nodes of a recovered parse,
/// merged; past [`MAX_ERROR_SPANS`] one span covers every line.
pub(super) fn syntax_error_lines(root: Node<'_>) -> Vec<[u32; 2]> {
    let mut spans = Vec::new();
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.is_error() || node.is_missing() {
            let line = |row: usize| u32::try_from(row + 1).unwrap_or(u32::MAX);
            let (start, end) = (node.start_position(), node.end_position());
            // A node ending at column 0 ends with the previous line's newline.
            let last = if end.column == 0 && end.row > start.row {
                end.row - 1
            } else {
                end.row
            };
            spans.push((line(start.row), line(last)));
            continue;
        }
        if node.has_error() {
            stack.extend(node.children(&mut cursor));
        }
    }
    spans.sort_unstable();
    let mut merged: Vec<[u32; 2]> = Vec::new();
    for (start, end) in spans {
        match merged.last_mut() {
            Some(last) if start <= last[1].saturating_add(1) => last[1] = last[1].max(end),
            _ => merged.push([start, end]),
        }
    }
    if merged.len() > MAX_ERROR_SPANS {
        return vec![[1, u32::MAX]];
    }
    merged
}
