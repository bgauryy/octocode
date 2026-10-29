//! The node walk: main traversal and deadline polls.

use super::collect::collect_node_facts;
use super::positions::NodePositions;
use super::rust::{RustContext, collect_rust_paths, rust_child_contexts, rust_inner_unsupported};
use super::{GraphAccumulator, NameToken, note, receiver};
use tree_sitter::Node;

use crate::graph::GraphFactCode;

use crate::signatures::nodes::{declaration_name, is_name_leaf};

/// Whether a node walk should read the clock at this step: the first step
/// and then once per 256 nodes. Per-node work is bounded (no parent climbs),
/// so a coarse poll overshoots the deadline by at most a few hundred nodes.
fn polls_deadline(steps: &mut u32) -> bool {
    let poll = steps.is_multiple_of(256);
    *steps = steps.wrapping_add(1);
    poll
}

/// A macro call in item position: directly in a file or module body, or as
/// an `expression_statement` there (`make_items!();`).
pub(super) fn is_item_level(node: Node<'_>) -> bool {
    let item_parent = |node: Node<'_>| matches!(node.kind(), "source_file" | "declaration_list");
    node.parent().is_some_and(|parent| {
        item_parent(parent)
            || parent.kind() == "expression_statement" && parent.parent().is_some_and(item_parent)
    })
}

pub(super) fn push_macro_gap(acc: &mut GraphAccumulator) {
    if !acc
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == GraphFactCode::RustMacroExpansion)
    {
        acc.diagnostics
            .push(note(GraphFactCode::RustMacroExpansion));
    }
}

pub(super) fn visit_node(
    root: Node<'_>,
    content: &str,
    line_index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    deadline: std::time::Instant,
    outer_scope: &[String],
    outer_declaration: Option<&(String, String)>,
) -> bool {
    enum Frame<'tree> {
        /// A node, its carried Rust context, and its depth below the root.
        Enter(Node<'tree>, RustContext, usize),
        ExitDeclaration,
        ExitModule,
    }

    let rust = acc.ext == "rs";
    let mut receivers = if acc.declarations_only {
        None
    } else {
        receiver::ReceiverTypes::new(&acc.ext, root)
    };
    #[cfg(feature = "tree-sitter-scala")]
    let mut scala_terms = (!acc.declarations_only && super::scala::is_scala(&acc.ext))
        .then(receiver::ScalaTerms::default);
    let mut frames = vec![Frame::Enter(root, RustContext::default(), 0)];
    let mut declarations: Vec<(String, String)> = outer_declaration.into_iter().cloned().collect();
    // Names of the enclosing `mod` items, outermost first.
    let mut module_scope: Vec<String> = outer_scope.to_vec();
    let mut children = Vec::new();
    let mut cursor = root.walk();
    let mut steps = 0_u32;
    // Ancestors of the node being entered, root first: the traversal keeps
    // them so no collector has to climb `Node::parent()`.
    let mut path: Vec<Node<'_>> = Vec::new();
    while let Some(frame) = frames.pop() {
        if polls_deadline(&mut steps) && std::time::Instant::now() >= deadline {
            return false;
        }
        let (node, mut context) = match frame {
            Frame::Enter(node, context, depth) => {
                path.truncate(depth);
                if let Some(tokens) = acc.name_tokens.as_mut() {
                    let format = rust
                        && node.kind() == "string_literal"
                        && path
                            .last()
                            .is_some_and(|parent| parent.kind() == "token_tree");
                    if format || is_name_leaf(node) {
                        tokens.push(NameToken {
                            start: node.start_byte(),
                            end: node.end_byte(),
                            format,
                        });
                    }
                }
                (node, context)
            }
            Frame::ExitDeclaration => {
                declarations.pop();
                continue;
            }
            Frame::ExitModule => {
                module_scope.pop();
                continue;
            }
        };
        if rust {
            context.unsupported |= rust_inner_unsupported(node, content);
            if !context.test
                && (context.attributes.cfg_test
                    || super::rust_cfg::inner_cfg_requires_test(node, content))
            {
                let range = line_index.range(node);
                acc.test_lines
                    .push([range.start.line + 1, range.end.line + 1]);
                context.test = true;
            }
        }
        let active = declarations.last();
        let rust_node = rust.then_some(RustNodeContext {
            context: &context,
            module_scope: &module_scope,
        });
        let calls_before = acc.calls.len();
        let identity = collect_node_facts(
            node,
            &path,
            content,
            line_index,
            acc,
            active.map(|(id, _)| id.as_str()),
            active.map(|(_, name)| name.as_str()),
            rust_node,
            deadline,
        );
        // An expression macro body's paths are the flat tokens the outer
        // walk already read.
        if rust && !acc.declarations_only && acc.macro_chain.is_none() {
            collect_rust_paths(
                node,
                &path,
                content,
                line_index,
                acc,
                declarations.last().map(|(id, _)| id.as_str()),
                module_scope.len(),
            );
        }
        path.push(node);
        if acc.calls.len() > calls_before
            && let Some(receivers) = receivers.as_mut()
            && let Some(call) = acc.calls.last_mut()
        {
            call.receiver_type = receivers.receiver_type(node, &path, content);
        }
        #[cfg(feature = "tree-sitter-scala")]
        if acc.calls.len() > calls_before
            && let Some(terms) = scala_terms.as_mut()
            && let Some(call) = acc.calls.last_mut()
        {
            let scope = terms.observe(node, &path, content);
            call.receiver_type = scope.receiver_type;
            call.term_binding = scope.term_binding;
            call.self_template = scope.self_template;
        }
        let depth = path.len();
        if let Some(identity) = identity {
            declarations.push(identity);
            frames.push(Frame::ExitDeclaration);
        }
        if rust
            && node.kind() == "mod_item"
            && let Some(name) = declaration_name(node, content)
        {
            module_scope.push(name);
            frames.push(Frame::ExitModule);
        }

        // Preserve preorder and declaration lifetimes without using the call stack.
        // A cursor enumerates wide sibling lists without repeated child indexing.
        children.clear();
        cursor.reset(node);
        children.extend(node.named_children(&mut cursor));
        let children_start = frames.len();
        if rust {
            let inherited = context.for_children(node);
            let contexts = rust_child_contexts(&children, content, &inherited);
            frames.extend(
                children
                    .iter()
                    .zip(contexts)
                    .map(|(child, context)| Frame::Enter(*child, context, depth)),
            );
        } else {
            frames.extend(
                children
                    .iter()
                    .map(|child| Frame::Enter(*child, RustContext::default(), depth)),
            );
        }
        frames[children_start..].reverse();
    }
    std::time::Instant::now() < deadline
}

/// Rust traversal state for one node: its carried context and the names of
/// the `mod` items that enclose it (outermost first, the node itself excluded).
#[derive(Clone, Copy)]
pub(super) struct RustNodeContext<'a> {
    pub(super) context: &'a RustContext,
    pub(super) module_scope: &'a [String],
}

/// A Go `import ( ... )` / `import "x"` declaration: its `import_spec`
/// children are the imports; the declaration itself would duplicate the
/// first one on the `import` line.
pub(super) fn is_grouped_go_import(node: Node<'_>) -> bool {
    if node.kind() != "import_declaration" {
        return false;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| matches!(child.kind(), "import_spec" | "import_spec_list"))
}

/// A function declared directly in a type body (class, struct, trait,
/// interface, impl, enum) is a `method`, as the JS/TS extractor labels it;
/// Go/Java/C# method syntax always is. The enclosing declaration's kind is
/// the suffix of its id (`declaration:…@byte:kind`).
pub(super) fn method_kind(
    kind: &'static str,
    node: Node<'_>,
    active_decl: Option<&str>,
) -> &'static str {
    if kind != "function" {
        return kind;
    }
    if matches!(
        node.kind(),
        "method_declaration" | "method_definition" | "singleton_method" | "method_elem"
    ) {
        return "method";
    }
    let in_type = active_decl
        .and_then(|id| id.rsplit_once(':'))
        .is_some_and(|(_, parent)| {
            matches!(
                parent,
                "class" | "struct" | "trait" | "interface" | "impl" | "enum"
            )
        });
    if in_type { "method" } else { kind }
}
