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

use crate::signatures::node_kinds::kind_of;
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
        Some(match ext {
            "rs" => Self::Rust,
            "py" => Self::Python,
            "java" => Self::Java,
            "go" => Self::Go,
            "cs" => Self::CSharp,
            // CUDA is C++ with attributes; its `.cu` keeps its own extension.
            "cpp" | "cu" | "cuh" => Self::Cpp,
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
        (kind_of(node) == kind).then_some(())?;
        Some((
            node.child_by_field_name(object)?,
            node.child_by_field_name(member)?,
        ))
    }

    fn is_self(self, node: Node<'_>, content: &str) -> bool {
        match self {
            Self::Rust => kind_of(node) == "self",
            Self::Python => {
                kind_of(node) == "identifier" && node_text(node, content) == Some("self")
            }
            Self::Java | Self::CSharp | Self::Cpp => {
                matches!(kind_of(node), "this" | "this_expression")
            }
            Self::Go => false,
        }
    }

    /// Bare names resolve to fields of the enclosing class.
    fn has_implicit_this(self) -> bool {
        matches!(self, Self::Java | Self::CSharp | Self::Cpp)
    }
}

#[derive(Clone)]
pub(super) struct Binding {
    name: String,
    ty: Option<String>,
    /// First byte where the binding is visible.
    from: usize,
    /// Byte where its scope ends.
    until: usize,
    /// A Rust `let` without a type whose value is a call (`let x = c.f()?;`).
    value_call: Option<crate::graph::GraphReceiverCall>,
}

/// Per-tree receiver-type resolver; caches function bindings, type fields
/// and the file's named type declarations.
pub(super) struct ReceiverTypes<'t> {
    lang: Lang,
    root: Node<'t>,
    bindings: HashMap<usize, Vec<Binding>>,
    fields: HashMap<usize, Fields>,
    types: Option<HashMap<String, Vec<Node<'t>>>>,
    /// For a re-parsed macro body: the locals its invocation sees in the
    /// enclosing function, which the body's own tree does not hold.
    outer: Vec<Binding>,
}

impl<'t> ReceiverTypes<'t> {
    pub(super) fn new(ext: &str, root: Node<'t>) -> Option<Self> {
        Some(Self {
            lang: Lang::for_extension(ext)?,
            root,
            bindings: HashMap::new(),
            fields: HashMap::new(),
            types: None,
            outer: Vec::new(),
        })
    }

    /// This resolver for a re-parsed macro body, seeing `outer`: the locals
    /// visible where the macro is invoked ([`Self::visible_locals`]).
    pub(super) fn with_outer(mut self, outer: &[Binding]) -> Self {
        self.outer = outer.to_vec();
        self
    }

    /// The locals visible at `at` in its innermost enclosing function, the
    /// latest of each name; in a macro body with no function of its own, the
    /// ones its invocation sees. `path` holds the enclosing nodes of `at`.
    pub(super) fn visible_locals(
        &mut self,
        at: Node<'t>,
        path: &[Node<'t>],
        content: &str,
    ) -> Vec<Binding> {
        let position = at.start_byte();
        let Some(&function) = path
            .iter()
            .rev()
            .find(|n| self.lang.is_function(kind_of(**n)))
        else {
            return self.outer.clone();
        };
        let mut latest: HashMap<&str, &Binding> = HashMap::new();
        for binding in self.bindings_of(function, content) {
            if binding.from <= position
                && position < binding.until
                && latest
                    .get(binding.name.as_str())
                    .is_none_or(|b| b.from <= binding.from)
            {
                latest.insert(binding.name.as_str(), binding);
            }
        }
        latest
            .into_values()
            .map(|binding| Binding {
                from: 0,
                until: usize::MAX,
                ..binding.clone()
            })
            .collect()
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
            (kind_of(call) == "method_invocation").then_some(call)?
        } else {
            let function = call.child_by_field_name("function")?;
            if kind_of(function) == "generic_function" {
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
        // `(*p).m()` reads `p`, as `p->m()` does.
        let receiver = if lang == Lang::Cpp {
            crate::signatures::calls::cfamily::dereferenced(receiver, content)
        } else {
            receiver
        };
        // `T(..).m()`, `T::new(..).m()`, `NewT(..).m()`, `new T(..).m()`:
        // the constructed type.
        let constructed = match lang {
            Lang::Python => python::python_constructor(receiver, content),
            Lang::Go => go::go_constructor(receiver, false, content),
            Lang::Rust => self.rust_constructor(receiver, content),
            Lang::Java => java::java_constructor(receiver, content),
            Lang::CSharp => csharp::csharp_constructor(receiver, content),
            Lang::Cpp => None,
        };
        if constructed.is_some() {
            return constructed;
        }
        if kind_of(receiver) == "identifier" {
            let name = node_text(receiver, content)?;
            if matches!(name, "self" | "this") {
                return None;
            }
            return match self.local(receiver, path, name, content) {
                Some(binding) => binding.ty.clone(),
                None if lang.has_implicit_this() => self.enclosing_field(path, name, content, true),
                None if lang == Lang::Rust => self.rust_item_type(name, content),
                None => None,
            };
        }
        let (object, member) = lang.member_parts(receiver)?;
        let member = node_text(member, content)?;
        if lang.is_self(object, content) {
            return self.enclosing_field(path, member, content, false);
        }
        if lang == Lang::Go && kind_of(object) == "identifier" {
            let owner = self
                .local(object, path, node_text(object, content)?, content)?
                .ty
                .clone()?;
            let declaration = self.unique_type(&owner, content)?;
            return self.fields_of(declaration, content).get(member).cloned()?;
        }
        None
    }

    /// The binding of the local `name` visible at `at` (its type may be
    /// unknown), `None` when it is not bound. `path` holds the enclosing
    /// nodes of `at`, root first.
    fn local(
        &mut self,
        at: Node<'t>,
        path: &[Node<'t>],
        name: &str,
        content: &str,
    ) -> Option<&Binding> {
        let position = at.start_byte();
        for &node in path.iter().rev() {
            if self.lang.is_function(kind_of(node)) {
                let found = self
                    .bindings_of(node, content)
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| b.name == name && b.from <= position && position < b.until)
                    .max_by_key(|(_, b)| b.from)
                    .map(|(index, _)| index);
                if let Some(index) = found {
                    return self.bindings.get(&node.id())?.get(index);
                }
                // Nested Rust fn items do not capture their parent's locals.
                if self.lang == Lang::Rust {
                    return None;
                }
            }
        }
        // A macro body's own tree holds no enclosing function: the local is
        // the one its invocation sees.
        if path
            .iter()
            .any(|node| self.lang.is_function(kind_of(*node)))
        {
            return None;
        }
        self.outer.iter().find(|binding| binding.name == name)
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
}

fn named_children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

/// Index of the last node in `path` whose kind is in `kinds`.
fn nearest(path: &[Node<'_>], kinds: &[&str]) -> Option<usize> {
    path.iter().rposition(|node| kinds.contains(&kind_of(node)))
}

/// The nearest ancestor of `node` whose kind is in `kinds`, found in one
/// root-down descent (the last match on the way down is the nearest).
fn ancestor<'t>(root: Node<'t>, node: Node<'t>, kinds: &[&str]) -> Option<Node<'t>> {
    let mut nearest = None;
    let mut current = root;
    while current.id() != node.id() {
        if kinds.contains(&kind_of(current)) {
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
            .find(|node| kinds.contains(&kind_of(*node)))
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

mod bindings;
mod cpp;
mod csharp;
mod fields;
mod go;
mod java;
mod python;
mod rust;
#[cfg(feature = "tree-sitter-scala")]
mod scala;
mod type_text;

use crate::signatures::receiver_fields::{Fields, merge};
use bindings::{bind_pattern, push};
use cpp::cpp_declarator_name;
pub(super) use go::go_return_type;
pub(super) use rust::{DeclaredReturns, rust_return_types};
#[cfg(feature = "tree-sitter-scala")]
pub(super) use scala::ScalaTerms;
use type_text::is_identifier;
pub(super) use type_text::{clean_type_text, python_type_text, rust_try_argument};

#[cfg(test)]
mod tests;
