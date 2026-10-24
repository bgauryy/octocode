//! Python receiver bindings.
use super::*;

pub(super) fn python_bindings<'t>(
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

impl<'t> ReceiverTypes<'t> {
    /// Class-body annotations (`f: T`) are declared; `self.f = …` assignments
    /// in methods are inferred from a constructor call or a typed parameter,
    /// and any other assignment makes an inferred field unknown.
    pub(super) fn python_fields(&mut self, class: Node<'t>, content: &str) -> Fields {
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
                                    self.local(right, &path, node_text(right, content)?, content)?
                                        .ty
                                        .clone()
                                })
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

/// Python `T(..)` / `mod.T(..)` where `T` starts uppercase.
pub(super) fn python_constructor(value: Node<'_>, content: &str) -> Option<String> {
    (value.kind() == "call").then_some(())?;
    let function = value.child_by_field_name("function")?;
    if !matches!(function.kind(), "identifier" | "attribute") {
        return None;
    }
    let name: String = node_text(function, content)?.split_whitespace().collect();
    let valid = name.split('.').all(is_identifier) && starts_uppercase(last_segment(&name));
    valid.then_some(name)
}
