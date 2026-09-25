//! Pure tree-sitter node-classification and text/identifier utilities.
//!
//! These helpers are language-neutral and hold no accumulator state: they map
//! a `tree_sitter::Node` plus the source `content` to node-kind classifications,
//! name/string descendants, or cleaned identifier/specifier strings. They form
//! the syntactic vocabulary shared by the graph-facts extraction pipeline in the
//! parent module and depend only on `tree_sitter` and the string content.

use tree_sitter::Node;

pub(super) fn declaration_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "function_item"
        | "function_definition"
        | "function_declaration"
        | "method_declaration"
        | "method_definition"
        | "method"
        | "singleton_method"
        | "function_clause" => Some("function"),
        "constructor_declaration" => Some("constructor"),
        "class_definition" | "class_declaration" | "class_specifier" | "class" => Some("class"),
        "struct_item" | "struct_specifier" | "struct_declaration" => Some("struct"),
        "enum_item" | "enum_declaration" | "enum_specifier" => Some("enum"),
        "trait_item" => Some("trait"),
        "interface_declaration" | "interface_item" => Some("interface"),
        "impl_item" => Some("impl"),
        "mod_item" | "module_definition" => Some("module"),
        "const_item" | "const_declaration" | "constant_declaration" | "static_item" => {
            Some("constant")
        }
        "type_item" | "type_declaration" | "type_alias" | "type_definition" | "type_spec" => {
            Some("type")
        }
        "macro_definition" | "macro_rule" => Some("macro"),
        // Generic Assembly grammars model navigation anchors as labels rather
        // than high-level function declarations.
        "label" => Some("label"),
        _ => None,
    }
}

pub(super) fn name_node(node: Node<'_>) -> Option<Node<'_>> {
    for field in ["name", "type", "path"] {
        if let Some(child) = node.child_by_field_name(field) {
            if is_name_like(child.kind()) {
                return Some(child);
            }
            if let Some(descendant) = first_name_descendant(child, 2) {
                return Some(descendant);
            }
        }
    }
    first_name_descendant(node, 4)
}

pub(super) fn declaration_name(node: Node<'_>, content: &str) -> Option<String> {
    let name = node_text(name_node(node)?, content)?;
    compact_identifier(name)
}

fn first_name_descendant(node: Node<'_>, depth: u8) -> Option<Node<'_>> {
    if depth == 0 {
        return None;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if is_name_like(child.kind()) {
            return Some(child);
        }
        if let Some(found) = first_name_descendant(child, depth - 1) {
            return Some(found);
        }
    }
    None
}

fn is_name_like(kind: &str) -> bool {
    matches!(
        kind,
        "identifier"
            | "ident"
            | "type_identifier"
            | "field_identifier"
            | "property_identifier"
            | "scoped_identifier"
            | "scoped_type_identifier"
            | "namespace_identifier"
            | "module_name"
            | "simple_identifier"
            | "constant"
            | "alias"
            | "atom"
            | "word"
            | "name"
    )
}

pub(super) fn is_import_node(kind: &str) -> bool {
    matches!(
        kind,
        "import_statement"
            | "import_from_statement"
            | "import_declaration"
            | "import_spec"
            | "use_declaration"
            | "extern_crate_declaration"
            | "preproc_include"
            | "require_command"
            | "source_command"
    )
}

pub(super) fn import_specifier(node: Node<'_>, content: &str) -> Option<String> {
    if let Some(string_node) = first_string_descendant(node, 4) {
        return node_text(string_node, content).and_then(clean_specifier);
    }
    node_text(node, content).and_then(clean_specifier)
}

fn first_string_descendant(node: Node<'_>, depth: u8) -> Option<Node<'_>> {
    if depth == 0 {
        return None;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if is_string_like(child.kind()) {
            return Some(child);
        }
        if let Some(found) = first_string_descendant(child, depth - 1) {
            return Some(found);
        }
    }
    None
}

fn is_string_like(kind: &str) -> bool {
    matches!(
        kind,
        "string"
            | "string_literal"
            | "interpreted_string_literal"
            | "raw_string_literal"
            | "system_lib_string"
            | "string_content"
            | "quoted_string"
    )
}

pub(super) fn is_call_node(kind: &str) -> bool {
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
            | "object_creation_expression"
            | "constructor_invocation"
    )
}

/// The callee label of a call node and the node that spells it.
pub(super) fn call_callee<'tree>(
    node: Node<'tree>,
    content: &str,
) -> Option<(String, Node<'tree>)> {
    for field in ["function", "name", "method", "macro", "constructor"] {
        if let Some(child) = node.child_by_field_name(field) {
            if let Some(name) = node_text(child, content).and_then(compact_identifier) {
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
pub(super) fn last_name_leaf<'tree>(
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
pub(super) fn is_name_leaf(node: Node<'_>) -> bool {
    node.named_child_count() == 0 && is_name_like(node.kind())
}

pub(super) fn is_exported_declaration(
    ext: &str,
    node: Node<'_>,
    content: &str,
    name: &str,
    parent: Option<&str>,
) -> bool {
    let text = node_text(node, content).unwrap_or("").trim_start();
    match ext {
        "rs" => text.starts_with("pub ") || text.starts_with("pub("),
        "go" => name.chars().next().is_some_and(char::is_uppercase),
        "py" | "pyi" => parent.is_none() && !name.starts_with('_'),
        "java" | "cs" => text.starts_with("public ") || text.starts_with("export "),
        "c" | "h" | "cpp" | "hpp" | "cc" | "cxx" | "hh" | "hxx" | "cu" | "cuh" => {
            parent.is_none() && !text.starts_with("static ")
        }
        // The generic grammar does not model `.global`/`.globl` visibility
        // strongly enough to advertise labels as exported bindings.
        "asm" | "assembly" | "s" => false,
        "scala" | "sc" | "sbt" => parent.is_none() && !name.starts_with('_'),
        _ => parent.is_none() && !name.starts_with('_'),
    }
}

pub(super) fn node_text<'a>(node: Node<'_>, content: &'a str) -> Option<&'a str> {
    content.get(node.start_byte()..node.end_byte())
}

fn compact_identifier(text: &str) -> Option<String> {
    let trimmed = text.trim().trim_end_matches('!').trim();
    if trimmed.is_empty() || trimmed.len() > 160 {
        return None;
    }
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return None;
    }
    let value = trimmed
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('`')
        .trim()
        .to_owned();
    if value.is_empty() || value.len() > 160 {
        None
    } else {
        Some(value)
    }
}

pub(super) fn clean_specifier(text: &str) -> Option<String> {
    let mut value = text.trim();
    for prefix in [
        "import",
        "from",
        "use",
        "extern crate",
        "#include",
        "require",
        "source",
    ] {
        if let Some(rest) = value.strip_prefix(prefix) {
            value = rest.trim();
            break;
        }
    }
    value = value.trim_end_matches(';').trim();
    value = value
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('`')
        .trim_matches('<')
        .trim_matches('>')
        .trim();
    if value.is_empty() || value.len() > 200 || value.contains('\n') || value.contains('\r') {
        None
    } else {
        Some(value.to_owned())
    }
}
