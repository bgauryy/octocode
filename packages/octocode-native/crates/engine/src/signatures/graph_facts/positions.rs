//! Graph positions of syntax-tree nodes and the error spans of a recovered parse.

use crate::signatures::node_kinds::kind_of;
use tree_sitter::Node;

use crate::graph::{GraphPosition, GraphRange};
use crate::num::saturating_u32;

/// Last source token of a declaration. An indentation-based grammar can own
/// the following blank lines and comment extras until its dedent. Follow only
/// the rightmost syntax branch, retaining delimiters and literal bytes.
pub(super) fn declaration_end_byte(node: Node<'_>) -> usize {
    let mut cursor = node.walk();
    let start = node.start_position();
    loop {
        if !cursor.goto_last_child() {
            return cursor.node().end_byte();
        }
        loop {
            let child = cursor.node();
            let comment = crate::signatures::node_kinds::is_comment_kind(kind_of(child));
            let at = child.start_position();
            // An inline comment or a body-indented trailing comment belongs
            // to this declaration. Only outdented comment extras can belong
            // to the next declaration whose dedent ended this node.
            if comment && (at.row == start.row || at.column > start.column) {
                return child.end_byte();
            }
            if !child.is_extra()
                && !child.is_missing()
                && child.start_byte() < child.end_byte()
                && !kind_of(child).trim().is_empty()
                && !comment
            {
                break;
            }
            if !cursor.goto_previous_sibling() {
                // A malformed or empty wrapper with only trivia cannot name
                // a smaller source token. Keep its observed extent.
                cursor.goto_parent();
                return cursor.node().end_byte();
            }
        }
    }
}

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

    /// A declaration's range, ending at its final syntax token rather than
    /// trailing comments or an indentation-based grammar's dedent position.
    pub(super) fn declaration_range(&self, node: Node<'_>, header: Option<usize>) -> GraphRange {
        let start = node.start_byte();
        GraphRange {
            start: self.position(header.map_or(start, |at| at.min(start))),
            end: self.position(declaration_end_byte(node)),
        }
    }

    pub(super) fn position(&self, byte_offset: usize) -> GraphPosition {
        let (line, character) = self
            .index
            .byte_to_position(self.content, crate::num::saturating_u32(byte_offset));
        GraphPosition { line, character }
    }
}

/// For each of a parent's `children`, the start byte of the outer annotation
/// run written just before it, in one pass over the siblings (never a
/// per-node sibling search). Rust puts `#[…]` attributes beside the item, not
/// inside it, so they join the item's header as a Python decorator or a Java
/// annotation does. Comments between them do not end the run, and the file's
/// inner `#![…]` attributes belong to the file, not to an item.
pub(super) fn leading_annotation_starts(children: &[Node<'_>]) -> Vec<Option<usize>> {
    use crate::signatures::node_kinds::{is_annotation_kind, is_comment_kind};
    let mut run: Option<usize> = None;
    children
        .iter()
        .map(|child| {
            let leading = run;
            let kind = kind_of(child);
            if is_annotation_kind(kind) && kind != "inner_attribute_item" {
                run.get_or_insert(child.start_byte());
            } else if !is_comment_kind(kind) {
                run = None;
            }
            leading
        })
        .collect()
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
            let line = |row: usize| saturating_u32(row + 1);
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
