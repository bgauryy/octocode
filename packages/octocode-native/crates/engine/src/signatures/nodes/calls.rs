//! Call nodes: the callee label a call edge names and the name token it targets.

use tree_sitter::Node;

use super::{compact_identifier, first_name_descendant, is_name_like, node_text};

pub(in crate::signatures) fn is_call_node(kind: &str) -> bool {
    matches!(
        kind,
        "call_expression"
            | "call"
            | "method_invocation"
            | "invocation_expression"
            | "function_call_expression"
            | "member_call_expression"
            | "macro_invocation"
            | "command"
    )
}

/// The callee label of a call node and the node that spells it. The label
/// is structural: the argument lists, index expressions and literals of a
/// chained receiver (`write(&path, "…").expect`) are elided to their
/// brackets (`write().expect`), so no argument text is carried as a name.
pub(in crate::signatures) fn call_callee<'tree>(
    node: Node<'tree>,
    content: &str,
) -> Option<(String, Node<'tree>)> {
    let (label, spelled) = callee_text(node, content)?;
    Some((elide_arguments(&label), spelled))
}

/// `label` with the inside of every bracket group and string literal
/// removed: `a(x, "s").b[0].c` → `a().b[].c`. Quotes and brackets nested
/// inside a group are skipped with it; a backslash escapes the next char.
pub(super) fn elide_arguments(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in label.chars() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == open {
                quote = None;
                if depth == 0 {
                    out.push(c);
                }
            }
            continue;
        }
        match c {
            '"' | '\'' | '`' => {
                quote = Some(c);
                if depth == 0 {
                    out.push(c);
                }
            }
            '(' | '[' | '{' => {
                if depth == 0 {
                    out.push(c);
                }
                depth += 1;
            }
            ')' | ']' | '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    out.push(c);
                }
            }
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

fn callee_text<'tree>(node: Node<'tree>, content: &str) -> Option<(String, Node<'tree>)> {
    // Java `obj.name(..)`: the receiver lives in a sibling `object` field.
    if let (Some(object), Some(name)) = (
        node.child_by_field_name("object"),
        node.child_by_field_name("name"),
    ) && let Some(method) = node_text(name, content).and_then(compact_identifier)
    {
        let receiver = node_text(object, content)
            .map(|text| text.split_whitespace().collect::<String>())
            .and_then(|text| compact_identifier(&text))
            .filter(|text| text.len() <= 80)
            .unwrap_or_else(|| "<expr>".to_owned());
        return Some((format!("{receiver}.{method}"), name));
    }
    for field in ["function", "name", "method", "macro", "constructor"] {
        if let Some(child) = node.child_by_field_name(field) {
            if let Some(name) = node_text(child, content).and_then(compact_identifier) {
                return Some((name, child));
            }
            // rustfmt/prettier break method chains across lines: collapse
            // the whitespace so `self\n    .config\n    .iter` keeps its
            // receiver instead of degrading to a bare `iter`.
            if let Some(name) = node_text(child, content)
                .map(|text| text.split_whitespace().collect::<String>())
                .and_then(|text| compact_identifier(&text))
            {
                return Some((name, child));
            }
            if let Some(descendant) = first_name_descendant(child, 3)
                && let Some(name) = node_text(descendant, content).and_then(compact_identifier)
            {
                return Some((name, descendant));
            }
        }
    }
    let child = first_name_descendant(node, 3)?;
    Some((
        node_text(child, content).and_then(compact_identifier)?,
        child,
    ))
}

/// The last name-leaf under `node` (inclusive) spelling `name`: the token a
/// call edge targets (`run` in `self.run`, `new` in `Self::new`).
pub(in crate::signatures) fn last_name_leaf<'tree>(
    node: Node<'tree>,
    content: &str,
    name: &str,
) -> Option<Node<'tree>> {
    let mut found = None;
    let mut pending = vec![node];
    let mut cursor = node.walk();
    while let Some(current) = pending.pop() {
        if is_name_leaf(current)
            && node_text(current, content) == Some(name)
            && found.is_none_or(|last: Node<'_>| last.start_byte() < current.start_byte())
        {
            found = Some(current);
        }
        pending.extend(current.named_children(&mut cursor));
    }
    found
}

/// An identifier-kind token (`identifier`, `type_identifier`,
/// `field_identifier`, …): the only nodes that can reference a declaration.
/// Comments and string contents are separate token kinds, so they never match.
pub(in crate::signatures) fn is_name_leaf(node: Node<'_>) -> bool {
    node.named_child_count() == 0 && is_name_like(node.kind())
}
