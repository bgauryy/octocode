//! Which nodes declare what: declaration kinds, the name and extent of a
//! declaration, C-family declarators, and export visibility.

use tree_sitter::Node;

use super::{name_node, node_text};

pub(super) fn declaration_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "function_item"
        | "function_signature_item"
        | "method_elem"
        | "function_definition"
        | "function_declaration"
        | "method_declaration"
        | "method_definition"
        | "method"
        | "singleton_method"
        | "function_clause" => Some("function"),
        "constructor_declaration" => Some("constructor"),
        "class_definition"
        | "class_declaration"
        | "abstract_class_declaration"
        | "class_specifier"
        | "class" => Some("class"),
        "struct_item" | "struct_specifier" | "struct_declaration" => Some("struct"),
        "enum_item" | "enum_declaration" | "enum_specifier" => Some("enum"),
        "trait_item" => Some("trait"),
        "interface_declaration" | "interface_item" => Some("interface"),
        "impl_item" => Some("impl"),
        "mod_item" | "module_definition" => Some("module"),
        // C#: namespaces, records, properties, delegates. TypeScript
        // `namespace N {}` is an `internal_module`.
        "namespace_declaration" | "file_scoped_namespace_declaration" | "internal_module" => {
            Some("namespace")
        }
        "record_declaration" | "record_struct_declaration" => Some("class"),
        "property_declaration" => Some("property"),
        "delegate_declaration" => Some("type"),
        "const_item" | "const_declaration" | "constant_declaration" | "static_item" => {
            Some("constant")
        }
        "type_item"
        | "type_declaration"
        | "type_alias"
        | "type_alias_declaration"
        | "type_definition"
        | "type_spec" => Some("type"),
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
pub(in crate::signatures) fn declaration<'t>(
    node: Node<'t>,
    content: &str,
) -> Option<(&'static str, Node<'t>)> {
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
        // C++ member function declarations (`void go();`, a pure virtual).
        "field_declaration" if is_function_prototype(node) => {
            declarator_name(node).map(|name| ("function", name))
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

/// The node that spans a declaration as written. Some grammars wrap the
/// declaration node in a node that holds its header: C++ and CUDA put
/// `template <…>` in a `template_declaration` around the function, class or
/// alias, and Python puts the `@decorator` lines in a `decorated_definition`
/// around the `def` or `class`. The wrapper starts where the declaration is
/// read from (a doc comment sits above the header), so its range is the
/// declaration's; the name node still places `line`.
pub(in crate::signatures) fn declaration_extent(node: Node<'_>) -> Node<'_> {
    let mut extent = node;
    while let Some(parent) = extent.parent()
        && matches!(
            parent.kind(),
            "template_declaration" | "decorated_definition"
        )
    {
        extent = parent;
    }
    extent
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

pub(in crate::signatures) fn is_exported_declaration(
    ext: &str,
    node: Node<'_>,
    content: &str,
    name: &str,
    parent: Option<&str>,
) -> bool {
    let text = node_text(node, content).unwrap_or("").trim_start();
    match ext {
        // Only plain `pub` leaves the crate: `pub(crate)`, `pub(super)`,
        // `pub(self)` and `pub(in path)` stay inside it.
        "rs" => text
            .strip_prefix("pub")
            .is_some_and(|rest| rest.starts_with(char::is_whitespace)),
        "go" => name.chars().next().is_some_and(char::is_uppercase),
        "py" => parent.is_none() && !name.starts_with('_'),
        "java" | "cs" => text.starts_with("public ") || text.starts_with("export "),
        ext if crate::signatures::languages::is_c_family_extension(ext) => {
            parent.is_none() && !text.starts_with("static ")
        }
        // The generic grammar does not model `.global`/`.globl` visibility
        // strongly enough to advertise labels as exported bindings.
        "asm" => false,
        _ => parent.is_none() && !name.starts_with('_'),
    }
}
