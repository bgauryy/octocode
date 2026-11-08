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
        | "local_function_statement"
        | "function_clause" => Some("function"),
        "constructor_declaration" => Some("constructor"),
        "class_definition"
        | "class_declaration"
        | "abstract_class_declaration"
        | "class_specifier"
        | "class" => Some("class"),
        "struct_item" | "struct_specifier" | "struct_declaration" | "union_item" => Some("struct"),
        "enum_item" | "enum_declaration" | "enum_specifier" | "enum_definition" => Some("enum"),
        "trait_item" => Some("trait"),
        "interface_declaration" | "interface_item" | "annotation_type_declaration" => {
            Some("interface")
        }
        "impl_item" => Some("impl"),
        "mod_item" | "module_definition" => Some("module"),
        // C#: namespaces, records, properties, delegates. TypeScript
        // `namespace N {}` is an `internal_module`.
        "namespace_declaration"
        | "namespace_definition"
        | "file_scoped_namespace_declaration"
        | "internal_module" => Some("namespace"),
        "record_declaration" | "record_struct_declaration" => Some("class"),
        "property_declaration" => Some("property"),
        "delegate_declaration" => Some("type"),
        "const_item" | "const_declaration" | "static_item" => Some("constant"),
        // Rust variants, Java constants, C# members, C/C++ enumerators, Scala
        // 3 cases and TypeScript `A = 1` members.
        "enum_variant"
        | "enum_constant"
        | "enum_member_declaration"
        | "enumerator"
        | "simple_enum_case"
        | "full_enum_case"
        | "enum_assignment" => Some("enumMember"),
        "type_item"
        | "type_declaration"
        | "type_alias"
        | "type_alias_declaration"
        | "type_definition"
        | "type_spec" => Some("type"),
        "macro_definition" => Some("macro"),
        // Generic Assembly grammars model navigation anchors as labels rather
        // than high-level function declarations.
        "label" => Some("label"),
        _ => None,
    }
}

/// The declaration `node` introduces, as its kind and the node holding its
/// name. Extends [`declaration_kind`] with shapes whose kind depends on
/// context or whose name is not the first name-like child: Scala objects,
/// traits, `val`s and `var`s, C/C++ macros and file-scope constants, each
/// data member a field declares ([`member_field`]), and Python module-level
/// and class-level assignments.
pub(in crate::signatures) fn declaration<'t>(
    node: Node<'t>,
    content: &str,
) -> Option<(&'static str, Node<'t>)> {
    declaration_given(node, || parents_of(node, &[]), content)
}

/// [`declaration`] for a walk that knows `node`'s `ancestors` (root first,
/// nearest last): `Node::parent()` searches from the root, so it climbs only
/// past the outermost one it knows.
pub(in crate::signatures) fn declaration_in<'t>(
    node: Node<'t>,
    ancestors: &[Node<'t>],
    content: &str,
) -> Option<(&'static str, Node<'t>)> {
    declaration_given(node, || parents_of(node, ancestors), content)
}

/// `node`'s parents, nearest first: the `known` ones (root first, nearest
/// last, as a walk keeps its path), then the ones climbed from the outermost
/// of those, each only when it is asked for.
fn parents_of<'t, 'a>(
    node: Node<'t>,
    known: &'a [Node<'t>],
) -> impl Iterator<Item = Node<'t>> + Clone + 'a {
    let top = known.first().copied().unwrap_or(node);
    known
        .iter()
        .rev()
        .copied()
        .chain(std::iter::successors(Some(top), Node::parent).skip(1))
}

/// The field declaration a data member `node` belongs to, given `node`'s
/// parents nearest first: a declarator of a Java or C/C++ field or a Java
/// interface constant (`declarator`, one per name of `int a, b;`), a name of
/// a Go or Rust field (`name`), or a C# field or event field's
/// `variable_declarator`. A function declarator declares a member function,
/// not data.
pub(in crate::signatures) fn member_field<'t>(
    node: Node<'t>,
    mut parents: impl Iterator<Item = Node<'t>>,
) -> Option<Node<'t>> {
    // Only a declarator or a name is one; other nodes ask for no parent.
    if !(is_declarator_leaf(node.kind()) || node.kind().ends_with("_declarator")) {
        return None;
    }
    let parent = parents.next()?;
    match parent.kind() {
        "field_declaration" | "constant_declaration" => {
            let in_field = |field: &str| {
                let mut cursor = parent.walk();
                parent
                    .children_by_field_name(field, &mut cursor)
                    .any(|child| child.id() == node.id())
            };
            ((in_field("declarator") && !is_function_declarator(node)) || in_field("name"))
                .then_some(parent)
        }
        "variable_declaration" if node.kind() == "variable_declarator" => {
            parents.next().filter(|field| {
                matches!(
                    field.kind(),
                    "field_declaration" | "event_field_declaration"
                )
            })
        }
        _ => None,
    }
}

/// Whether a declaration under `parent` sits at file or namespace scope: a
/// preprocessor block (an include guard, `#ifdef __cplusplus`) opens no scope.
fn at_file_scope(mut parent: Option<Node<'_>>) -> bool {
    while let Some(block) = parent.filter(|block| {
        matches!(
            block.kind(),
            "preproc_ifdef" | "preproc_if" | "preproc_else" | "preproc_elif" | "preproc_elifdef"
        )
    }) {
        parent = block.parent();
    }
    parent.is_some_and(|parent| matches!(parent.kind(), "translation_unit" | "declaration_list"))
}

fn declaration_given<'t, P: Iterator<Item = Node<'t>> + Clone>(
    node: Node<'t>,
    parents: impl Fn() -> P,
    content: &str,
) -> Option<(&'static str, Node<'t>)> {
    let parent = || parents().next();
    let node_kind = node.kind();
    if let Some(field) = member_field(node, parents()) {
        let name = if is_declarator_leaf(node.kind()) {
            Some(node)
        } else {
            declarator_name(node)
        }?;
        let constant = field.kind() == "constant_declaration" || is_constant_field(field, content);
        return Some((if constant { "constant" } else { "property" }, name));
    }
    // A namespace keeps its whole qualified name. An unnamed C-family
    // namespace selects its keyword; its display label is structural, not a binder.
    if matches!(
        node_kind,
        "namespace_declaration" | "file_scoped_namespace_declaration" | "namespace_definition"
    ) {
        return node
            .child_by_field_name("name")
            .or_else(|| {
                if node_kind != "namespace_definition" {
                    return None;
                }
                let mut cursor = node.walk();
                node.children(&mut cursor)
                    .find(|child| child.kind() == "namespace" && !child.is_missing())
            })
            .map(|name| ("namespace", name));
    }
    // A Go `type` or `const` declaration only wraps its specs (`type X …`, a
    // grouped `type ( … )`, `const ( … )`); each spec is the declaration.
    if matches!(node_kind, "type_declaration" | "const_declaration") {
        let mut cursor = node.walk();
        if node
            .named_children(&mut cursor)
            .any(|child| matches!(child.kind(), "type_spec" | "type_alias" | "const_spec"))
        {
            return None;
        }
    }
    if let Some(kind) = declaration_kind(node_kind) {
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
    match node_kind {
        "object_definition" => node
            .child_by_field_name("name")
            .map(|name| ("module", name)),
        "trait_definition" => node.child_by_field_name("name").map(|name| ("trait", name)),
        // A Scala `given` is a term; an anonymous one has only the type it
        // provides to name it.
        "given_definition" => node
            .child_by_field_name("name")
            .or_else(|| node.child_by_field_name("return_type"))
            .map(|name| ("constant", name)),
        // A member or top-level `val` or `var`; a local one is a value of its
        // block, as a local of any other grammar is.
        "val_definition" | "var_definition"
            if !parent()
                .is_some_and(|parent| matches!(parent.kind(), "block" | "indented_block")) =>
        {
            let kind = if node_kind == "val_definition" {
                "constant"
            } else {
                "variable"
            };
            node.child_by_field_name("pattern")
                .filter(|pattern| pattern.kind() == "identifier")
                .map(|name| (kind, name))
        }
        "preproc_def" | "preproc_function_def" => {
            node.child_by_field_name("name").map(|name| ("macro", name))
        }
        // A TypeScript enum member written without a value is its name alone.
        "property_identifier" if parent().is_some_and(|parent| parent.kind() == "enum_body") => {
            Some(("enumMember", node))
        }
        // A Go embedded field: a field declaration of a type alone, named
        // by that type.
        "field_declaration"
            if node.child_by_field_name("name").is_none()
                && node.child_by_field_name("declarator").is_none() =>
        {
            node.child_by_field_name("type")
                .and_then(embedded_type_name)
                .map(|name| ("property", name))
        }
        // C++ member function declarations (`void go();`, a pure virtual).
        "field_declaration" if is_function_prototype(node) => {
            declarator_name(node).map(|name| ("function", name))
        }
        // C/C++ prototypes (`void f(void);`, a header's API) and
        // `const`/`constexpr` variables at file or namespace scope.
        "declaration" if at_file_scope(parent()) && is_function_prototype(node) => {
            declarator_name(node).map(|name| ("function", name))
        }
        "declaration" if at_file_scope(parent()) && has_const_qualifier(node, content) => {
            declarator_name(node).map(|name| ("constant", name))
        }
        // Python `NAME = value` (or `name: T`) at module level, a variable,
        // or in a class body, a class attribute.
        "expression_statement" => {
            // Reject non-assignment statements before resolving the parent:
            // tree-sitter finds a node's parent by searching from the root.
            let assignment = node
                .named_child(0)
                .filter(|child| child.kind() == "assignment")?;
            let left = assignment.child_by_field_name("left")?;
            if left.kind() == "attribute" {
                instance_attribute_init(node, parents(), content)?;
                return left
                    .child_by_field_name("attribute")
                    .map(|name| ("property", name));
            }
            if left.kind() != "identifier" {
                return None;
            }
            let mut parents = parents();
            let unwritten = match parents.next()?.kind() {
                "module" => "variable",
                "block" if parents.next()?.kind() == "class_definition" => "property",
                _ => return None,
            };
            let name = node_text(left, content)?;
            let constant =
                name.chars().any(char::is_alphabetic) && !name.chars().any(char::is_lowercase);
            Some((if constant { "constant" } else { unwritten }, left))
        }
        _ => None,
    }
}

/// The name of the type a Go embedded field embeds: `Reader` of
/// `io.Reader`, `Base` of `*Base`, `List` of `List[int]`.
fn embedded_type_name(ty: Node<'_>) -> Option<Node<'_>> {
    match ty.kind() {
        "type_identifier" => Some(ty),
        "qualified_type" => ty.child_by_field_name("name"),
        "pointer_type" => ty.named_child(0).and_then(embedded_type_name),
        "generic_type" => ty.child_by_field_name("type").and_then(embedded_type_name),
        _ => None,
    }
}

/// The `__init__` whose receiver the Python statement `node` assigns an
/// attribute of (`self.x = …`, `self.x: T = …`), given `node`'s parents
/// nearest first: `node` sits in that method's own body, at any depth of its
/// blocks, and the method is its class's. Another object's attribute, a
/// nested attribute (`self.a.b`), and a statement of a nested function or
/// class declare nothing. The caller keeps the first such assignment of
/// each name.
pub(in crate::signatures) fn instance_attribute_init<'t>(
    node: Node<'t>,
    mut parents: impl Iterator<Item = Node<'t>>,
    content: &str,
) -> Option<Node<'t>> {
    let assignment = node
        .named_child(0)
        .filter(|child| child.kind() == "assignment")?;
    let object = assignment
        .child_by_field_name("left")
        .filter(|left| left.kind() == "attribute")?
        .child_by_field_name("object")
        .filter(|object| object.kind() == "identifier")?;
    let init = parents.find(|parent| {
        matches!(
            parent.kind(),
            "function_definition" | "class_definition" | "lambda" | "module"
        )
    })?;
    if init.kind() != "function_definition"
        || init
            .child_by_field_name("name")
            .and_then(|name| node_text(name, content))
            != Some("__init__")
    {
        return None;
    }
    let mut owner = parents.next()?;
    if owner.kind() == "decorated_definition" {
        owner = parents.next()?;
    }
    if owner.kind() != "block" || parents.next()?.kind() != "class_definition" {
        return None;
    }
    // The receiver is the method's first parameter, whatever its name.
    let receiver = init
        .child_by_field_name("parameters")?
        .named_child(0)
        .and_then(|parameter| match parameter.kind() {
            "identifier" => Some(parameter),
            "typed_parameter" => parameter.named_child(0),
            _ => None,
        })?;
    (node_text(receiver, content).is_some()
        && node_text(receiver, content) == node_text(object, content))
    .then_some(init)
}

/// The node that spans a declaration as written. Some grammars wrap the
/// declaration node in a node that holds its header: C++ and CUDA put
/// `template <…>` in a `template_declaration` around the function, class or
/// alias, and Python puts the `@decorator` lines in a `decorated_definition`
/// around the `def` or `class`. The wrapper starts where the declaration is
/// read from (a doc comment sits above the header), so its range is the
/// declaration's; the name node still places `line`. `parents` is the node's
/// nearest-first parent chain, which a traversal already knows.
pub(in crate::signatures) fn declaration_extent_from_parents<'t>(
    node: Node<'t>,
    parents: impl Iterator<Item = Node<'t>> + Clone,
) -> Node<'t> {
    // A data member is read from its field declaration: its modifiers and
    // type come before its name.
    if let Some(field) = member_field(node, parents.clone()) {
        return field;
    }
    parents
        .take_while(|parent| {
            matches!(
                parent.kind(),
                "template_declaration" | "decorated_definition"
            )
        })
        .last()
        .unwrap_or(node)
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

/// A declaration whose declarator is a function ([`is_function_declarator`]).
fn is_function_prototype(node: Node<'_>) -> bool {
    node.child_by_field_name("declarator")
        .is_some_and(is_function_declarator)
}

/// A declarator that declares a function (through return-type pointer or
/// reference wrappers); `int (*fp)(int)` is a function-pointer variable.
fn is_function_declarator(declarator: Node<'_>) -> bool {
    let mut current = Some(declarator);
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
