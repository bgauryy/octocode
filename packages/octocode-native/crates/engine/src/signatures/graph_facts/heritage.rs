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
//!
//! C# syntax does not distinguish a base class from an interface: only the
//! first entry of a class base list can be a class, so it is reported as
//! `extends` even when it names an interface. Later entries are always
//! interfaces.

use tree_sitter::Node;

use super::super::nodes::node_text;
use super::{GraphAccumulator, LineIndex};
use crate::graph::GraphFactEdge;

pub(super) fn collect_heritage(
    node: Node<'_>,
    content: &str,
    line_index: &LineIndex<'_>,
    acc: &mut GraphAccumulator,
    from: &str,
) {
    let mut bases: Vec<(&'static str, Node<'_>)> = Vec::new();
    match (acc.ext.as_str(), node.kind()) {
        ("rs", "impl_item") => {
            if let Some(trait_node) = node.child_by_field_name("trait") {
                bases.push(("implements", trait_node));
            }
        }
        ("rs", "trait_item") => {
            if let Some(bounds) = node.child_by_field_name("bounds") {
                push_children(&mut bases, bounds, "extends", |kind| {
                    matches!(
                        kind,
                        "type_identifier" | "scoped_type_identifier" | "generic_type"
                    )
                });
            }
        }
        ("py" | "pyi", "class_definition") => {
            if let Some(arguments) = node.child_by_field_name("superclasses") {
                let mut cursor = arguments.walk();
                for argument in arguments.named_children(&mut cursor) {
                    let base = match argument.kind() {
                        "identifier" | "attribute" => Some(argument),
                        // `Generic[T]`, `Base[int]`: the subscripted name.
                        "subscript" => argument
                            .child_by_field_name("value")
                            .filter(|value| matches!(value.kind(), "identifier" | "attribute")),
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
        ("java", "class_declaration" | "enum_declaration" | "record_declaration") => {
            if let Some(superclass) = node.child_by_field_name("superclass") {
                push_children(&mut bases, superclass, "extends", is_java_type);
            }
            if let Some(interfaces) = node.child_by_field_name("interfaces") {
                push_type_list(&mut bases, interfaces, "implements");
            }
        }
        ("java", "interface_declaration") => {
            let mut cursor = node.walk();
            let extends = node
                .named_children(&mut cursor)
                .find(|child| child.kind() == "extends_interfaces");
            if let Some(extends) = extends {
                push_type_list(&mut bases, extends, "extends");
            }
        }
        ("cs", kind) => {
            let mut cursor = node.walk();
            let base_list = node
                .named_children(&mut cursor)
                .find(|child| child.kind() == "base_list");
            if let Some(base_list) = base_list {
                let mut cursor = base_list.walk();
                let types = base_list.named_children(&mut cursor).filter_map(|child| {
                    match child.kind() {
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
        (_, "class_specifier" | "struct_specifier") => {
            let mut cursor = node.walk();
            let clause = node
                .named_children(&mut cursor)
                .find(|child| child.kind() == "base_class_clause");
            if let Some(clause) = clause {
                push_children(&mut bases, clause, "extends", |kind| {
                    matches!(
                        kind,
                        "type_identifier" | "qualified_identifier" | "template_type"
                    )
                });
            }
        }
        _ => {}
    }
    for (relation, base) in bases {
        let Some(to) = node_text(base, content).and_then(base_type_name) else {
            continue;
        };
        let line = line_index.range(base).start.line + 1;
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
            .filter(|child| accept(child.kind()))
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
        .find(|child| child.kind() == "type_list");
    if let Some(list) = list {
        push_children(bases, list, relation, is_java_type);
    }
}

/// The base type as written with type arguments and whitespace removed:
/// `Base<T>` → `Base`, `ns::Base<T>::Inner` → `ns::Base::Inner`. `None` for
/// anything that is not a plain (possibly qualified) name.
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
    let plain = depth == 0
        && !name.is_empty()
        && name.len() <= 160
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | ':' | '$'));
    plain.then_some(name)
}

#[cfg(test)]
mod tests {
    use super::base_type_name;

    #[test]
    fn base_type_names_drop_type_arguments_and_keep_paths() {
        assert_eq!(base_type_name("Base").as_deref(), Some("Base"));
        assert_eq!(base_type_name("List<String>").as_deref(), Some("List"));
        assert_eq!(
            base_type_name("ns::Base<T, U<V>>::Inner").as_deref(),
            Some("ns::Base::Inner")
        );
        assert_eq!(
            base_type_name("std::fmt::Display").as_deref(),
            Some("std::fmt::Display")
        );
        assert_eq!(base_type_name("pkg.Base").as_deref(), Some("pkg.Base"));
        assert_eq!(base_type_name("mixin(A)"), None);
        assert_eq!(base_type_name("Broken<T"), None);
        assert_eq!(base_type_name(""), None);
    }
}
