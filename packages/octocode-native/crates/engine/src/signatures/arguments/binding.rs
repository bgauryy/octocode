//! Conservative lexical barriers for syntax adapters. A missing type is not
//! evidence that a local name can be resolved through a global declaration.

use std::{collections::HashMap, time::Instant};
use tree_sitter::Node;

use super::ArgumentAdapter;
use crate::graph::{ByteRange, GraphArgumentBinding, ObservedSpan};
use crate::signatures::nodes::node_text;

struct Binding {
    name_range: ObservedSpan,
    scope: std::ops::Range<usize>,
    visible_from: usize,
}

#[derive(Default)]
pub(crate) struct Bindings {
    names: HashMap<String, Vec<Binding>>,
    unknown: Vec<std::ops::Range<usize>>,
    complete: bool,
    implicit_members: bool,
    lookup_admitted: bool,
}

impl Bindings {
    pub(crate) fn collect(
        root: Node<'_>,
        adapter: ArgumentAdapter,
        source: &str,
        deadline: Instant,
    ) -> Self {
        let mut result = Self {
            implicit_members: adapter.implicit_members(),
            lookup_admitted: true,
            ..Self::default()
        };
        let mut pending = vec![root];
        let mut cursor = root.walk();
        while let Some(node) = pending.pop() {
            if Instant::now() >= deadline || crate::deep_stack::job_cancelled() {
                return result;
            }
            if let Some(scope) = local_definition_barrier(node, source) {
                result.unknown.push(scope.byte_range());
            }
            if unknown_scope(adapter, node.kind()) {
                let scope = if matches!(
                    node.kind(),
                    "lambda"
                        | "lambda_expression"
                        | "closure_expression"
                        | "anonymous_method_expression"
                        | "list_comprehension"
                        | "dictionary_comprehension"
                        | "set_comprehension"
                        | "generator_expression"
                        | "let_chain"
                ) {
                    node
                } else if adapter == ArgumentAdapter::Rust && node.kind() == "macro_invocation" {
                    lexical_scope(node, false)
                } else {
                    lexical_scope(node, true)
                };
                result.unknown.push(scope.byte_range());
            }
            if let Some((fields, parameter)) = binding_fields(adapter, node.kind()) {
                let scope = binding_scope(node, adapter, parameter);
                for field in fields {
                    let mut children = node.walk();
                    for pattern in node.children_by_field_name(field, &mut children) {
                        let mut patterns = vec![pattern];
                        while let Some(part) = patterns.pop() {
                            if Instant::now() >= deadline || crate::deep_stack::job_cancelled() {
                                return result;
                            }
                            if matches!(part.kind(), "identifier" | "field_identifier") {
                                if let Some(name) = node_text(part, source) {
                                    result.names.entry(name.to_owned()).or_default().push(
                                        Binding {
                                            name_range: ByteRange {
                                                start_byte: part.start_byte() as u32,
                                                end_byte: part.end_byte() as u32,
                                            }
                                            .into(),
                                            scope: scope.byte_range(),
                                            visible_from: visible_from(
                                                node, part, scope, adapter, parameter,
                                            ),
                                        },
                                    );
                                }
                            } else if !matches!(
                                part.kind(),
                                "type_annotation"
                                    | "type_arguments"
                                    | "type_identifier"
                                    | "primitive_type"
                            ) {
                                if let Some(left) = part
                                    .child_by_field_name("left")
                                    .or_else(|| part.child_by_field_name("pattern"))
                                    .or_else(|| part.child_by_field_name("declarator"))
                                    .or_else(|| part.child_by_field_name("name"))
                                {
                                    patterns.push(left);
                                } else {
                                    let mut pattern_cursor = part.walk();
                                    patterns.extend(part.named_children(&mut pattern_cursor));
                                }
                            }
                        }
                    }
                }
            }
            pending.extend(node.named_children(&mut cursor));
        }
        result.complete = true;
        result
    }

    pub(crate) fn in_region(mut self, unit: crate::text::analysis::UnitHandle) -> Self {
        for binding in self.names.values_mut().flatten() {
            binding.name_range.unit = unit;
        }
        // A source macro body is not macro expansion; no outer-name or hygiene lookup is established.
        self.lookup_admitted = false;
        self
    }

    pub(crate) fn assess(&self, root_name: &str, at: usize) -> GraphArgumentBinding {
        if !self.complete
            || matches!(root_name, "self" | "this" | "super")
            || self.unknown.iter().any(|scope| scope.contains(&at))
        {
            return GraphArgumentBinding::NotAssessed;
        }
        self.names
            .get(root_name)
            .into_iter()
            .flatten()
            .filter(|binding| binding.scope.contains(&at) && binding.visible_from <= at)
            .min_by_key(|binding| (binding.scope.len(), std::cmp::Reverse(binding.visible_from)))
            .map_or(
                if self.lookup_admitted {
                    GraphArgumentBinding::ScopedLookup {
                        implicit_members: self.implicit_members,
                    }
                } else {
                    GraphArgumentBinding::NotAssessed
                },
                |binding| GraphArgumentBinding::Unrepresented {
                    source_range: binding.name_range,
                },
            )
    }
}

fn binding_scope(node: Node<'_>, adapter: ArgumentAdapter, parameter: bool) -> Node<'_> {
    if parameter {
        let mut parent = node.parent();
        while let Some(current) = parent {
            if matches!(current.kind(), "catch_clause" | "catch_handler") {
                return current.child_by_field_name("body").unwrap_or(current);
            }
            if is_callable_scope(current.kind()) {
                break;
            }
            parent = current.parent();
        }
    }
    if adapter == ArgumentAdapter::Rust && node.kind() == "let_condition" {
        let mut parent = node.parent();
        while let Some(current) = parent {
            if let Some(body) = current
                .child_by_field_name("consequence")
                .or_else(|| current.child_by_field_name("body"))
            {
                return body;
            }
            parent = current.parent();
        }
    }
    if matches!(node.kind(), "for_expression" | "enhanced_for_statement") {
        return node.child_by_field_name("body").unwrap_or(node);
    }
    if (parameter && matches!(node.kind(), "function_definition" | "lambda"))
        || node.kind() == "match_arm"
    {
        return node;
    }
    lexical_scope(node, parameter || adapter == ArgumentAdapter::Python)
}

fn visible_from(
    node: Node<'_>,
    name: Node<'_>,
    scope: Node<'_>,
    adapter: ArgumentAdapter,
    parameter: bool,
) -> usize {
    if parameter
        || adapter == ArgumentAdapter::Python
        || matches!(
            node.kind(),
            "for_expression"
                | "let_condition"
                | "match_arm"
                | "enhanced_for_statement"
                | "field_declaration"
        )
    {
        scope.start_byte()
    } else if adapter.binding_visible_after_name() {
        name.end_byte()
    } else {
        node.end_byte()
    }
}

fn local_definition_barrier<'a>(node: Node<'a>, source: &str) -> Option<Node<'a>> {
    let declaration = crate::signatures::nodes::declaration(node, source);
    let boundary = declaration.is_some_and(|(kind, _)| {
        matches!(
            kind,
            "function" | "method" | "class" | "struct" | "type" | "interface" | "enum"
        )
    }) || matches!(
        node.kind(),
        "import_statement"
            | "import_from_statement"
            | "use_declaration"
            | "using_directive"
            | "import_declaration"
    );
    if !boundary {
        return None;
    }
    let scope = lexical_scope(node, true);
    is_callable_scope(scope.kind()).then_some(scope)
}

fn is_callable_scope(kind: &str) -> bool {
    matches!(
        kind,
        "function_definition"
            | "function_item"
            | "function_declaration"
            | "method_declaration"
            | "constructor_declaration"
            | "local_function_statement"
            | "lambda_expression"
            | "closure_expression"
            | "func_literal"
    )
}

fn lexical_scope(mut node: Node<'_>, function: bool) -> Node<'_> {
    while let Some(parent) = node.parent() {
        node = parent;
        if is_callable_scope(node.kind())
            || (!function
                && matches!(
                    node.kind(),
                    "block"
                        | "compound_statement"
                        | "statement_block"
                        | "constructor_body"
                        | "class_body"
                        | "field_declaration_list"
                        | "declaration_list"
                        | "for_statement"
                        | "for_expression"
                        | "enhanced_for_statement"
                        | "for_each_statement"
                        | "catch_clause"
                        | "match_arm"
                ))
        {
            break;
        }
    }
    node
}

fn binding_fields(adapter: ArgumentAdapter, kind: &str) -> Option<(&'static [&'static str], bool)> {
    use ArgumentAdapter as A;
    Some(match (adapter, kind) {
        (A::Rust, "parameter" | "self_parameter") => (&["pattern"], true),
        (A::Rust, "let_declaration" | "for_expression" | "let_condition" | "match_arm") => {
            (&["pattern"], false)
        }
        (A::Go, "parameter_declaration" | "variadic_parameter_declaration") => (&["name"], true),
        (A::Go, "var_spec" | "const_spec") => (&["name"], false),
        (A::Go, "short_var_declaration" | "range_clause" | "receive_statement") => {
            (&["left"], false)
        }
        (A::Python, "assignment" | "augmented_assignment" | "for_statement" | "for_in_clause") => {
            (&["left"], false)
        }
        (A::Python, "named_expression") => (&["name"], false),
        (A::Python, "function_definition" | "lambda") => (&["parameters"], true),
        (adapter, "parameter_declaration" | "optional_parameter_declaration")
            if adapter.is_c_family() =>
        {
            (&["declarator"], true)
        }
        (adapter, "declaration" | "init_declarator" | "field_declaration")
            if adapter.is_c_family() =>
        {
            (&["declarator"], false)
        }
        (A::Java, "formal_parameter" | "spread_parameter" | "catch_formal_parameter") => {
            (&["name"], true)
        }
        (A::Java, "variable_declarator" | "enhanced_for_statement") => (&["name"], false),
        #[cfg(feature = "tree-sitter-c-sharp")]
        (A::CSharp, "parameter") => (&["name"], true),
        #[cfg(feature = "tree-sitter-c-sharp")]
        (A::CSharp, "variable_declarator" | "catch_declaration") => (&["name"], false),
        (A::Scala, "parameter" | "class_parameter") => (&["name"], true),
        (A::Scala, "val_definition" | "var_definition") => (&["pattern"], false),
        (A::JavaScript, "variable_declarator") => (&["name"], false),
        _ => return None,
    })
}

fn unknown_scope(adapter: ArgumentAdapter, kind: &str) -> bool {
    use ArgumentAdapter as A;
    match adapter {
        // OXC supplies precise binding admission; its recovery fallback cannot.
        A::JavaScript => true,
        A::Python => matches!(
            kind,
            "global_statement"
                | "delete_statement"
                | "nonlocal_statement"
                | "list_comprehension"
                | "dictionary_comprehension"
                | "set_comprehension"
                | "generator_expression"
                | "with_statement"
                | "except_clause"
        ),
        A::Rust => matches!(
            kind,
            "closure_expression" | "macro_invocation" | "match_expression" | "let_chain"
        ),
        #[cfg(any(feature = "tree-sitter-cpp", feature = "tree-sitter-cuda"))]
        A::CFamily => unknown_scope(A::C, kind),
        A::C => matches!(
            kind,
            "lambda_expression" | "preproc_def" | "preproc_function_def" | "for_range_loop"
        ),
        A::Java => matches!(kind, "lambda_expression" | "instanceof_expression"),
        #[cfg(feature = "tree-sitter-c-sharp")]
        A::CSharp => matches!(
            kind,
            "lambda_expression"
                | "anonymous_method_expression"
                | "declaration_expression"
                | "declaration_pattern"
                | "recursive_pattern"
                | "for_each_statement"
        ),
        A::Scala => matches!(kind, "lambda_expression" | "case_clause" | "for_expression"),
        A::Go => kind == "type_switch_statement",
    }
}
