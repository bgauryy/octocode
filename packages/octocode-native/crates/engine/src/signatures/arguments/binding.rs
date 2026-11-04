//! Conservative lexical barriers for syntax adapters. A missing type is not
//! evidence that a local name can be resolved through a global declaration.

use crate::num::saturating_u32;
use std::{collections::HashMap, time::Instant};
use tree_sitter::Node;

use super::ArgumentAdapter;
use crate::graph::{ByteRange, GraphArgumentBinding, ObservedSpan};
use crate::signatures::nodes::node_text;

mod kinds;

use kinds::{binding_fields, is_block_scope, is_callable_scope, unknown_scope};

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
        // Each node carries what its ancestors settle: `Node::parent()` searches
        // from the root on every hop, so a climb per node costs the depth squared
        // on the deep trees recovered parses make.
        let mut pending = vec![(root, Scopes::of_root(root))];
        let mut cursor = root.walk();
        let mut children = Vec::new();
        while let Some((node, scopes)) = pending.pop() {
            if Instant::now() >= deadline || crate::deep_stack::job_cancelled() {
                return result;
            }
            if let Some(scope) = local_definition_barrier(node, source, scopes) {
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
                    scopes.block
                } else {
                    scopes.function
                };
                result.unknown.push(scope.byte_range());
            }
            if let Some((fields, parameter)) = binding_fields(adapter, node.kind()) {
                let scope = binding_scope(node, adapter, parameter, scopes);
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
                                                start_byte: saturating_u32(part.start_byte()),
                                                end_byte: saturating_u32(part.end_byte()),
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
            children.clear();
            children.extend(node.named_children(&mut cursor));
            if adapter.is_c_family() {
                crate::signatures::graph_facts::c_family::retain_compiled(
                    node,
                    source,
                    &mut children,
                );
            }
            let below = scopes.below(node, adapter);
            pending.extend(children.iter().map(|child| (*child, below)));
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

/// What a node's ancestors settle for the bindings it may hold, carried down
/// the walk so each node reads it in constant time.
#[derive(Clone, Copy)]
struct Scopes<'t> {
    /// The node's parent.
    parent: Option<Node<'t>>,
    /// The nearest callable ancestor, else the root.
    function: Node<'t>,
    /// The nearest callable or block ancestor, else the root.
    block: Node<'t>,
    /// What the nearest catch ancestor binds its parameter in, unless a
    /// callable lies between them.
    catch_body: Option<Node<'t>>,
    /// Rust: the body of the nearest ancestor that has one.
    let_body: Option<Node<'t>>,
}

impl<'t> Scopes<'t> {
    fn of_root(root: Node<'t>) -> Self {
        Self {
            parent: None,
            function: root,
            block: root,
            catch_body: None,
            let_body: None,
        }
    }

    /// The scopes of `node`'s children.
    fn below(self, node: Node<'t>, adapter: ArgumentAdapter) -> Self {
        let kind = node.kind();
        let callable = is_callable_scope(kind);
        Self {
            parent: Some(node),
            function: if callable { node } else { self.function },
            block: if callable || is_block_scope(kind) {
                node
            } else {
                self.block
            },
            catch_body: if matches!(kind, "catch_clause" | "catch_handler") {
                Some(node.child_by_field_name("body").unwrap_or(node))
            } else if callable {
                None
            } else {
                self.catch_body
            },
            let_body: if adapter == ArgumentAdapter::Rust {
                node.child_by_field_name("consequence")
                    .or_else(|| node.child_by_field_name("body"))
                    .or(self.let_body)
            } else {
                None
            },
        }
    }
}

fn binding_scope<'t>(
    node: Node<'t>,
    adapter: ArgumentAdapter,
    parameter: bool,
    scopes: Scopes<'t>,
) -> Node<'t> {
    if parameter && let Some(body) = scopes.catch_body {
        return body;
    }
    if adapter == ArgumentAdapter::Rust
        && node.kind() == "let_condition"
        && let Some(body) = scopes.let_body
    {
        return body;
    }
    if matches!(node.kind(), "for_expression" | "enhanced_for_statement") {
        return node.child_by_field_name("body").unwrap_or(node);
    }
    if (parameter && matches!(node.kind(), "function_definition" | "lambda"))
        || node.kind() == "match_arm"
    {
        return node;
    }
    if parameter || adapter == ArgumentAdapter::Python {
        scopes.function
    } else {
        scopes.block
    }
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

fn local_definition_barrier<'a>(
    node: Node<'a>,
    source: &str,
    scopes: Scopes<'a>,
) -> Option<Node<'a>> {
    let declaration = crate::signatures::nodes::declaration_under(node, scopes.parent, source);
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
    is_callable_scope(scopes.function.kind()).then_some(scopes.function)
}
