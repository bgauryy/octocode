//! Scala type names as the term scope reads them: a declared type without
//! its arguments, and the type parameters in scope.

use std::collections::HashSet;

use tree_sitter::Node;

use crate::signatures::nodes::node_text;

/// A type as its (possibly dotted) name without arguments: `Monad[F]` →
/// `Monad`, `=> cats.Eval[A]` → `cats.Eval`. `None` for function, tuple,
/// compound, structural and other unnamed types.
pub(super) fn type_text(ty: Option<Node<'_>>, content: &str) -> Option<String> {
    let ty = ty?;
    match ty.kind() {
        "type_identifier" | "stable_type_identifier" => node_text(ty, content)
            .map(|text| text.split_whitespace().collect::<String>())
            .filter(|text| !text.is_empty()),
        "generic_type" => type_text(ty.child_by_field_name("type"), content),
        "lazy_parameter_type" => type_text(ty.named_child(0), content),
        _ => None,
    }
}

/// Names of the type parameters in scope at the end of `path`.
pub(super) fn type_parameters<'c>(path: &[Node<'_>], content: &'c str) -> HashSet<&'c str> {
    let mut names = HashSet::new();
    let owners = path.iter().filter(|node| {
        matches!(
            node.kind(),
            "function_definition"
                | "function_declaration"
                | "class_definition"
                | "trait_definition"
                | "enum_definition"
                | "given_definition"
                | "lambda_expression"
        )
    });
    for node in owners {
        let mut cursor = node.walk();
        for clause in node
            .named_children(&mut cursor)
            .filter(|child| child.kind() == "type_parameters")
        {
            let mut inner = clause.walk();
            for name in clause.children_by_field_name("name", &mut inner) {
                names.extend(node_text(name, content));
            }
        }
    }
    names
}
