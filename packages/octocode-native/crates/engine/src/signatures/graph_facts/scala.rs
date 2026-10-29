//! Scala heritage: the `extends … with …` bases of a template, in written
//! order, so the linker can rebuild a linearization.
//!
//! Every base is an `extends` edge with its type arguments removed
//! (`FlatMap.AbstractFlatMap[F](a)` → `FlatMap.AbstractFlatMap`). A scope
//! whose members the syntax cannot attribute to named bases gets an edge to
//! a marker no declaration can match, so member lookup through it stops
//! instead of guessing:
//! - [`ANONYMOUS_TEMPLATE`]: an anonymous class (`new A { … }`) that is not
//!   the whole value of the declaration holding it;
//! - [`SELF_TYPE`]: a typed self alias (`self: Other =>`), whose members are
//!   visible in the template without being inherited;
//! - [`UNNAMED_BASE`]: a base written as something other than a type name.
//!
//! An anonymous class that is a declaration's whole value
//! (`implicit def F: Monad[F] = new Monad[F] { … }`) gives that declaration
//! its bases: its members are the declaration's children.

use tree_sitter::Node;

pub(super) const ANONYMOUS_TEMPLATE: &str = "<anonymous-template>";
pub(super) const SELF_TYPE: &str = "<self-type>";
pub(super) const UNNAMED_BASE: &str = "<unnamed-base>";

/// A base clause: a type-name node, or a marker for an opaque base.
pub(super) enum Base<'t> {
    Named(Node<'t>),
    Opaque(&'static str, Node<'t>),
}

pub(super) fn is_scala(ext: &str) -> bool {
    matches!(ext, "scala" | "sc" | "sbt")
}

/// The bases of a Scala declaration node, in written order.
pub(super) fn bases(node: Node<'_>) -> Vec<Base<'_>> {
    let mut out = Vec::new();
    match node.kind() {
        "class_definition" | "trait_definition" | "object_definition" | "enum_definition" => {
            if let Some(extend) = node.child_by_field_name("extend") {
                let mut cursor = extend.walk();
                for base in extend.children_by_field_name("type", &mut cursor) {
                    if base.is_named() {
                        base_types(base, &mut out);
                    }
                }
            }
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                let typed_self = body
                    .named_children(&mut cursor)
                    .find(|child| child.kind() == "self_type")
                    .filter(|alias| alias.named_child_count() > 1);
                if let Some(alias) = typed_self {
                    out.push(Base::Opaque(SELF_TYPE, alias));
                }
            }
        }
        "val_definition" | "var_definition" | "function_definition" => {
            let value = node
                .child_by_field_name("value")
                .or_else(|| node.child_by_field_name("body"));
            if let Some(instance) = value.filter(|value| is_anonymous_class(*value)) {
                let mut cursor = instance.walk();
                for child in instance.named_children(&mut cursor) {
                    if !matches!(
                        child.kind(),
                        "template_body" | "arguments" | "early_defs" | "end_marker" | "comment"
                    ) {
                        base_types(child, &mut out);
                    }
                }
            }
        }
        _ => {}
    }
    out
}

/// `new A with B { … }`: an instance expression with a template body.
pub(super) fn is_anonymous_class(node: Node<'_>) -> bool {
    if node.kind() != "instance_expression" {
        return false;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| child.kind() == "template_body")
}

/// Whether the anonymous class `node` is the whole value of `parent`, the
/// declaration that then carries its bases (see [`bases`]).
pub(super) fn is_declaration_value(
    node: Node<'_>,
    parent: Option<Node<'_>>,
    content: &str,
) -> bool {
    parent.is_some_and(|parent| {
        matches!(
            parent.kind(),
            "val_definition" | "var_definition" | "function_definition"
        ) && (parent.child_by_field_name("value") == Some(node)
            || parent.child_by_field_name("body") == Some(node))
            && crate::signatures::nodes::declaration(parent, content).is_some()
    })
}

/// The type names one base clause contributes: `A with B` is two bases,
/// `Base[F]` and `Base[F](x)` name `Base`.
fn base_types<'t>(node: Node<'t>, out: &mut Vec<Base<'t>>) {
    match node.kind() {
        "type_identifier" | "stable_type_identifier" => out.push(Base::Named(node)),
        "generic_type" => match node.child_by_field_name("type") {
            Some(inner) => base_types(inner, out),
            None => out.push(Base::Opaque(UNNAMED_BASE, node)),
        },
        "applied_constructor_type" => match node.named_child(0) {
            Some(inner) => base_types(inner, out),
            None => out.push(Base::Opaque(UNNAMED_BASE, node)),
        },
        "compound_type" => {
            let mut cursor = node.walk();
            let mut refined = false;
            for child in node.named_children(&mut cursor) {
                if child.kind() == "refinement" {
                    refined = true;
                } else {
                    base_types(child, out);
                }
            }
            if refined {
                out.push(Base::Opaque(UNNAMED_BASE, node));
            }
        }
        _ => out.push(Base::Opaque(UNNAMED_BASE, node)),
    }
}
