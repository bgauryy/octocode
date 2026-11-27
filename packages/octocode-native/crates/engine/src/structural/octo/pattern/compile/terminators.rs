//! What the end of a pattern spells: the terminator compilation appended
//! for context, a `;` the caller wrote after the pattern's node, and the
//! kinds the grammar reads the same parts as in another place.

use tree_sitter::{Language, Node};

use crate::structural::language::{WRAP_MARKER, wrap_in_class_body};
use crate::structural::octo::error::CompileError;
use crate::structural::octo::matching::{children, named_children, node_text, parse_tree};

use super::super::{Alternate, WrittenTerminator, node_key};
use super::root::{effective_pattern_root, kinds_with_a_leading_word};

/// Where the `;` that ends `root` starts in `source`, when the caller's
/// `pattern` does not end in one: compilation appended it for context (a
/// fragment parses as a statement or item only when terminated), so it is no
/// part the pattern spells.
pub(super) fn unwritten_terminator(root: Node<'_>, pattern: &str, source: &str) -> Option<usize> {
    if pattern.trim_end().ends_with(';') {
        return None;
    }
    children(root)
        .last()
        .filter(|last| !last.is_named() && !last.is_missing() && node_text(**last, source) == ";")
        .map(Node::start_byte)
}

/// The `;` the caller's `pattern` ends in, when the grammar puts it after
/// `root`, as the last part of the node that wraps it
/// ([`WrittenTerminator`]).
pub(super) fn written_terminator(root: Node<'_>, pattern: &str) -> Option<WrittenTerminator> {
    if !pattern.trim_end().ends_with(';') {
        return None;
    }
    let parent = root.parent()?;
    let mut after =
        std::iter::successors(root.next_sibling(), Node::next_sibling).filter(|n| !n.is_extra());
    let semicolon = after.next()?;
    let terminates = !semicolon.is_named()
        && !semicolon.is_missing()
        && semicolon.kind() == ";"
        && after.next().is_none();
    terminates.then(|| WrittenTerminator {
        root: node_key(root),
        parent: parent.parent().map(|_| parent.kind_id()),
    })
}

impl Alternate {
    /// `node` read as `kind` too, and as the grammar's longer forms of it.
    fn new(language: &Language, node: Node<'_>, kind: u16) -> Self {
        let longer = language
            .node_kind_for_id(kind)
            .map(|name| kinds_with_a_leading_word(language, name))
            .unwrap_or_default();
        Self {
            node: node_key(node),
            kinds: std::iter::once(kind).chain(longer).collect(),
        }
    }
}

/// The node of `root` the grammar reads as another kind with a body (`{}`)
/// in place of the terminator the caller's `pattern` does not spell (the
/// one compilation appended at `unwritten`, or none): Rust reads
/// `fn f() {}` as a `function_item` beside `fn f();`, a
/// `function_signature_item`, and TypeScript `export function f() {}`
/// exports a `function_declaration` where `export function f()` exports a
/// `function_signature`. `None` when the pattern ends in a terminator or
/// body, or the grammar reads no such declaration.
pub(super) fn bodied_alternate(
    language: &Language,
    pattern: &str,
    source: &str,
    root: Node<'_>,
    unwritten: Option<usize>,
) -> Result<Option<Alternate>, CompileError> {
    if pattern.trim_end().ends_with([';', '}']) {
        return Ok(None);
    }
    let (at, end) = unwritten.map_or((root.end_byte(), root.end_byte()), |at| (at, at + 1));
    let (Some(before), Some(after)) = (source.get(..at), source.get(end..)) else {
        return Ok(None);
    };
    let bodied_source = format!("{before} {{}}{after}");
    let tree = parse_tree(language, &bodied_source)?;
    if tree.root_node().has_error() {
        return Ok(None);
    }
    let bodied = effective_pattern_root(tree.root_node(), &bodied_source);
    Ok(adds_a_body(root, bodied).map(|(node, kind)| Alternate::new(language, node, kind)))
}

/// The node of `written`, or of its last parts down, that `bodied` reads as
/// another kind with the same parts and a `body` after them, and that kind.
fn adds_a_body<'w>(written: Node<'w>, bodied: Node<'_>) -> Option<(Node<'w>, u16)> {
    let (parts, bodied_parts) = (named_children(written), named_children(bodied));
    if written.kind_id() == bodied.kind_id() {
        if parts.len() != bodied_parts.len() {
            return None;
        }
        return adds_a_body(*parts.last()?, *bodied_parts.last()?);
    }
    let kinds = |nodes: &[Node<'_>]| nodes.iter().map(Node::kind_id).collect::<Vec<_>>();
    let only_a_body = bodied_parts.len() == parts.len() + 1
        && kinds(&bodied_parts).starts_with(&kinds(&parts))
        && bodied
            .child_by_field_name("body")
            .is_some_and(|body| bodied_parts.last() == Some(&body));
    only_a_body.then(|| (written, bodied.kind_id()))
}

/// The declaration the grammar reads `root`'s text as in a class body, when
/// that is another kind with the same parts: a Java or C# local variable's
/// text is a field there.
pub(super) fn member_alternate(
    language: &Language,
    source: &str,
    root: Node<'_>,
) -> Result<Option<Alternate>, CompileError> {
    let Some(text) = source.get(root.start_byte()..root.end_byte()) else {
        return Ok(None);
    };
    // A grammar may end the class itself with a `;` (C++).
    let class = wrap_in_class_body(text);
    let mut parsed = None;
    for member in [class.clone(), format!("{class};")] {
        let tree = parse_tree(language, &member)?;
        if !tree.root_node().has_error() {
            parsed = Some((member, tree));
            break;
        }
    }
    let Some((member, tree)) = parsed else {
        return Ok(None);
    };
    let member_root = effective_pattern_root(tree.root_node(), &member);
    let kinds = |node: Node<'_>| {
        named_children(node)
            .iter()
            .map(Node::kind_id)
            .collect::<Vec<_>>()
    };
    let same_parts = member_root.kind_id() != root.kind_id()
        && kinds(member_root) == kinds(root)
        && member_root
            .child_by_field_name("name")
            .is_none_or(|name| node_text(name, &member) != WRAP_MARKER);
    Ok(same_parts.then(|| Alternate::new(language, root, member_root.kind_id())))
}
