//! The node a parsed pattern stands for, and what its parse holds: errors,
//! top-level nodes, the grammar's longer forms of it, and the rejection of one
//! that did not parse.

use tree_sitter::Node;

use crate::structural::language::{Expando, WRAP_MARKER};
use crate::structural::octo::matching::{children, named_children, node_text};
use crate::structural::octo::metavars::{MetaVar, meta_from_node};

use super::super::unsaid::is_longer_form;

/// The first error node under `root` that spells more than a capture.
pub(super) fn stray_error<'tree>(
    root: Node<'tree>,
    source: &str,
    expando: Expando,
) -> Option<Node<'tree>> {
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node.is_error() && meta_from_node(node, source, expando).is_none() {
            return Some(node);
        }
        pending.extend(children(node).into_iter().rev());
    }
    None
}

/// Whether `root` holds an error other than a `$$$` capture parsed as one.
pub(super) fn errors_beyond_captures(root: Node<'_>, source: &str, expando: Expando) -> bool {
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node.is_missing()
            || (node.is_error()
                && !matches!(
                    meta_from_node(node, source, expando),
                    Some(MetaVar::Multi(_) | MetaVar::IgnoredMulti)
                ))
        {
            return true;
        }
        pending.extend(children(node));
    }
    false
}

/// How many error and MISSING nodes `root` holds.
pub(super) fn parse_errors(root: Node<'_>) -> usize {
    let mut count = 0;
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node.is_error() || node.is_missing() {
            count += 1;
        }
        pending.extend(children(node));
    }
    count
}

/// The nodes directly under a pattern's wrapper. A recovered error is one of
/// them, though the parser marks it extra like a comment.
pub(super) fn top_level_nodes(root: Node<'_>) -> usize {
    named_children(root)
        .into_iter()
        .filter(|node| !node.is_extra() || node.is_error())
        .count()
}

/// The grammar's longer forms of `root`'s kind ([`is_longer_form`]).
pub(super) fn longer_forms(root: Node<'_>) -> Vec<String> {
    let language = root.language();
    (0..language.node_kind_count())
        .filter_map(|id| u16::try_from(id).ok())
        .filter(|id| language.node_kind_is_named(*id))
        .filter_map(|id| language.node_kind_for_id(id))
        .filter(|kind| is_longer_form(root, kind))
        .map(str::to_owned)
        .collect()
}

/// Whether `node` is the synthetic C-family function a statement pattern is
/// parsed inside (see `AgLanguage::c_family`), named [`WRAP_MARKER`].
pub(super) fn is_wrap_marker_function(node: Node<'_>, source: &str) -> bool {
    node.kind() == "function_definition"
        && node
            .child_by_field_name("declarator")
            .and_then(|declarator| declarator.child_by_field_name("declarator"))
            .is_some_and(|name| node_text(name, source) == WRAP_MARKER)
}

/// The node a pattern matches: its parse with every synthetic or
/// single-child wrapper removed. Wrappers we add are recognised by
/// [`WRAP_MARKER`] (rather than by kind, like the grammar's own wrappers), so
/// a real `class $NAME { ... }` or function pattern keeps its outer node.
pub(in crate::structural::octo::pattern) fn effective_pattern_root<'a>(
    mut node: Node<'a>,
    source: &str,
) -> Node<'a> {
    loop {
        let named = named_children(node);
        if named.len() == 1 && is_pattern_wrapper(node.kind()) {
            node = named[0];
            continue;
        }
        // C-family synthetic function: unwrap `void __OctoWrap(void) {
        // <statement> }` down to its single statement.
        if is_wrap_marker_function(node, source)
            && let Some(body) = node.child_by_field_name("body")
        {
            let body_named = named_children(body);
            if body_named.len() == 1 {
                node = body_named[0];
                continue;
            }
        }
        // The synthetic wrapper class: unwrap `class __OctoWrap { <member> }`
        // down to the single real member, giving it real class-body context
        // (a bare `public int Foo(...) { ... }` parsed standalone isn't valid
        // C# at all — no top-level member/method syntax exists outside a
        // type body).
        let is_wrapper = node
            .child_by_field_name("name")
            .is_some_and(|n| node_text(n, source) == WRAP_MARKER);
        if is_wrapper && let Some(body) = node.child_by_field_name("body") {
            let body_named = named_children(body);
            if body_named.len() == 1 {
                node = body_named[0];
                continue;
            }
        }
        break node;
    }
}

pub(super) fn is_pattern_wrapper(kind: &str) -> bool {
    matches!(
        kind,
        "program"
            | "source_file"
            | "module"
            | "compilation_unit"
            | "translation_unit"
            | "stylesheet"
            | "fragment"
            | "document"
            | "expression_statement"
            // C# wraps every top-level statement of a snippet in one.
            | "global_statement"
            | "config_file"
            | "body"
    )
}

/// The rejection for a pattern whose root did not parse. It names the last
/// token the parser could not place (`$$$BODY` in `fn $N() $$$BODY`, where a
/// function needs its body), so the caller sees what to complete.
pub(super) fn unparsed_pattern_message(root: Node<'_>, source: &str, expando: Expando) -> String {
    const BASE: &str = "invalid structural pattern: pattern parsed with syntax errors";
    let near = children(root)
        .into_iter()
        .rev()
        .find(|node| !node.is_extra())
        .map(|node| expando.restore(node_text(node, source).trim()))
        .filter(|text| !text.is_empty());
    let advice = "A fragment that parses only inside other code takes pattern {context, selector}, or a rule on its kind.";
    match near {
        Some(text) => format!("{BASE} near `{text}`. {advice}"),
        None => format!("{BASE}. {advice}"),
    }
}
