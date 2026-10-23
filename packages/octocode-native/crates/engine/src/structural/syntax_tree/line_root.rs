//! Where a `line` roots a syntax-tree walk.

use tree_sitter::Node;

/// Where a `line` roots the walk.
pub(super) enum LineRoot<'tree> {
    /// The smallest node covering the line's text.
    Covering(Node<'tree>),
    /// A blank line has no text to cover: the item after it inside the node
    /// enclosing it (else the item before it, else that node), with a note
    /// that says which node the tree starts at.
    Blank { node: Node<'tree>, note: String },
}

/// The smallest node (named, with `named_only`) covering the text of
/// 1-based `line`, without its surrounding whitespace; for a blank line, the
/// nearest item inside the node enclosing it; `None` past the end.
pub(super) fn line_node<'tree>(
    root: Node<'tree>,
    content: &str,
    line: u32,
    named_only: bool,
) -> Option<LineRoot<'tree>> {
    let index = usize::try_from(line.checked_sub(1)?).ok()?;
    let start = if index == 0 {
        0
    } else {
        content
            .match_indices('\n')
            .nth(index - 1)
            .map(|(offset, _)| offset + 1)?
    };
    if start > content.len() || (start == content.len() && index > 0) {
        return None;
    }
    let end = content[start..]
        .find('\n')
        .map_or(content.len(), |offset| start + offset);
    let text = &content[start..end];
    if text.trim().is_empty() {
        return Some(blank_line_root(root, start, end, line));
    }
    let from = start + (text.len() - text.trim_start().len());
    let to = (end - (text.len() - text.trim_end().len())).max(from);
    let node = if named_only {
        root.named_descendant_for_byte_range(from, to)
    } else {
        root.descendant_for_byte_range(from, to)
    }?;
    Some(LineRoot::Covering(node))
}

/// The root of a walk asked for blank `line` (bytes `start..end`): the
/// smallest node strictly enclosing the line, then the first named child
/// (comments and annotations aside) starting after it, else the last one
/// ending before it.
fn blank_line_root(root: Node<'_>, start: usize, end: usize, line: u32) -> LineRoot<'_> {
    let mut cursor = root.walk();
    let mut enclosing = root;
    while let Some(inner) = enclosing
        .named_children(&mut cursor)
        .find(|child| child.start_byte() < start && child.end_byte() > end)
    {
        enclosing = inner;
    }
    // Comments are extras, and an attribute or decorator that is a sibling
    // annotates the item after it: neither is an item, so the tree starts at
    // the item itself.
    let children = enclosing
        .named_children(&mut cursor)
        .filter(|child| {
            !child.is_extra() && !crate::signatures::extractor::is_annotation_kind(child.kind())
        })
        .collect::<Vec<_>>();
    let next = children.iter().find(|child| child.start_byte() >= end);
    let previous = children
        .iter()
        .rev()
        .find(|child| child.end_byte() <= start);
    let (node, side) = match (next, previous) {
        (Some(node), _) => (*node, "the next item"),
        (None, Some(node)) => (*node, "the item before it"),
        (None, None) => (enclosing, "the node enclosing it"),
    };
    let note = format!(
        "line {line} is blank; the tree starts at {side}, {} at line {}",
        node.kind(),
        node.start_position().row + 1
    );
    LineRoot::Blank { node, note }
}
