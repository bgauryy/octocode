//! Receiver-type facts for the tree-sitter graph lane.
//!
//! A member call `x.m()` / `self.f.m()` carries the syntactic type of its
//! receiver when the file spells it, highest precision first:
//! 1. a declared type: a typed parameter, a typed local (`let x: T`,
//!    `T x = …`, `var x T`, `x: T = …`);
//! 2. a constructor assignment: `T::new(..)`/`T::from_*(..)`/`T::default()`/
//!    `T { .. }`, `new T(..)`, Python `T(..)`, Go `T{..}`/`&T{..}`/`new(T)`/
//!    `NewT(..)`;
//! 3. `self.f` / `this.f` (Go: `recv.f`): the declared or constructed type of
//!    field `f` on the enclosing struct/class.
//!
//! A local is the nearest binding that precedes the call in the enclosing
//! function and whose scope still holds it. A binding whose type cannot be
//! read (destructuring, a factory call, a loop variable) shadows any outer
//! one, so the fact is omitted rather than guessed. Java, C# and C++ fall back
//! from an unbound bare name to a field of the enclosing class.

use std::collections::HashMap;

use tree_sitter::Node;

use super::super::nodes::{ancestors, node_text};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Lang {
    Rust,
    Python,
    Java,
    Go,
    CSharp,
    Cpp,
}

impl Lang {
    fn for_extension(ext: &str) -> Option<Self> {
        Some(match super::canonical_extension(ext) {
            "rs" => Self::Rust,
            "py" | "pyi" => Self::Python,
            "java" => Self::Java,
            "go" => Self::Go,
            "cs" => Self::CSharp,
            "cpp" => Self::Cpp,
            _ => return None,
        })
    }

    /// Declarations that own a set of locals. Closures, lambdas and Go
    /// function literals capture their enclosing function's locals, so they
    /// are not boundaries.
    fn is_function(self, kind: &str) -> bool {
        match self {
            Self::Rust => kind == "function_item",
            Self::Python => kind == "function_definition",
            Self::Java => matches!(
                kind,
                "method_declaration"
                    | "constructor_declaration"
                    | "compact_constructor_declaration"
            ),
            Self::Go => matches!(kind, "function_declaration" | "method_declaration"),
            Self::CSharp => matches!(
                kind,
                "method_declaration"
                    | "constructor_declaration"
                    | "local_function_statement"
                    | "operator_declaration"
                    | "conversion_operator_declaration"
                    | "destructor_declaration"
                    | "accessor_declaration"
            ),
            Self::Cpp => kind == "function_definition",
        }
    }

    /// The object and member of a member access (`x.f`, `x->f`).
    fn member_parts<'t>(self, node: Node<'t>) -> Option<(Node<'t>, Node<'t>)> {
        let (kind, object, member) = match self {
            Self::Rust => ("field_expression", "value", "field"),
            Self::Python => ("attribute", "object", "attribute"),
            Self::Java => ("field_access", "object", "field"),
            Self::Go => ("selector_expression", "operand", "field"),
            Self::CSharp => ("member_access_expression", "expression", "name"),
            Self::Cpp => ("field_expression", "argument", "field"),
        };
        (node.kind() == kind).then_some(())?;
        Some((
            node.child_by_field_name(object)?,
            node.child_by_field_name(member)?,
        ))
    }

    fn is_self(self, node: Node<'_>, content: &str) -> bool {
        match self {
            Self::Rust => node.kind() == "self",
            Self::Python => node.kind() == "identifier" && node_text(node, content) == Some("self"),
            Self::Java | Self::CSharp | Self::Cpp => {
                matches!(node.kind(), "this" | "this_expression")
            }
            Self::Go => false,
        }
    }

    /// Bare names resolve to fields of the enclosing class.
    fn has_implicit_this(self) -> bool {
        matches!(self, Self::Java | Self::CSharp | Self::Cpp)
    }
}

struct Binding {
    name: String,
    ty: Option<String>,
    /// First byte where the binding is visible.
    from: usize,
    /// Byte where its scope ends.
    until: usize,
}

type Fields = HashMap<String, Option<String>>;

/// Per-tree receiver-type resolver; caches function bindings, type fields
/// and the file's named type declarations.
pub(super) struct ReceiverTypes<'t> {
    lang: Lang,
    root: Node<'t>,
    bindings: HashMap<usize, Vec<Binding>>,
    fields: HashMap<usize, Fields>,
    types: Option<HashMap<String, Vec<Node<'t>>>>,
}

impl<'t> ReceiverTypes<'t> {
    pub(super) fn new(ext: &str, root: Node<'t>) -> Option<Self> {
        Some(Self {
            lang: Lang::for_extension(ext)?,
            root,
            bindings: HashMap::new(),
            fields: HashMap::new(),
            types: None,
        })
    }

    /// Receiver type of the member call `call`, or `None`. `path` holds the
    /// ancestors of `call` and `call` itself, root first, as the caller's
    /// traversal already tracks them: the nodes between `call` and its
    /// receiver are never functions or type owners, so `path` stands in for
    /// the receiver's ancestors without a `Node::parent()` climb.
    pub(super) fn receiver_type(
        &mut self,
        call: Node<'t>,
        path: &[Node<'t>],
        content: &str,
    ) -> Option<String> {
        let lang = self.lang;
        let function = if lang == Lang::Java {
            (call.kind() == "method_invocation").then_some(call)?
        } else {
            let function = call.child_by_field_name("function")?;
            if function.kind() == "generic_function" {
                function.child_by_field_name("function")?
            } else {
                function
            }
        };
        let receiver = if lang == Lang::Java {
            function.child_by_field_name("object")?
        } else {
            lang.member_parts(function)?.0
        };
        if receiver.kind() == "identifier" {
            let name = node_text(receiver, content)?;
            if matches!(name, "self" | "this") {
                return None;
            }
            return match self.local(receiver, path, name, content) {
                Some(ty) => ty,
                None if lang.has_implicit_this() => self.enclosing_field(path, name, content, true),
                None => None,
            };
        }
        let (object, member) = lang.member_parts(receiver)?;
        let member = node_text(member, content)?;
        if lang.is_self(object, content) {
            return self.enclosing_field(path, member, content, false);
        }
        if lang == Lang::Go && object.kind() == "identifier" {
            let owner = self.local(object, path, node_text(object, content)?, content)??;
            let declaration = self.unique_type(&owner, content)?;
            return self.fields_of(declaration, content).get(member).cloned()?;
        }
        None
    }

    /// Type of the local `name` visible at `at`: `Some(None)` when it is
    /// bound but its type is unknown, `None` when it is not bound. `path`
    /// holds the enclosing nodes of `at`, root first.
    fn local(
        &mut self,
        at: Node<'t>,
        path: &[Node<'t>],
        name: &str,
        content: &str,
    ) -> Option<Option<String>> {
        let position = at.start_byte();
        for &node in path.iter().rev() {
            if self.lang.is_function(node.kind()) {
                let found = self
                    .bindings_of(node, content)
                    .iter()
                    .filter(|b| b.name == name && b.from <= position && position < b.until)
                    .max_by_key(|b| b.from)
                    .map(|b| b.ty.clone());
                if found.is_some() {
                    return found;
                }
                // Nested Rust fn items do not capture their parent's locals.
                if self.lang == Lang::Rust {
                    return None;
                }
            }
        }
        None
    }

    fn bindings_of(&mut self, function: Node<'t>, content: &str) -> &[Binding] {
        if !self.bindings.contains_key(&function.id()) {
            let mut out = Vec::new();
            // Depth-first, each node with its depth below `function`: `path`
            // keeps the enclosing nodes of the one being visited, so a binding
            // site reads its parents without a root-down `Node::parent` descent.
            let mut pending = vec![(function, 0)];
            let mut path = Vec::new();
            let mut cursor = function.walk();
            while let Some((node, depth)) = pending.pop() {
                path.truncate(depth);
                let up = Up {
                    root: self.root,
                    node,
                    path: &path,
                };
                self.collect_bindings(up, function, content, &mut out);
                path.push(node);
                pending.extend(
                    node.named_children(&mut cursor)
                        .map(|child| (child, depth + 1)),
                );
            }
            self.bindings.insert(function.id(), out);
        }
        &self.bindings[&function.id()]
    }

    fn collect_bindings(
        &self,
        up: Up<'_, 't>,
        function: Node<'t>,
        content: &str,
        out: &mut Vec<Binding>,
    ) {
        match self.lang {
            Lang::Rust => self.rust_bindings(up, content, out),
            Lang::Python => self.python_bindings(up, function, content, out),
            Lang::Java => self.java_bindings(up, content, out),
            Lang::Go => self.go_bindings(up, content, out),
            Lang::CSharp => self.csharp_bindings(up, function, content, out),
            Lang::Cpp => self.cpp_bindings(up, content, out),
        }
    }

    fn rust_bindings(&self, up: Up<'_, 't>, content: &str, out: &mut Vec<Binding>) {
        let node = up.node;
        let pattern = node.child_by_field_name("pattern");
        match node.kind() {
            "parameter" => {
                let Some(pattern) = pattern else { return };
                let until = up.nth(2).unwrap_or(node);
                let ty = node
                    .child_by_field_name("type")
                    .and_then(|ty| self.rust_type(ty, content));
                bind_pattern(pattern, ty, node.end_byte(), until.end_byte(), content, out);
            }
            "closure_parameters" => {
                let until = up.nth(1).unwrap_or(node).end_byte();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if child.kind() != "parameter" {
                        bind_pattern(child, None, child.end_byte(), until, content, out);
                    }
                }
            }
            "let_declaration" => {
                let Some(pattern) = pattern else { return };
                let ty = node
                    .child_by_field_name("type")
                    .and_then(|ty| self.rust_type(ty, content))
                    .or_else(|| {
                        node.child_by_field_name("value")
                            .and_then(|value| self.rust_constructor(value, content))
                    });
                let until = up.nth(1).unwrap_or(node).end_byte();
                bind_pattern(pattern, ty, node.end_byte(), until, content, out);
            }
            "for_expression" | "match_arm" => {
                let Some(pattern) = pattern else { return };
                bind_pattern(
                    pattern,
                    None,
                    pattern.end_byte(),
                    node.end_byte(),
                    content,
                    out,
                );
            }
            "let_condition" => {
                let Some(pattern) = pattern else { return };
                let until = up
                    .ancestor(&["if_expression", "while_expression"])
                    .unwrap_or(node)
                    .end_byte();
                bind_pattern(pattern, None, node.end_byte(), until, content, out);
            }
            _ => {}
        }
    }

    fn python_bindings(
        &self,
        up: Up<'_, 't>,
        function: Node<'t>,
        content: &str,
        out: &mut Vec<Binding>,
    ) {
        let node = up.node;
        // Python locals live until the end of their function.
        let function_end = up
            .ancestor(&["function_definition"])
            .unwrap_or(function)
            .end_byte();
        match node.kind() {
            "parameters" | "lambda_parameters" => {
                let until = up.nth(1).unwrap_or(node).end_byte();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    let (name, ty) = match child.kind() {
                        "typed_parameter" => {
                            let mut inner = child.walk();
                            let name = child
                                .named_children(&mut inner)
                                .find(|item| item.kind() == "identifier");
                            (name, child.child_by_field_name("type"))
                        }
                        "default_parameter" => (child.child_by_field_name("name"), None),
                        "typed_default_parameter" => (
                            child.child_by_field_name("name"),
                            child.child_by_field_name("type"),
                        ),
                        _ => (None, None),
                    };
                    let ty = ty.and_then(|ty| python_type_text(node_text(ty, content)?));
                    let target = name.unwrap_or(child);
                    bind_pattern(target, ty, child.end_byte(), until, content, out);
                }
            }
            "assignment" => {
                let Some(left) = node.child_by_field_name("left") else {
                    return;
                };
                if !matches!(
                    left.kind(),
                    "identifier" | "pattern_list" | "tuple_pattern" | "list_pattern"
                ) {
                    return;
                }
                let ty = node
                    .child_by_field_name("type")
                    .and_then(|ty| python_type_text(node_text(ty, content)?))
                    .or_else(|| {
                        node.child_by_field_name("right")
                            .and_then(|right| python_constructor(right, content))
                    });
                bind_pattern(left, ty, node.end_byte(), function_end, content, out);
            }
            "for_statement" => {
                if let Some(left) = node.child_by_field_name("left") {
                    bind_pattern(left, None, left.end_byte(), function_end, content, out);
                }
            }
            "for_in_clause" => {
                if let Some(left) = node.child_by_field_name("left") {
                    let until = up.nth(1).unwrap_or(node).end_byte();
                    bind_pattern(left, None, node.start_byte(), until, content, out);
                }
            }
            "as_pattern" => {
                if let Some(alias) = node.child_by_field_name("alias") {
                    bind_pattern(alias, None, node.end_byte(), function_end, content, out);
                }
            }
            "named_expression" => {
                if let Some(name) = node.child_by_field_name("name") {
                    let ty = node
                        .child_by_field_name("value")
                        .and_then(|value| python_constructor(value, content));
                    bind_pattern(name, ty, node.end_byte(), function_end, content, out);
                }
            }
            _ => {}
        }
    }

    fn java_bindings(&self, up: Up<'_, 't>, content: &str, out: &mut Vec<Binding>) {
        let node = up.node;
        let declared = |ty: Option<Node<'t>>, value: Option<Node<'t>>| {
            let ty = ty?;
            if node_text(ty, content) == Some("var") {
                value.and_then(|value| java_constructor(value, content))
            } else {
                clean_type_text(node_text(ty, content)?, Lang::Java)
            }
        };
        match node.kind() {
            "formal_parameter" | "catch_formal_parameter" => {
                let Some(name) = node.child_by_field_name("name") else {
                    return;
                };
                let until = up.nth(2).unwrap_or(node);
                let ty = if node.kind() == "catch_formal_parameter" {
                    let mut cursor = node.walk();
                    node.named_children(&mut cursor)
                        .find(|child| child.kind() == "catch_type")
                        .and_then(|ty| clean_type_text(node_text(ty, content)?, Lang::Java))
                } else {
                    declared(node.child_by_field_name("type"), None)
                };
                push(out, name, ty, node.end_byte(), until.end_byte(), content);
            }
            "spread_parameter" => {
                let until = up.nth(2).unwrap_or(node);
                bind_pattern(node, None, node.end_byte(), until.end_byte(), content, out);
            }
            "local_variable_declaration" => {
                let ty = node.child_by_field_name("type");
                let until = up.nth(1).unwrap_or(node).end_byte();
                let mut cursor = node.walk();
                for declarator in node.children_by_field_name("declarator", &mut cursor) {
                    let Some(name) = declarator.child_by_field_name("name") else {
                        continue;
                    };
                    let value = declarator.child_by_field_name("value");
                    push(
                        out,
                        name,
                        declared(ty, value),
                        declarator.end_byte(),
                        until,
                        content,
                    );
                }
            }
            "enhanced_for_statement" | "resource" => {
                let Some(name) = node.child_by_field_name("name") else {
                    return;
                };
                let ty = declared(
                    node.child_by_field_name("type"),
                    node.child_by_field_name("value"),
                );
                let until = if node.kind() == "resource" {
                    up.ancestor(&["try_with_resources_statement"])
                        .unwrap_or(node)
                } else {
                    node
                };
                push(out, name, ty, name.end_byte(), until.end_byte(), content);
            }
            "lambda_expression" => {
                if let Some(parameters) = node.child_by_field_name("parameters")
                    && parameters.kind() != "formal_parameters"
                {
                    bind_pattern(
                        parameters,
                        None,
                        parameters.end_byte(),
                        node.end_byte(),
                        content,
                        out,
                    );
                }
            }
            _ => {}
        }
    }

    fn go_bindings(&self, up: Up<'_, 't>, content: &str, out: &mut Vec<Binding>) {
        let node = up.node;
        match node.kind() {
            "parameter_declaration" | "variadic_parameter_declaration" => {
                let Some(owner) = up.nth(2) else {
                    return;
                };
                if !matches!(
                    owner.kind(),
                    "function_declaration" | "method_declaration" | "func_literal"
                ) {
                    return;
                }
                let ty = (node.kind() == "parameter_declaration")
                    .then(|| node.child_by_field_name("type"))
                    .flatten()
                    .and_then(|ty| clean_type_text(node_text(ty, content)?, Lang::Go));
                let mut cursor = node.walk();
                for name in node.children_by_field_name("name", &mut cursor) {
                    push(
                        out,
                        name,
                        ty.clone(),
                        node.end_byte(),
                        owner.end_byte(),
                        content,
                    );
                }
            }
            "short_var_declaration" => {
                let (Some(left), Some(right)) = (
                    node.child_by_field_name("left"),
                    node.child_by_field_name("right"),
                ) else {
                    return;
                };
                let until = go_scope_end(up);
                let lefts = named_children(left);
                let rights = named_children(right);
                for (index, name) in lefts.iter().enumerate() {
                    let ty = if lefts.len() == rights.len() {
                        go_constructor(rights[index], false, content)
                    } else if index == 0 && rights.len() == 1 {
                        go_constructor(rights[0], true, content)
                    } else {
                        None
                    };
                    bind_pattern(*name, ty, node.end_byte(), until, content, out);
                }
            }
            "var_spec" => {
                let until = go_scope_end(up);
                let declared = node
                    .child_by_field_name("type")
                    .and_then(|ty| clean_type_text(node_text(ty, content)?, Lang::Go));
                let values = node
                    .child_by_field_name("value")
                    .map(named_children)
                    .unwrap_or_default();
                let mut cursor = node.walk();
                let names: Vec<_> = node.children_by_field_name("name", &mut cursor).collect();
                for (index, name) in names.iter().enumerate() {
                    let ty = declared.clone().or_else(|| {
                        if values.len() == names.len() {
                            go_constructor(values[index], false, content)
                        } else if index == 0 && values.len() == 1 {
                            go_constructor(values[0], true, content)
                        } else {
                            None
                        }
                    });
                    push(out, *name, ty, node.end_byte(), until, content);
                }
            }
            "range_clause" => {
                if let Some(left) = node.child_by_field_name("left") {
                    let until = up.nth(1).unwrap_or(node).end_byte();
                    bind_pattern(left, None, node.end_byte(), until, content, out);
                }
            }
            "type_switch_statement" => {
                if let Some(alias) = node.child_by_field_name("alias") {
                    bind_pattern(alias, None, alias.end_byte(), node.end_byte(), content, out);
                }
            }
            _ => {}
        }
    }

    fn csharp_bindings(
        &self,
        up: Up<'_, 't>,
        function: Node<'t>,
        content: &str,
        out: &mut Vec<Binding>,
    ) {
        let node = up.node;
        let declared = |ty: Option<Node<'t>>| {
            let ty = ty?;
            (ty.kind() != "implicit_type")
                .then(|| clean_type_text(node_text(ty, content)?, Lang::CSharp))
                .flatten()
        };
        match node.kind() {
            "parameter" => {
                let Some(name) = node.child_by_field_name("name") else {
                    return;
                };
                let until = up.nth(2).unwrap_or(node);
                let ty = declared(node.child_by_field_name("type"));
                push(out, name, ty, node.end_byte(), until.end_byte(), content);
            }
            "variable_declaration" => {
                let Some(parent) = up.nth(1) else { return };
                if matches!(
                    parent.kind(),
                    "field_declaration" | "event_field_declaration"
                ) {
                    return;
                }
                let scope = if parent.kind() == "local_declaration_statement" {
                    up.nth(2).unwrap_or(parent)
                } else {
                    parent
                };
                let ty = node.child_by_field_name("type");
                let mut cursor = node.walk();
                for declarator in node.named_children(&mut cursor) {
                    if declarator.kind() != "variable_declarator" {
                        continue;
                    }
                    let Some(name) = declarator.child_by_field_name("name") else {
                        continue;
                    };
                    let resolved = if ty.is_some_and(|ty| ty.kind() == "implicit_type") {
                        csharp_initializer(declarator, name)
                            .and_then(|value| csharp_constructor(value, content))
                    } else {
                        declared(ty)
                    };
                    push(
                        out,
                        name,
                        resolved,
                        declarator.end_byte(),
                        scope.end_byte(),
                        content,
                    );
                }
            }
            "foreach_statement" => {
                if let Some(left) = node.child_by_field_name("left") {
                    let ty = declared(node.child_by_field_name("type"));
                    bind_pattern(left, ty, left.end_byte(), node.end_byte(), content, out);
                }
            }
            "declaration_pattern" | "declaration_expression" => {
                if let Some(name) = node.child_by_field_name("name") {
                    let ty = declared(node.child_by_field_name("type"));
                    let until = up.ancestor(&[function.kind()]).unwrap_or(function);
                    bind_pattern(name, ty, node.end_byte(), until.end_byte(), content, out);
                }
            }
            "catch_declaration" => {
                if let Some(name) = node.child_by_field_name("name") {
                    let ty = declared(node.child_by_field_name("type"));
                    let until = up.nth(1).unwrap_or(node).end_byte();
                    push(out, name, ty, node.end_byte(), until, content);
                }
            }
            "lambda_expression" => {
                if let Some(parameters) = node.child_by_field_name("parameters")
                    && parameters.kind() != "parameter_list"
                {
                    bind_pattern(
                        parameters,
                        None,
                        parameters.end_byte(),
                        node.end_byte(),
                        content,
                        out,
                    );
                }
            }
            _ => {}
        }
    }

    fn cpp_bindings(&self, up: Up<'_, 't>, content: &str, out: &mut Vec<Binding>) {
        let node = up.node;
        let declared = |ty: Option<Node<'t>>| {
            let ty = ty?;
            (ty.kind() != "placeholder_type_specifier")
                .then(|| clean_type_text(node_text(ty, content)?, Lang::Cpp))
                .flatten()
        };
        match node.kind() {
            "parameter_declaration" | "optional_parameter_declaration" => {
                let Some(name) = node
                    .child_by_field_name("declarator")
                    .and_then(cpp_declarator_name)
                else {
                    return;
                };
                let until = up
                    .ancestor(&["function_definition", "lambda_expression"])
                    .unwrap_or(node)
                    .end_byte();
                let ty = declared(node.child_by_field_name("type"));
                push(out, name, ty, node.end_byte(), until, content);
            }
            "declaration" => {
                let ty = node.child_by_field_name("type");
                let until = up.nth(1).unwrap_or(node).end_byte();
                let mut cursor = node.walk();
                for declarator in node.children_by_field_name("declarator", &mut cursor) {
                    let (target, value) = if declarator.kind() == "init_declarator" {
                        (
                            declarator.child_by_field_name("declarator"),
                            declarator.child_by_field_name("value"),
                        )
                    } else {
                        (Some(declarator), None)
                    };
                    let Some(name) = target.and_then(cpp_declarator_name) else {
                        continue;
                    };
                    let resolved = if ty.is_some_and(|ty| ty.kind() == "placeholder_type_specifier")
                    {
                        value.and_then(|value| cpp_constructor(value, content))
                    } else {
                        declared(ty)
                    };
                    push(out, name, resolved, declarator.end_byte(), until, content);
                }
            }
            "for_range_loop" => {
                if let Some(name) = node
                    .child_by_field_name("declarator")
                    .and_then(cpp_declarator_name)
                {
                    let ty = declared(node.child_by_field_name("type"));
                    push(out, name, ty, name.end_byte(), node.end_byte(), content);
                }
            }
            _ => {}
        }
    }

    fn rust_type(&self, ty: Node<'t>, content: &str) -> Option<String> {
        let cleaned = clean_type_text(node_text(ty, content)?, Lang::Rust)?;
        self.rust_self(ty, cleaned, content)
    }

    /// `Self` names the enclosing impl's type.
    fn rust_self(&self, at: Node<'t>, ty: String, content: &str) -> Option<String> {
        if ty != "Self" {
            return Some(ty);
        }
        let implementation = ancestor(self.root, at, &["impl_item"])?;
        clean_type_text(
            node_text(implementation.child_by_field_name("type")?, content)?,
            Lang::Rust,
        )
    }

    /// `T::new(..)`, `T::from_*(..)`, `T::default()`, `T::with_*(..)` and
    /// `T { .. }`, through `?`, `.await` and `&`.
    fn rust_constructor(&self, value: Node<'t>, content: &str) -> Option<String> {
        let mut value = value;
        loop {
            value = match value.kind() {
                "try_expression" | "await_expression" | "parenthesized_expression" => {
                    value.named_child(0)?
                }
                "reference_expression" => value.child_by_field_name("value")?,
                _ => break,
            };
        }
        let ty = match value.kind() {
            "call_expression" => {
                let function = value.child_by_field_name("function")?;
                if function.kind() != "scoped_identifier" {
                    return None;
                }
                let name = node_text(function.child_by_field_name("name")?, content)?;
                let constructor = name == "new"
                    || name == "default"
                    || name.starts_with("new_")
                    || name.starts_with("from")
                    || name.starts_with("with_");
                if !constructor {
                    return None;
                }
                clean_type_text(
                    node_text(function.child_by_field_name("path")?, content)?,
                    Lang::Rust,
                )?
            }
            "struct_expression" => clean_type_text(
                node_text(value.child_by_field_name("name")?, content)?,
                Lang::Rust,
            )?,
            _ => return None,
        };
        starts_uppercase(last_segment(&ty)).then_some(())?;
        self.rust_self(value, ty, content)
    }

    /// The type of member `name` on the class/struct enclosing `at`. A bare
    /// name (`unqualified`) also searches outer classes.
    /// Field `name` of the type that encloses `path` (root first, as in
    /// [`Self::receiver_type`]).
    fn enclosing_field(
        &mut self,
        path: &[Node<'t>],
        name: &str,
        content: &str,
        unqualified: bool,
    ) -> Option<String> {
        let owners: &[&str] = match self.lang {
            Lang::Rust => {
                let implementation = path[nearest(path, &["impl_item"])?];
                let ty = clean_type_text(
                    node_text(implementation.child_by_field_name("type")?, content)?,
                    Lang::Rust,
                )?;
                let declaration = self.unique_type(last_segment(&ty), content)?;
                return self.fields_of(declaration, content).get(name).cloned()?;
            }
            Lang::Go => return None,
            Lang::Python => &["class_definition"],
            Lang::Java => &[
                "class_declaration",
                "enum_declaration",
                "record_declaration",
                "interface_declaration",
            ],
            Lang::CSharp => &[
                "class_declaration",
                "struct_declaration",
                "record_declaration",
                "record_struct_declaration",
                "interface_declaration",
            ],
            Lang::Cpp => &["class_specifier", "struct_specifier"],
        };
        // Index in `path` of the current owner; `None` once the owner came
        // from elsewhere in the file (an out-of-line C++ member's class).
        let mut index = nearest(path, owners);
        let mut current = index.map(|i| path[i]);
        if current.is_none() && self.lang == Lang::Cpp {
            // Out-of-line member `void Svc::run() { .. }`.
            let function = path[nearest(path, &["function_definition"])?];
            let declarator = function.child_by_field_name("declarator")?;
            let qualified = declarator.child_by_field_name("declarator")?;
            let owner = node_text(qualified.child_by_field_name("scope")?, content)?;
            current = self.unique_type(owner, content);
        }
        while let Some(owner) = current {
            if let Some(ty) = self.fields_of(owner, content).get(name) {
                return ty.clone();
            }
            if !unqualified {
                return None;
            }
            current = match index {
                Some(i) => {
                    index = nearest(&path[..i], owners);
                    index.map(|j| path[j])
                }
                None => ancestor(self.root, owner, owners),
            };
        }
        None
    }

    /// The only declaration of the named struct/class in this file.
    fn unique_type(&mut self, name: &str, content: &str) -> Option<Node<'t>> {
        let lang = self.lang;
        let root = self.root;
        let types = self.types.get_or_insert_with(|| {
            let mut types: HashMap<String, Vec<Node<'t>>> = HashMap::new();
            let mut pending = vec![root];
            let mut cursor = root.walk();
            while let Some(node) = pending.pop() {
                let declaration = match (lang, node.kind()) {
                    (Lang::Rust, "struct_item")
                    | (Lang::Cpp, "class_specifier" | "struct_specifier") => {
                        node.child_by_field_name("body").map(|_| node)
                    }
                    (Lang::Go, "type_spec") => node
                        .child_by_field_name("type")
                        .filter(|ty| ty.kind() == "struct_type")
                        .map(|_| node),
                    _ => None,
                };
                if let Some(declaration) = declaration
                    && let Some(name) = declaration
                        .child_by_field_name("name")
                        .and_then(|name| node_text(name, content))
                {
                    types.entry(name.to_owned()).or_default().push(declaration);
                }
                pending.extend(node.named_children(&mut cursor));
            }
            types
        });
        match types.get(name).map(Vec::as_slice) {
            Some([only]) => Some(*only),
            _ => None,
        }
    }

    fn fields_of(&mut self, owner: Node<'t>, content: &str) -> &Fields {
        if !self.fields.contains_key(&owner.id()) {
            let fields = match self.lang {
                Lang::Python => self.python_fields(owner, content),
                _ => self.declared_fields(owner, content),
            };
            self.fields.insert(owner.id(), fields);
        }
        &self.fields[&owner.id()]
    }

    fn declared_fields(&self, owner: Node<'t>, content: &str) -> Fields {
        let lang = self.lang;
        let mut fields = Fields::new();
        let mut add = |name: Node<'_>, ty: Option<String>| {
            if let Some(name) = node_text(name, content) {
                merge(&mut fields, name, ty);
            }
        };
        let body = match lang {
            Lang::Go => owner
                .child_by_field_name("type")
                .and_then(|ty| ty.named_child(0))
                .filter(|list| list.kind() == "field_declaration_list"),
            _ => owner.child_by_field_name("body"),
        };
        let Some(body) = body else { return fields };
        let mut members = named_children(body);
        if let Some(extra) = members
            .iter()
            .find(|member| member.kind() == "enum_body_declarations")
        {
            members.extend(named_children(*extra));
        }
        // Record components and C# primary-constructor parameters.
        if let Some(parameters) = owner.child_by_field_name("parameters").or_else(|| {
            (lang == Lang::CSharp)
                .then(|| named_children(owner))
                .and_then(|children| children.into_iter().find(|c| c.kind() == "parameter_list"))
        }) {
            members.extend(named_children(parameters));
        }
        for member in members {
            match (lang, member.kind()) {
                (Lang::Rust, "field_declaration") => {
                    if let (Some(name), Some(ty)) = (
                        member.child_by_field_name("name"),
                        member.child_by_field_name("type"),
                    ) {
                        add(name, self.rust_type(ty, content));
                    }
                }
                (Lang::Go, "field_declaration") => {
                    let ty = member
                        .child_by_field_name("type")
                        .and_then(|ty| clean_type_text(node_text(ty, content)?, Lang::Go));
                    let mut cursor = member.walk();
                    for name in member.children_by_field_name("name", &mut cursor) {
                        add(name, ty.clone());
                    }
                }
                (Lang::Java, "field_declaration" | "constant_declaration") => {
                    let ty = member
                        .child_by_field_name("type")
                        .and_then(|ty| clean_type_text(node_text(ty, content)?, Lang::Java));
                    let mut cursor = member.walk();
                    for declarator in member.children_by_field_name("declarator", &mut cursor) {
                        if let Some(name) = declarator.child_by_field_name("name") {
                            add(name, ty.clone());
                        }
                    }
                }
                (Lang::Java, "formal_parameter")
                | (Lang::CSharp, "parameter" | "property_declaration") => {
                    if let (Some(name), Some(ty)) = (
                        member.child_by_field_name("name"),
                        member.child_by_field_name("type"),
                    ) {
                        add(
                            name,
                            node_text(ty, content).and_then(|ty| clean_type_text(ty, lang)),
                        );
                    }
                }
                (Lang::CSharp, "field_declaration") => {
                    let Some(declaration) = named_children(member)
                        .into_iter()
                        .find(|child| child.kind() == "variable_declaration")
                    else {
                        continue;
                    };
                    let ty = declaration
                        .child_by_field_name("type")
                        .and_then(|ty| clean_type_text(node_text(ty, content)?, Lang::CSharp));
                    for declarator in named_children(declaration) {
                        if declarator.kind() == "variable_declarator"
                            && let Some(name) = declarator.child_by_field_name("name")
                        {
                            add(name, ty.clone());
                        }
                    }
                }
                (Lang::Cpp, "field_declaration") => {
                    let ty = member
                        .child_by_field_name("type")
                        .and_then(|ty| clean_type_text(node_text(ty, content)?, Lang::Cpp));
                    let mut cursor = member.walk();
                    for declarator in member.children_by_field_name("declarator", &mut cursor) {
                        if declarator.kind() != "function_declarator"
                            && let Some(name) = cpp_declarator_name(declarator)
                        {
                            add(name, ty.clone());
                        }
                    }
                }
                _ => {}
            }
        }
        // A Rust tuple struct's positional fields: `self.0`.
        if lang == Lang::Rust && body.kind() == "ordered_field_declaration_list" {
            let mut cursor = body.walk();
            for (index, ty) in body.children_by_field_name("type", &mut cursor).enumerate() {
                merge(&mut fields, &index.to_string(), self.rust_type(ty, content));
            }
        }
        fields
    }

    /// Class-body annotations (`f: T`) are declared; `self.f = …` assignments
    /// in methods are inferred from a constructor call or a typed parameter,
    /// and any other assignment makes an inferred field unknown.
    fn python_fields(&mut self, class: Node<'t>, content: &str) -> Fields {
        let mut declared = Fields::new();
        let mut inferred = Fields::new();
        let Some(body) = class.child_by_field_name("body") else {
            return declared;
        };
        let mut methods = Vec::new();
        for statement in named_children(body) {
            match statement.kind() {
                "expression_statement" => {
                    let Some(assignment) = statement
                        .named_child(0)
                        .filter(|a| a.kind() == "assignment")
                    else {
                        continue;
                    };
                    let Some(left) = assignment
                        .child_by_field_name("left")
                        .filter(|left| left.kind() == "identifier")
                        .and_then(|left| node_text(left, content))
                    else {
                        continue;
                    };
                    if let Some(ty) = assignment.child_by_field_name("type") {
                        merge(
                            &mut declared,
                            left,
                            python_type_text(node_text(ty, content).unwrap_or("")),
                        );
                    } else if let Some(right) = assignment.child_by_field_name("right") {
                        merge(&mut inferred, left, python_constructor(right, content));
                    }
                }
                "function_definition" => methods.push(statement),
                "decorated_definition" => {
                    if let Some(definition) = statement
                        .child_by_field_name("definition")
                        .filter(|d| d.kind() == "function_definition")
                    {
                        methods.push(definition);
                    }
                }
                _ => {}
            }
        }
        for method in methods {
            let mut pending = vec![method];
            let mut cursor = method.walk();
            while let Some(node) = pending.pop() {
                if node.kind() == "class_definition" {
                    continue;
                }
                if node.kind() == "assignment"
                    && let Some(left) = node.child_by_field_name("left")
                    && let Some((object, attribute)) = Lang::Python.member_parts(left)
                    && Lang::Python.is_self(object, content)
                    && let Some(attribute) = node_text(attribute, content)
                {
                    if let Some(ty) = node.child_by_field_name("type") {
                        merge(
                            &mut declared,
                            attribute,
                            python_type_text(node_text(ty, content).unwrap_or("")),
                        );
                    } else if let Some(right) = node.child_by_field_name("right")
                        && right.kind() != "none"
                    {
                        let ty = python_constructor(right, content).or_else(|| {
                            (right.kind() == "identifier")
                                .then(|| {
                                    let path = ancestors(self.root, right);
                                    self.local(right, &path, node_text(right, content)?, content)
                                })
                                .flatten()
                                .flatten()
                        });
                        merge(&mut inferred, attribute, ty);
                    }
                }
                pending.extend(node.named_children(&mut cursor));
            }
        }
        inferred.extend(declared);
        inferred
    }
}

/// Record `ty` for `name`; a second, different type makes it unknown.
fn merge(fields: &mut Fields, name: &str, ty: Option<String>) {
    match fields.get_mut(name) {
        Some(existing) if *existing != ty => *existing = None,
        Some(_) => {}
        None => {
            fields.insert(name.to_owned(), ty);
        }
    }
}

fn push(
    out: &mut Vec<Binding>,
    name: Node<'_>,
    ty: Option<String>,
    from: usize,
    until: usize,
    content: &str,
) {
    if let Some(name) = node_text(name, content) {
        out.push(Binding {
            name: name.to_owned(),
            ty,
            from,
            until,
        });
    }
}

/// Bind a plain name to `ty`; every name inside a destructuring pattern is
/// bound with an unknown type so it shadows outer bindings.
fn bind_pattern(
    pattern: Node<'_>,
    ty: Option<String>,
    from: usize,
    until: usize,
    content: &str,
    out: &mut Vec<Binding>,
) {
    if pattern.kind() == "identifier" {
        push(out, pattern, ty, from, until, content);
        return;
    }
    let mut pending = vec![pattern];
    let mut cursor = pattern.walk();
    while let Some(node) = pending.pop() {
        if node.kind() == "identifier" {
            push(out, node, None, from, until, content);
        }
        pending.extend(node.named_children(&mut cursor));
    }
}

fn named_children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

/// Index of the last node in `path` whose kind is in `kinds`.
fn nearest(path: &[Node<'_>], kinds: &[&str]) -> Option<usize> {
    path.iter().rposition(|node| kinds.contains(&node.kind()))
}

/// The nearest ancestor of `node` whose kind is in `kinds`, found in one
/// root-down descent (the last match on the way down is the nearest).
fn ancestor<'t>(root: Node<'t>, node: Node<'t>, kinds: &[&str]) -> Option<Node<'t>> {
    let mut nearest = None;
    let mut current = root;
    while current.id() != node.id() {
        if kinds.contains(&current.kind()) {
            nearest = Some(current);
        }
        current = current.child_with_descendant(node)?;
    }
    nearest
}

fn last_segment(ty: &str) -> &str {
    ty.rsplit([':', '.']).next().unwrap_or(ty)
}

fn starts_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

/// The end of the block that holds a Go declaration statement.
fn go_scope_end(up: Up<'_, '_>) -> usize {
    let mut level = 1;
    while let Some(parent) = up.nth(level) {
        match parent.kind() {
            "var_declaration" | "var_spec_list" => level += 1,
            "statement_list" => return up.nth(level + 1).unwrap_or(parent).end_byte(),
            _ => return parent.end_byte(),
        }
    }
    up.node.end_byte()
}

/// A binding site and its enclosing nodes inside the function being scanned
/// (`path`, function first). Ancestors above the function come from one
/// root-down descent, as [`ancestor`] and `Node::parent` give them.
#[derive(Clone, Copy)]
struct Up<'p, 't> {
    root: Node<'t>,
    node: Node<'t>,
    path: &'p [Node<'t>],
}

impl<'t> Up<'_, 't> {
    /// The `level`-th ancestor (1 is the parent), `None` above the root.
    fn nth(&self, level: usize) -> Option<Node<'t>> {
        if level == 0 {
            return Some(self.node);
        }
        if let Some(index) = self.path.len().checked_sub(level) {
            return Some(self.path[index]);
        }
        let top = self.path.first().copied().unwrap_or(self.node);
        let above = level - self.path.len();
        let chain = ancestors(self.root, top);
        chain.len().checked_sub(above).map(|index| chain[index])
    }

    /// The nearest strict ancestor whose kind is one of `kinds`.
    fn ancestor(&self, kinds: &[&str]) -> Option<Node<'t>> {
        if let Some(found) = self
            .path
            .iter()
            .rev()
            .find(|node| kinds.contains(&node.kind()))
        {
            return Some(*found);
        }
        ancestor(
            self.root,
            self.path.first().copied().unwrap_or(self.node),
            kinds,
        )
    }
}

/// `T{..}`, `&T{..}`, `new(T)`, `NewT(..)` → `T`, `pkg.NewT(..)` → `pkg.T`.
/// A multi-value assignment (`x, err := …`) only reads constructor calls.
fn go_constructor(value: Node<'_>, multi: bool, content: &str) -> Option<String> {
    let literal = |node: Node<'_>| {
        (node.kind() == "composite_literal")
            .then(|| {
                clean_type_text(
                    node_text(node.child_by_field_name("type")?, content)?,
                    Lang::Go,
                )
            })
            .flatten()
    };
    match value.kind() {
        "composite_literal" if !multi => literal(value),
        "unary_expression" if !multi => {
            let operator = value.child_by_field_name("operator")?;
            (node_text(operator, content) == Some("&")).then_some(())?;
            literal(value.child_by_field_name("operand")?)
        }
        "call_expression" => {
            let function = value.child_by_field_name("function")?;
            let (package, name) = match function.kind() {
                "identifier" => (None, node_text(function, content)?),
                "selector_expression" => {
                    let package = function.child_by_field_name("operand")?;
                    (package.kind() == "identifier").then_some(())?;
                    (
                        Some(node_text(package, content)?),
                        node_text(function.child_by_field_name("field")?, content)?,
                    )
                }
                _ => return None,
            };
            if package.is_none() && name == "new" && !multi {
                let argument = value.child_by_field_name("arguments")?.named_child(0)?;
                return clean_type_text(node_text(argument, content)?, Lang::Go);
            }
            let ty = name
                .strip_prefix("New")
                .filter(|rest| starts_uppercase(rest))?;
            Some(match package {
                Some(package) => format!("{package}.{ty}"),
                None => ty.to_owned(),
            })
        }
        _ => None,
    }
}

fn java_constructor(value: Node<'_>, content: &str) -> Option<String> {
    (value.kind() == "object_creation_expression").then_some(())?;
    clean_type_text(
        node_text(value.child_by_field_name("type")?, content)?,
        Lang::Java,
    )
}

/// The initializer of a C# `variable_declarator` (`x = value`).
fn csharp_initializer<'t>(declarator: Node<'t>, name: Node<'t>) -> Option<Node<'t>> {
    let value = named_children(declarator)
        .into_iter()
        .rfind(|child| *child != name && child.kind() != "bracketed_argument_list")?;
    if value.kind() == "equals_value_clause" {
        value.named_child(0)
    } else {
        Some(value)
    }
}

fn csharp_constructor(value: Node<'_>, content: &str) -> Option<String> {
    (value.kind() == "object_creation_expression").then_some(())?;
    clean_type_text(
        node_text(value.child_by_field_name("type")?, content)?,
        Lang::CSharp,
    )
}

fn cpp_constructor(value: Node<'_>, content: &str) -> Option<String> {
    (value.kind() == "new_expression").then_some(())?;
    clean_type_text(
        node_text(value.child_by_field_name("type")?, content)?,
        Lang::Cpp,
    )
}

/// The name a C++ declarator binds, through pointer and reference
/// declarators; arrays and function declarators bind no typed object.
fn cpp_declarator_name(declarator: Node<'_>) -> Option<Node<'_>> {
    let mut current = declarator;
    loop {
        current = match current.kind() {
            "identifier" | "field_identifier" => return Some(current),
            "pointer_declarator" | "reference_declarator" => current
                .child_by_field_name("declarator")
                .or_else(|| current.named_child(0))?,
            _ => return None,
        };
    }
}

/// Python `T(..)` / `mod.T(..)` where `T` starts uppercase.
fn python_constructor(value: Node<'_>, content: &str) -> Option<String> {
    (value.kind() == "call").then_some(())?;
    let function = value.child_by_field_name("function")?;
    if !matches!(function.kind(), "identifier" | "attribute") {
        return None;
    }
    let name: String = node_text(function, content)?.split_whitespace().collect();
    let valid = name.split('.').all(is_identifier) && starts_uppercase(last_segment(&name));
    valid.then_some(name)
}

/// A Python annotation as a type name: string forward references are read,
/// `Optional[T]` and `T | None` unwrap to `T`, subscripts are dropped.
pub(super) fn python_type_text(text: &str) -> Option<String> {
    let mut text = text.trim();
    for quote in ['"', '\''] {
        if let Some(inner) = text.strip_prefix(quote).and_then(|t| t.strip_suffix(quote)) {
            text = inner.trim();
        }
    }
    for prefix in ["Optional[", "typing.Optional["] {
        if let Some(inner) = text.strip_prefix(prefix).and_then(|t| t.strip_suffix(']')) {
            return python_type_text(inner);
        }
    }
    let parts: Vec<&str> = split_top_level(text, '|')
        .into_iter()
        .map(str::trim)
        .filter(|part| *part != "None")
        .collect();
    match parts.as_slice() {
        [only] if *only != text => python_type_text(only),
        [only] => clean_generic(only, '[', ']').filter(|ty| ty.split('.').all(is_identifier)),
        _ => None,
    }
}

/// A type as written, reduced to its (possibly qualified) name: references,
/// pointers, `mut`/`const`/`dyn`/`impl`, lifetimes, trailing `?`, auto-trait
/// bounds and generic arguments are stripped; Rust `Box`/`Rc`/`Arc` unwrap to
/// their content. Arrays, slices, tuples, maps and function types are `None`.
pub(super) fn clean_type_text(text: &str, lang: Lang) -> Option<String> {
    let mut text = text.trim();
    loop {
        let before = text;
        for prefix in [
            "&",
            "*",
            "mut ",
            "const ",
            "dyn ",
            "impl ",
            "struct ",
            "class ",
            "enum ",
            "typename ",
        ] {
            if let Some(rest) = text.strip_prefix(prefix) {
                text = rest.trim_start();
            }
        }
        if text.starts_with('\'') {
            // A Rust lifetime: `&'a T`.
            text = text
                .split_once(' ')
                .map_or("", |(_, rest)| rest)
                .trim_start();
        }
        text = text.trim_end_matches(['?', '*', '&']).trim_end();
        if text == before {
            break;
        }
    }
    let text = split_top_level(text, '+').first()?.trim();
    let (open, close) = match lang {
        Lang::Go | Lang::Python => ('[', ']'),
        _ => ('<', '>'),
    };
    if lang == Lang::Rust
        && let Some((base, arguments)) = generic_parts(text, open, close)
        && matches!(last_segment(base), "Box" | "Rc" | "Arc")
        && let [inner] = split_top_level(arguments, ',').as_slice()
    {
        return clean_type_text(inner, lang);
    }
    let name = clean_generic(text, open, close)?;
    let separator = if matches!(lang, Lang::Rust | Lang::Cpp) {
        "::"
    } else {
        "."
    };
    name.split(separator).all(is_identifier).then_some(name)
}

/// `base` and the argument text of `base<args>`.
fn generic_parts(text: &str, open: char, close: char) -> Option<(&str, &str)> {
    let start = text.find(open)?;
    let inner = text[start + 1..].strip_suffix(close)?;
    Some((text[..start].trim_end_matches("::"), inner))
}

/// `text` without a trailing generic argument list.
fn clean_generic(text: &str, open: char, close: char) -> Option<String> {
    if text.starts_with(open) {
        return None;
    }
    let base = match text.find(open) {
        Some(start) => {
            text.ends_with(close).then_some(())?;
            text[..start].trim_end_matches("::")
        }
        None => text,
    };
    (!base.is_empty()).then(|| base.to_owned())
}

/// Split at `separator` outside `<>`, `[]` and `()`.
fn split_top_level(text: &str, separator: char) -> Vec<&str> {
    let mut depth = 0i32;
    let mut parts = Vec::new();
    let mut start = 0;
    for (index, ch) in text.char_indices() {
        match ch {
            '<' | '[' | '(' => depth += 1,
            '>' | ']' | ')' => depth -= 1,
            _ if ch == separator && depth == 0 => {
                parts.push(&text[start..index]);
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

fn is_identifier(segment: &str) -> bool {
    let mut chars = segment.chars();
    chars
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_alphanumeric() || ch == '_')
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    /// `method name -> receiverType` of every call in `source`. Fixture
    /// methods under test are unique, so each lookup names exactly one call.
    fn receivers(source: &str, path: &str) -> BTreeMap<String, Vec<Option<String>>> {
        let json = super::super::tests::extract_graph_facts(source, path).expect("graph facts");
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let mut out: BTreeMap<String, Vec<Option<String>>> = BTreeMap::new();
        for call in value["calls"].as_array().unwrap() {
            let callee = call["callee"].as_str().unwrap();
            let callee = callee.split("::<").next().unwrap();
            let method = callee.rsplit(['.', ':', '>']).next().unwrap().to_owned();
            let receiver = call["receiverType"].as_str().map(str::to_owned);
            out.entry(method).or_default().push(receiver);
        }
        out
    }

    fn assert_receivers(source: &str, path: &str, expected: &[(&str, Option<&str>)]) {
        let actual = receivers(source, path);
        for (method, receiver) in expected {
            let found = actual
                .get(*method)
                .map(|items| items.iter().map(Option::as_deref).collect::<Vec<_>>());
            assert_eq!(
                found,
                Some(vec![*receiver]),
                "{path}: receiverType of `{method}` in {actual:?}"
            );
        }
    }

    #[test]
    fn rust_receiver_types_come_from_declarations_constructors_and_fields() {
        let source = r#"
struct Svc { store: Arc<Store>, cache: crate::cache::Cache, name: &'static str }
impl Svc {
    fn run(&self, param: &mut Repo, boxed: Box<dyn Sink>, list: &[Item]) {
        let mut a: Vec<Item> = Vec::new();
        let s = Store::new();
        let t = Other::from_config(c)?;
        let u = crate::store::Store { x: 1 };
        let d = Self::default();
        s.save_a();
        t.load_b();
        u.flush_c();
        d.reset_d();
        a.push_e(1);
        param.find_f();
        boxed.write_g();
        self.store.save_h();
        self.cache.get_i();
        self.helper_j();
        list.iter_k();
        unknown.call_l();
        let q = |p: Pool| p.take_m();
        if let Some(v) = maybe { v.opt_n(); }
        {
            let t = Thing::new();
            t.inner_o();
        }
        t.outer_p();
        x.turbo_q::<u8>();
        let s = make();
        s.shadow_r();
        let fresh = s.fresh_s();
    }
}
"#;
        assert_receivers(
            source,
            "src/svc.rs",
            &[
                ("save_a", Some("Store")),
                ("load_b", Some("Other")),
                ("flush_c", Some("crate::store::Store")),
                ("reset_d", Some("Svc")),
                ("push_e", Some("Vec")),
                ("find_f", Some("Repo")),
                ("write_g", Some("Sink")),
                ("save_h", Some("Store")),
                ("get_i", Some("crate::cache::Cache")),
                ("helper_j", None),
                ("iter_k", None),
                ("call_l", None),
                ("take_m", Some("Pool")),
                ("opt_n", None),
                ("inner_o", Some("Thing")),
                ("outer_p", Some("Other")),
                ("turbo_q", None),
                ("shadow_r", None),
                ("fresh_s", None),
            ],
        );
    }

    #[test]
    fn callee_stays_as_written_when_a_receiver_type_is_set() {
        let json = super::super::tests::extract_graph_facts(
            "struct S { store: Store }\nimpl S { fn f(&self) { self.store.save(); } }",
            "src/lib.rs",
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let call = &value["calls"][0];
        assert_eq!(call["callee"], "self.store.save");
        assert_eq!(call["receiverType"], "Store");
    }

    #[test]
    fn rust_let_initializer_sees_the_previous_binding() {
        let source = "fn f(x: Repo) { let x = x.open_a(); x.use_b(); }";
        assert_receivers(
            source,
            "src/lib.rs",
            &[("open_a", Some("Repo")), ("use_b", None)],
        );
    }

    #[test]
    fn python_receiver_types_come_from_annotations_constructors_and_init_fields() {
        let source = r#"
class Svc:
    repo: Repo

    def __init__(self, store: Store, cache=None):
        self.store = store
        self.helper = Helper()
        self.cache = cache
        self.conn: "Conn" = connect()

    def run(self, db: models.Database, opt: Optional[Client] = None, *rest):
        s = Store()
        m = models.Mapper(1)
        n: Node = build()
        s.save_a()
        m.map_b()
        n.visit_c()
        db.query_d()
        opt.send_e()
        self.store.put_f()
        self.helper.help_g()
        self.repo.find_h()
        self.cache.get_i()
        self.conn.open_j()
        low = factory()
        low.lower_k()
        for item in items:
            item.loop_l()
        s = make()
        s.shadow_m()
        self.own_n()
"#;
        assert_receivers(
            source,
            "pkg/svc.py",
            &[
                ("save_a", Some("Store")),
                ("map_b", Some("models.Mapper")),
                ("visit_c", Some("Node")),
                ("query_d", Some("models.Database")),
                ("send_e", Some("Client")),
                ("put_f", Some("Store")),
                ("help_g", Some("Helper")),
                ("find_h", Some("Repo")),
                ("get_i", None),
                ("open_j", Some("Conn")),
                ("lower_k", None),
                ("loop_l", None),
                ("shadow_m", None),
                ("own_n", None),
            ],
        );
    }

    #[test]
    fn java_receiver_types_come_from_locals_params_and_fields() {
        let source = r#"
class Svc {
    private Store store;
    private final Map<String, Item> items = new HashMap<>();
    void run(Repo repo, List<String> names, int[] raw) {
        var s = new Store();
        Cache c = new Cache();
        a.b.Client cl = null;
        s.saveA();
        c.getB();
        cl.callC();
        repo.findD();
        names.sizeE();
        store.putF();
        this.store.putG();
        items.getH();
        for (Item it : all) { it.useI(); }
        xs.forEach(s2 -> s2.lambdaJ());
        var q = factory();
        q.goK();
        raw.cloneL();
        Unknown.staticM();
    }
}
"#;
        assert_receivers(
            source,
            "src/Svc.java",
            &[
                ("saveA", Some("Store")),
                ("getB", Some("Cache")),
                ("callC", Some("a.b.Client")),
                ("findD", Some("Repo")),
                ("sizeE", Some("List")),
                ("putF", Some("Store")),
                ("putG", Some("Store")),
                ("getH", Some("Map")),
                ("useI", Some("Item")),
                ("lambdaJ", None),
                ("goK", None),
                ("cloneL", None),
                ("staticM", None),
            ],
        );
    }

    #[test]
    fn java_local_shadows_a_field_with_an_unknown_type() {
        let source = "class A { Store store; void f() { var store = make(); store.saveA(); } }";
        assert_receivers(source, "A.java", &[("saveA", None)]);
    }

    #[test]
    fn go_receiver_types_come_from_params_short_vars_constructors_and_struct_fields() {
        let source = r#"package p

type Server struct {
	store *store.Store
	cache Cache
}

func (s *Server) Run(ctx context.Context, n *Node, xs []Item) error {
	x := NewStore()
	y, err := pkg.NewThing()
	z := &Conn{}
	var w Writer
	var v = Value{}
	q := new(Queue)
	x.SaveA()
	y.DoB()
	z.OpenC()
	w.WriteD()
	v.GetE()
	q.PushF()
	ctx.DoneG()
	n.VisitH()
	s.store.GetI()
	s.cache.PutJ()
	err.ErrorK()
	for _, x := range xs {
		x.LoopL()
	}
	s.OwnM()
	xs.LenN()
	fmt.PrintlnO()
	return nil
}
"#;
        assert_receivers(
            source,
            "server.go",
            &[
                ("SaveA", Some("Store")),
                ("DoB", Some("pkg.Thing")),
                ("OpenC", Some("Conn")),
                ("WriteD", Some("Writer")),
                ("GetE", Some("Value")),
                ("PushF", Some("Queue")),
                ("DoneG", Some("context.Context")),
                ("VisitH", Some("Node")),
                ("GetI", Some("store.Store")),
                ("PutJ", Some("Cache")),
                ("ErrorK", None),
                ("LoopL", None),
                ("OwnM", Some("Server")),
                ("LenN", None),
                ("PrintlnO", None),
            ],
        );
    }

    #[test]
    fn csharp_receiver_types_come_from_locals_params_fields_and_properties() {
        let source = r#"
class Svc {
    private Store store;
    public Repo Repo { get; set; }
    void Run(Cache cache, List<int> ids, int? maybe) {
        var s = new Store();
        Client c = new();
        s.SaveA();
        c.CallB();
        cache.GetC();
        ids.AddD();
        store.PutE();
        this.store.PutF();
        this.Repo.FindG();
        foreach (var it in items) { it.UseH(); }
        if (o is Widget w) { w.DrawI(); }
        var q = Make();
        q.GoJ();
        Console.WriteK();
    }
}
"#;
        assert_receivers(
            source,
            "Svc.cs",
            &[
                ("SaveA", Some("Store")),
                ("CallB", Some("Client")),
                ("GetC", Some("Cache")),
                ("AddD", Some("List")),
                ("PutE", Some("Store")),
                ("PutF", Some("Store")),
                ("FindG", Some("Repo")),
                ("UseH", None),
                ("DrawI", Some("Widget")),
                ("GoJ", None),
                ("WriteK", None),
            ],
        );
    }

    #[test]
    fn cpp_receiver_types_come_from_locals_params_and_members() {
        let source = r#"
class Svc {
  Store* store;
  void run(Repo& repo, const Cache* cache) {
    Store s(1);
    auto p = new Pool();
    ns::Client c;
    s.saveA();
    p->takeB();
    c.callC();
    repo.findD();
    cache->getE();
    store->putF();
    this->store->putG();
    auto q = make();
    q.goH();
  }
};
"#;
        assert_receivers(
            source,
            "svc.cpp",
            &[
                ("saveA", Some("Store")),
                ("takeB", Some("Pool")),
                ("callC", Some("ns::Client")),
                ("findD", Some("Repo")),
                ("getE", Some("Cache")),
                ("putF", Some("Store")),
                ("putG", Some("Store")),
                ("goH", None),
            ],
        );
    }

    #[test]
    fn type_text_is_stripped_to_the_named_type() {
        use super::{Lang, clean_type_text};
        let clean = |text: &str, lang: Lang| clean_type_text(text, lang);
        assert_eq!(
            clean("&mut Store<'a, T>", Lang::Rust).as_deref(),
            Some("Store")
        );
        assert_eq!(
            clean("&'static mut Store", Lang::Rust).as_deref(),
            Some("Store")
        );
        assert_eq!(
            clean("Arc<Mutex<Store>>", Lang::Rust).as_deref(),
            Some("Mutex")
        );
        assert_eq!(
            clean("Box<dyn Sink + Send>", Lang::Rust).as_deref(),
            Some("Sink")
        );
        assert_eq!(clean("Vec::<u8>", Lang::Rust).as_deref(), Some("Vec"));
        assert_eq!(clean("*pkg.Store", Lang::Go).as_deref(), Some("pkg.Store"));
        assert_eq!(clean("Set[T]", Lang::Go).as_deref(), Some("Set"));
        assert_eq!(clean("[]Item", Lang::Go), None);
        assert_eq!(clean("map[string]Item", Lang::Go), None);
        assert_eq!(clean("Store[]", Lang::Java), None);
        assert_eq!(clean("int?", Lang::CSharp).as_deref(), Some("int"));
        assert_eq!(
            clean("struct ns::Store", Lang::Cpp).as_deref(),
            Some("ns::Store")
        );
        assert_eq!(clean("(A, B)", Lang::Rust), None);
        assert_eq!(clean("impl Fn()", Lang::Rust), None);
        assert_eq!(
            super::python_type_text("Optional[Store]").as_deref(),
            Some("Store")
        );
        assert_eq!(
            super::python_type_text("'models.Store'").as_deref(),
            Some("models.Store")
        );
        assert_eq!(
            super::python_type_text("Store | None").as_deref(),
            Some("Store")
        );
        assert_eq!(
            super::python_type_text("list[Store]").as_deref(),
            Some("list")
        );
        assert_eq!(super::python_type_text("A | B"), None);
    }
}
