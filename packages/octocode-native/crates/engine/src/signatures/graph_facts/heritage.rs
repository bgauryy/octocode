//! Syntactic `extends` / `implements` facts for the tree-sitter lane.
//!
//! Each base type named in a class/interface/struct/trait/impl header becomes
//! a `GraphFactEdge` from the declaring declaration id to the base type name as
//! written in source, with type arguments removed (`Base`, `pkg.Base`,
//! `std::fmt::Display`). Syntax only (`resolution: "syntax"`): `to` is not a
//! resolved declaration.
//!
//! | Language | Shape | Relation |
//! |---|---|---|
//! | Rust | `impl Trait for Type` (from the impl declaration) | `implements` |
//! | Rust | `trait A: B + C` | `extends` |
//! | Python | `class A(B, C)` (skips keyword args, splats, `object`) | `extends` |
//! | Java | class `extends` / interface `extends` | `extends` |
//! | Java | class/enum/record `implements` | `implements` |
//! | C++ | `class A : public B` | `extends` |
//! | C# | interface bases | `extends` |
//! | C# | struct / record struct bases | `implements` |
//! | C# | class / record: first base `extends`, later bases `implements` |
//! | Scala | class/trait/object `extends A with B`, anonymous classes (see `scala.rs`) | `extends` |
//! | Go | embedded struct fields and embedded interfaces (see `go.rs`) | `embeds` |
//!
//! C# syntax does not distinguish a base class from an interface: only the
//! first entry of a class base list can be a class, so it is reported as
//! `extends` even when it names an interface. Later entries are always
//! interfaces.

use crate::signatures::node_kinds::kind_of;
use tree_sitter::Node;

use super::super::nodes::node_text;
use super::{GraphAccumulator, NodePositions, scala};
use crate::graph::GraphFactEdge;

/// The heritage clauses a grammar's declarations carry, by its canonical
/// extension: the one dispatch of [`collect_heritage`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Clauses {
    Rust,
    Python,
    Java,
    Go,
    CSharp,
    Scala,
    /// A C++-style `base_class_clause` of a class or struct specifier.
    BaseClass,
}

impl Clauses {
    pub(super) fn of(ext: &str) -> Self {
        match ext {
            "rs" => Self::Rust,
            "py" => Self::Python,
            "java" => Self::Java,
            "go" => Self::Go,
            "cs" => Self::CSharp,
            ext if scala::is_scala(ext) => Self::Scala,
            _ => Self::BaseClass,
        }
    }

    /// Whether a clause names a supertype (`extends` / `implements`): every
    /// form but Go's, whose embedded types are `embeds`.
    pub(super) const fn names_supertypes(self) -> bool {
        !matches!(self, Self::Go)
    }
}

pub(super) fn collect_heritage(
    node: Node<'_>,
    content: &str,
    line_index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    from: &str,
) {
    let mut bases: Vec<(&'static str, Node<'_>)> = Vec::new();
    match (Clauses::of(&acc.ext), kind_of(node)) {
        (Clauses::Rust, "impl_item") => {
            if let Some(trait_node) = node.child_by_field_name("trait") {
                bases.push(("implements", trait_node));
            }
        }
        (Clauses::Rust, "trait_item") => {
            if let Some(bounds) = node.child_by_field_name("bounds") {
                push_children(&mut bases, bounds, "extends", |kind| {
                    matches!(
                        kind,
                        "type_identifier" | "scoped_type_identifier" | "generic_type"
                    )
                });
            }
        }
        (Clauses::Python, "class_definition") => {
            if let Some(arguments) = node.child_by_field_name("superclasses") {
                let mut cursor = arguments.walk();
                for argument in arguments.named_children(&mut cursor) {
                    let base = match kind_of(argument) {
                        "identifier" | "attribute" => Some(argument),
                        // `Generic[T]`, `Base[int]`: the subscripted name.
                        "subscript" => argument
                            .child_by_field_name("value")
                            .filter(|value| matches!(kind_of(value), "identifier" | "attribute")),
                        _ => None,
                    };
                    if let Some(base) = base
                        && node_text(base, content) != Some("object")
                    {
                        bases.push(("extends", base));
                    }
                }
            }
        }
        (Clauses::Java, "class_declaration" | "enum_declaration" | "record_declaration") => {
            if let Some(superclass) = node.child_by_field_name("superclass") {
                push_children(&mut bases, superclass, "extends", is_java_type);
            }
            if let Some(interfaces) = node.child_by_field_name("interfaces") {
                push_type_list(&mut bases, interfaces, "implements");
            }
        }
        (Clauses::Go, "type_spec") => {
            bases.extend(
                super::go::embedded_types(node)
                    .into_iter()
                    .map(|base| ("embeds", base)),
            );
        }
        (Clauses::Java, "interface_declaration") => {
            let mut cursor = node.walk();
            let extends = node
                .named_children(&mut cursor)
                .find(|child| kind_of(child) == "extends_interfaces");
            if let Some(extends) = extends {
                push_type_list(&mut bases, extends, "extends");
            }
        }
        (Clauses::CSharp, kind) => {
            let mut cursor = node.walk();
            let base_list = node
                .named_children(&mut cursor)
                .find(|child| kind_of(child) == "base_list");
            if let Some(base_list) = base_list {
                let mut cursor = base_list.walk();
                let types = base_list.named_children(&mut cursor).filter_map(|child| {
                    match kind_of(child) {
                        // `record R(int X) : Base(X)` names its base with arguments.
                        "primary_constructor_base_type" => child.child_by_field_name("type"),
                        "argument_list" | "comment" => None,
                        _ => Some(child),
                    }
                });
                for (index, base) in types.enumerate() {
                    let relation = match kind {
                        "interface_declaration" => "extends",
                        "struct_declaration" | "record_struct_declaration" => "implements",
                        _ if index == 0 => "extends",
                        _ => "implements",
                    };
                    bases.push((relation, base));
                }
            }
        }
        (Clauses::BaseClass, "class_specifier" | "struct_specifier") => {
            let mut cursor = node.walk();
            let clause = node
                .named_children(&mut cursor)
                .find(|child| kind_of(child) == "base_class_clause");
            if let Some(clause) = clause {
                push_children(&mut bases, clause, "extends", |kind| {
                    matches!(
                        kind,
                        "type_identifier" | "qualified_identifier" | "template_type"
                    )
                });
            }
        }
        (Clauses::Scala, _) => {
            for base in scala::bases(node) {
                let (to, at) = match base {
                    scala::Base::Named(base) => {
                        let Some(to) = node_text(base, content).and_then(base_type_name) else {
                            continue;
                        };
                        (to, base)
                    }
                    scala::Base::Opaque(marker, at) => (marker.to_owned(), at),
                };
                push_edge(
                    acc,
                    from,
                    to,
                    "extends",
                    line_index.range(at).start.line + 1,
                );
            }
        }
        _ => {}
    }
    for (relation, base) in bases {
        let Some(to) = node_text(base, content).and_then(base_type_name) else {
            continue;
        };
        let line = line_index.range(base).start.line + 1;
        push_edge(acc, from, to, relation, line);
    }
}

/// A Scala anonymous class that is not a declaration's whole value: its
/// members belong to the enclosing declaration `from`, whose scope then
/// inherits from bases the syntax cannot attribute.
pub(super) fn collect_scala_anonymous(
    node: Node<'_>,
    parent: Option<Node<'_>>,
    content: &str,
    line_index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    from: Option<&str>,
) {
    if let Some(from) = from
        && scala::is_scala(&acc.ext)
        && scala::is_anonymous_class(node)
        && !scala::is_declaration_value(node, parent, content)
    {
        let line = line_index.range(node).start.line + 1;
        push_edge(
            acc,
            from,
            scala::ANONYMOUS_TEMPLATE.to_owned(),
            "extends",
            line,
        );
    }
}

fn push_edge(acc: &mut GraphAccumulator, from: &str, to: String, relation: &str, line: u32) {
    acc.edges.push(GraphFactEdge {
        id: format!("{from}->{to}:{relation}:{line}:{}", acc.edges.len()),
        from: from.to_owned(),
        to,
        relation: relation.to_owned(),
        source: "tree-sitter".to_owned(),
        line,
        resolution: "syntax".to_owned(),
    });
}

fn is_java_type(kind: &str) -> bool {
    matches!(
        kind,
        "type_identifier" | "scoped_type_identifier" | "generic_type"
    )
}

fn push_children<'t>(
    bases: &mut Vec<(&'static str, Node<'t>)>,
    parent: Node<'t>,
    relation: &'static str,
    accept: impl Fn(&str) -> bool,
) {
    let mut cursor = parent.walk();
    bases.extend(
        parent
            .named_children(&mut cursor)
            .filter(|child| accept(kind_of(child)))
            .map(|child| (relation, child)),
    );
}

/// Java `implements A, B` / interface `extends A, B`: a `type_list` holder.
fn push_type_list<'t>(
    bases: &mut Vec<(&'static str, Node<'t>)>,
    holder: Node<'t>,
    relation: &'static str,
) {
    let mut cursor = holder.walk();
    let list = holder
        .named_children(&mut cursor)
        .find(|child| kind_of(child) == "type_list");
    if let Some(list) = list {
        push_children(bases, list, relation, is_java_type);
    }
}

/// The base type as written with type arguments and whitespace removed:
/// `Base<T>` → `Base`, `ns::Base<T>::Inner` → `ns::Base::Inner`. `None` for
/// anything that is not a plain (possibly qualified) name ([`is_plain_name`]).
pub(super) fn base_type_name(text: &str) -> Option<String> {
    let mut name = String::with_capacity(text.len());
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.checked_sub(1)?,
            _ if depth > 0 || c.is_whitespace() => {}
            _ => name.push(c),
        }
    }
    let plain = depth == 0 && !name.is_empty() && name.len() <= 160 && is_plain_name(&name);
    plain.then_some(name)
}

/// Whether `name` holds only identifier characters and `.`/`:` separators,
/// each segment possibly a Rust raw identifier (`r#Trait`), kept as written.
fn is_plain_name(name: &str) -> bool {
    name.split(['.', ':']).all(|segment| {
        let bare = match segment.strip_prefix("r#") {
            Some("") => return false,
            Some(bare) => bare,
            None => segment,
        };
        bare.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '$'))
    })
}

#[cfg(test)]
mod tests;
