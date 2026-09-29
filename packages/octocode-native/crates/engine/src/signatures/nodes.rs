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
        // C#: namespaces, records, properties, delegates.
        "namespace_declaration" | "file_scoped_namespace_declaration" => Some("namespace"),
        "record_declaration" | "record_struct_declaration" => Some("class"),
        "property_declaration" => Some("property"),
        "delegate_declaration" => Some("type"),
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

/// The declaration `node` introduces, as its kind and the node holding its
/// name. Extends [`declaration_kind`] with shapes whose kind depends on
/// context or whose name is not the first name-like child: Scala objects,
/// traits and `val`s, C/C++ macros and file-scope constants, Java/C#
/// constant fields, and Python module-level assignments.
pub(super) fn declaration<'t>(node: Node<'t>, content: &str) -> Option<(&'static str, Node<'t>)> {
    // A C# namespace is named by its whole dotted name (`Acme.Core`).
    if matches!(
        node.kind(),
        "namespace_declaration" | "file_scoped_namespace_declaration"
    ) {
        return node
            .child_by_field_name("name")
            .map(|name| ("namespace", name));
    }
    // A Go `type` declaration only wraps its specs (`type X …` or a grouped
    // `type ( … )`); each spec is the declaration.
    if node.kind() == "type_declaration" {
        let mut cursor = node.walk();
        if node
            .named_children(&mut cursor)
            .any(|child| matches!(child.kind(), "type_spec" | "type_alias"))
        {
            return None;
        }
    }
    if let Some(kind) = declaration_kind(node.kind()) {
        // C/C++ definitions carry no `name` field: the name sits in the
        // declarator chain, and the `type` field (a named return type such as
        // `Tensor`, or an attribute macro such as `__init`) must not be taken.
        if node.child_by_field_name("name").is_none()
            && node.child_by_field_name("declarator").is_some()
            && let Some(name) = declarator_name(node)
        {
            return Some((kind, name));
        }
        return name_node(node).map(|name| (kind, name));
    }
    match node.kind() {
        "object_definition" => node
            .child_by_field_name("name")
            .map(|name| ("module", name)),
        "trait_definition" => node.child_by_field_name("name").map(|name| ("trait", name)),
        "val_definition" => node
            .child_by_field_name("pattern")
            .filter(|pattern| pattern.kind() == "identifier")
            .map(|name| ("constant", name)),
        "preproc_def" | "preproc_function_def" => {
            node.child_by_field_name("name").map(|name| ("macro", name))
        }
        // Java `static final` / C# `const` and `static readonly` fields; plain
        // fields stay out of the outline.
        "field_declaration" if is_constant_field(node, content) => {
            declarator_name(node).map(|name| ("constant", name))
        }
        // C/C++ prototypes (`void f(void);`, a header's API) and
        // `const`/`constexpr` variables at file or namespace scope.
        "declaration"
            if node.parent().is_some_and(|parent| {
                matches!(parent.kind(), "translation_unit" | "declaration_list")
            }) && is_function_prototype(node) =>
        {
            declarator_name(node).map(|name| ("function", name))
        }
        "declaration"
            if node.parent().is_some_and(|parent| {
                matches!(parent.kind(), "translation_unit" | "declaration_list")
            }) && has_const_qualifier(node, content) =>
        {
            declarator_name(node).map(|name| ("constant", name))
        }
        // Python module-level `NAME = value`.
        "expression_statement"
            if node
                .parent()
                .is_some_and(|parent| parent.kind() == "module") =>
        {
            let assignment = node
                .named_child(0)
                .filter(|child| child.kind() == "assignment")?;
            let left = assignment
                .child_by_field_name("left")
                .filter(|left| left.kind() == "identifier")?;
            let name = node_text(left, content)?;
            let constant =
                name.chars().any(char::is_alphabetic) && !name.chars().any(char::is_lowercase);
            Some((if constant { "constant" } else { "variable" }, left))
        }
        _ => None,
    }
}

fn modifier_words<'c>(node: Node<'_>, content: &'c str) -> Vec<&'c str> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|child| matches!(child.kind(), "modifiers" | "modifier"))
        .filter_map(|child| node_text(child, content))
        .flat_map(str::split_whitespace)
        .collect()
}

fn is_constant_field(node: Node<'_>, content: &str) -> bool {
    let words = modifier_words(node, content);
    let has = |word: &str| words.contains(&word);
    has("const") || (has("static") && (has("final") || has("readonly")))
}

fn has_const_qualifier(node: Node<'_>, content: &str) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|child| child.kind() == "type_qualifier")
        .filter_map(|child| node_text(child, content))
        .any(|text| matches!(text, "const" | "constexpr"))
}

/// A declaration whose declarator is a function (through return-type pointer
/// or reference wrappers); `int (*fp)(int)` is a function-pointer variable.
fn is_function_prototype(node: Node<'_>) -> bool {
    let mut current = node.child_by_field_name("declarator");
    while let Some(declarator) = current {
        match declarator.kind() {
            "function_declarator" => {
                return declarator
                    .child_by_field_name("declarator")
                    .is_some_and(|inner| inner.kind() != "parenthesized_declarator");
            }
            "pointer_declarator" | "reference_declarator" => {
                let mut cursor = declarator.walk();
                current = declarator.child_by_field_name("declarator").or_else(|| {
                    declarator
                        .named_children(&mut cursor)
                        .find(|child| child.kind().ends_with("_declarator"))
                });
            }
            _ => return false,
        }
    }
    false
}

/// Identifier kinds that end a declarator chain.
fn is_declarator_leaf(kind: &str) -> bool {
    matches!(
        kind,
        "identifier" | "field_identifier" | "type_identifier" | "destructor_name" | "operator_name"
    )
}

/// The declared identifier of a (possibly nested) declarator: through
/// pointer/reference/function/array declarators and C++ qualified names
/// (`Engine::execute` yields `execute`).
fn declarator_name(node: Node<'_>) -> Option<Node<'_>> {
    let mut current = node;
    for _ in 0..8 {
        let next = if current.kind() == "qualified_identifier" {
            current.child_by_field_name("name")
        } else {
            current.child_by_field_name("declarator").or_else(|| {
                let mut cursor = current.walk();
                // `reference_declarator` (C++ `T& f()`) holds its inner
                // declarator as an unnamed child.
                current.named_children(&mut cursor).find(|child| {
                    matches!(child.kind(), "variable_declaration" | "variable_declarator")
                        || child.kind().ends_with("_declarator")
                        || (current.kind() == "reference_declarator"
                            && is_declarator_leaf(child.kind()))
                })
            })
        };
        match next {
            Some(child) if is_declarator_leaf(child.kind()) => return Some(child),
            Some(child) => current = child,
            None => break,
        }
    }
    current
        .child_by_field_name("name")
        .or_else(|| {
            let mut cursor = current.walk();
            current
                .named_children(&mut cursor)
                .find(|child| child.kind() == "identifier")
        })
        .filter(|name| matches!(name.kind(), "identifier" | "field_identifier"))
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
            | "using_directive"
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

/// Strict ancestors of `node` under `root`, root first, from one root-down
/// descent. `Node::parent()` repeats that descent from the root on every
/// call, so a climb of `d` hops costs `d` descents; walk this chain instead.
/// Empty when `node` is `root` or is not inside it.
pub(super) fn ancestors<'t>(root: Node<'t>, node: Node<'t>) -> Vec<Node<'t>> {
    let mut chain = Vec::new();
    let mut current = root;
    while current.id() != node.id() {
        chain.push(current);
        match current.child_with_descendant(node) {
            Some(next) => current = next,
            None => return Vec::new(),
        }
    }
    chain
}

pub(super) fn node_text<'a>(node: Node<'_>, content: &'a str) -> Option<&'a str> {
    content.get(node.start_byte()..node.end_byte())
}

pub(super) fn compact_identifier(text: &str) -> Option<String> {
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
