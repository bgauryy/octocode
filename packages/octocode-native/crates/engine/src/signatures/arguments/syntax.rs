use tree_sitter::Node;

use super::ArgumentAdapter;
use crate::graph::{GraphArgumentForm as Form, GraphArgumentValueKind};
use crate::signatures::nodes::node_text;

pub(crate) struct Reference<'a> {
    pub name: String,
    pub form: Form,
    pub selection: Node<'a>,
    pub root: Node<'a>,
}

pub(crate) enum Observation<'a> {
    Reference(Reference<'a>),
    Value(GraphArgumentValueKind),
    Unsupported,
}

/// Only transparent syntax wrappers are removed. A nested expression is never
/// searched for an identifier to stand in for the value of the whole slot.
pub(crate) fn observe<'a>(
    adapter: ArgumentAdapter,
    node: Node<'a>,
    content: &str,
) -> Observation<'a> {
    if let Some(reference) = reference(adapter, node, content, 0) {
        return Observation::Reference(reference);
    }
    let kind = node.kind();
    let value = if kind.ends_with("literal")
        || matches!(
            kind,
            "string"
                | "integer"
                | "float"
                | "true"
                | "false"
                | "none"
                | "null"
                | "nil"
                | "unit_expression"
        ) {
        GraphArgumentValueKind::Literal
    } else if adapter.is_recipient(kind) {
        GraphArgumentValueKind::CallResult
    } else if matches!(
        kind,
        "lambda"
            | "lambda_expression"
            | "closure_expression"
            | "arrow_function"
            | "function_expression"
            | "func_literal"
    ) {
        GraphArgumentValueKind::Closure
    } else if matches!(
        kind,
        "binary_expression"
            | "binary_operator"
            | "boolean_operator"
            | "comparison_operator"
            | "conditional_expression"
            | "ternary_expression"
            | "array"
            | "array_expression"
            | "array_creation_expression"
            | "list"
            | "dictionary"
            | "tuple"
            | "tuple_expression"
            | "object"
            | "struct_expression"
            | "composite_literal"
    ) {
        GraphArgumentValueKind::ValueExpression
    } else {
        return Observation::Unsupported;
    };
    Observation::Value(value)
}

fn reference<'a>(
    adapter: ArgumentAdapter,
    node: Node<'a>,
    content: &str,
    depth: usize,
) -> Option<Reference<'a>> {
    if depth > 64 || crate::deep_stack::job_cancelled() {
        return None;
    }
    if matches!(
        node.kind(),
        "identifier"
            | "type_identifier"
            | "field_identifier"
            | "namespace_identifier"
            | "self"
            | "this"
            | "super"
            | "crate"
    ) {
        return Some(Reference {
            name: node_text(node, content)?.to_owned(),
            form: Form::Identifier,
            selection: node,
            root: node,
        });
    }
    if node.kind() == "parenthesized_expression" && node.named_child_count() == 1 {
        return reference(adapter, node.named_child(0)?, content, depth + 1);
    }
    if matches!(node.kind(), "generic_function" | "template_function") {
        let inner = node
            .child_by_field_name("function")
            .or_else(|| node.child_by_field_name("name"))?;
        return reference(adapter, inner, content, depth + 1);
    }
    let address = match (adapter, node.kind()) {
        (ArgumentAdapter::Rust, "reference_expression") => Some(("value", Form::Borrow)),
        (adapter, "pointer_expression")
            if adapter.is_c_family()
                && node
                    .child_by_field_name("operator")
                    .and_then(|n| node_text(n, content))
                    == Some("&") =>
        {
            Some(("argument", Form::AddressOf))
        }
        (ArgumentAdapter::Go, "unary_expression")
            if node
                .child_by_field_name("operator")
                .and_then(|n| node_text(n, content))
                == Some("&") =>
        {
            Some(("operand", Form::AddressOf))
        }
        _ => None,
    };
    if let Some((field, form)) = address {
        let mut inner = reference(
            adapter,
            node.child_by_field_name(field)?,
            content,
            depth + 1,
        )?;
        inner.form = form;
        return Some(inner);
    }
    let fields = match (adapter, node.kind()) {
        (ArgumentAdapter::Python, "attribute") => ("object", "attribute", ".", Form::Member),
        (ArgumentAdapter::Go, "selector_expression") => ("operand", "field", ".", Form::Member),
        (adapter, "field_expression") if adapter.is_c_family() => {
            ("argument", "field", ".", Form::Member)
        }
        #[cfg(any(feature = "tree-sitter-cpp", feature = "tree-sitter-cuda"))]
        (ArgumentAdapter::CFamily, "qualified_identifier") => {
            ("scope", "name", "::", Form::Qualified)
        }
        (ArgumentAdapter::Rust, "scoped_identifier") => ("path", "name", "::", Form::Qualified),
        (ArgumentAdapter::Rust | ArgumentAdapter::Scala, "field_expression") => {
            ("value", "field", ".", Form::Member)
        }
        (ArgumentAdapter::Java, "field_access") => ("object", "field", ".", Form::Member),
        (ArgumentAdapter::JavaScript, "member_expression") => {
            ("object", "property", ".", Form::Member)
        }
        #[cfg(feature = "tree-sitter-c-sharp")]
        (ArgumentAdapter::CSharp, "member_access_expression") => {
            ("expression", "name", ".", Form::Member)
        }
        _ => return method_reference(adapter, node, content, depth),
    };
    let (left, right, separator, form) = fields;
    let prefix = reference(adapter, node.child_by_field_name(left)?, content, depth + 1)?;
    let suffix = reference(
        adapter,
        node.child_by_field_name(right)?,
        content,
        depth + 1,
    )?;
    Some(Reference {
        name: format!("{}{separator}{}", prefix.name, suffix.name),
        form,
        selection: suffix.selection,
        root: prefix.root,
    })
}

fn method_reference<'a>(
    adapter: ArgumentAdapter,
    node: Node<'a>,
    content: &str,
    depth: usize,
) -> Option<Reference<'a>> {
    if adapter != ArgumentAdapter::Java || node.kind() != "method_reference" {
        return None;
    }
    let mut cursor = node.walk();
    let children = node
        .named_children(&mut cursor)
        .filter(|n| n.kind() != "type_arguments")
        .collect::<Vec<_>>();
    let [receiver, method] = children.as_slice() else {
        return None;
    };
    let prefix = reference(adapter, *receiver, content, depth + 1)?;
    let suffix = reference(adapter, *method, content, depth + 1)?;
    Some(Reference {
        name: format!("{}::{}", prefix.name, suffix.name),
        form: Form::MethodReference,
        selection: suffix.selection,
        root: prefix.root,
    })
}

pub(crate) fn slot_value<'a>(
    adapter: ArgumentAdapter,
    slot: Node<'a>,
    content: &str,
) -> (Option<String>, Option<Node<'a>>) {
    match (adapter, slot.kind()) {
        (ArgumentAdapter::Python, "keyword_argument") => (
            slot.child_by_field_name("name")
                .and_then(|n| node_text(n, content))
                .map(str::to_owned),
            slot.child_by_field_name("value"),
        ),
        #[cfg(feature = "tree-sitter-c-sharp")]
        (ArgumentAdapter::CSharp, "argument") => {
            let label = slot.child_by_field_name("name");
            let mut cursor = slot.walk();
            if slot
                .children(&mut cursor)
                .any(|n| matches!(n.kind(), "ref" | "out" | "in"))
            {
                return (
                    label.and_then(|n| node_text(n, content)).map(str::to_owned),
                    None,
                );
            }
            let value = slot.named_children(&mut cursor).find(|n| Some(*n) != label);
            (
                label.and_then(|n| node_text(n, content)).map(str::to_owned),
                value,
            )
        }
        _ => (None, Some(slot)),
    }
}
