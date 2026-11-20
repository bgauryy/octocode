//! The node walk: main traversal and deadline polls.

use super::collect::collect_node_facts;
use super::positions::{NodePositions, leading_annotation_starts};
use super::rust::{
    RustContext, collect_rust_paths, rust_child_contexts, rust_inner_unsupported_among,
};
use super::{GraphAccumulator, NameToken, note, receiver};
use crate::signatures::node_kinds::kind_of;
use tree_sitter::Node;

use crate::graph::{GraphFactCode, GraphFactDiagnostic};

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
    let item_parent = |node: Node<'_>| matches!(kind_of(node), "source_file" | "declaration_list");
    node.parent().is_some_and(|parent| {
        item_parent(parent)
            || kind_of(parent) == "expression_statement" && parent.parent().is_some_and(item_parent)
    })
}

/// Record that the items a macro call at 1-based `line` generates are not
/// read; one diagnostic per call, so a reader of the file's declarations can
/// say where its outline is incomplete.
pub(super) fn push_macro_gap(acc: &mut GraphAccumulator, line: u32) {
    if !acc.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == GraphFactCode::RustMacroExpansion && diagnostic.line == Some(line)
    }) {
        acc.diagnostics.push(GraphFactDiagnostic {
            line: Some(line),
            ..note(GraphFactCode::RustMacroExpansion)
        });
    }
}

/// Keep `node` when it is a name leaf, or a Rust macro's format string.
fn note_name_token(acc: &mut GraphAccumulator, node: Node<'_>, path: &[Node<'_>], rust: bool) {
    let Some(tokens) = acc.name_tokens.as_mut() else {
        return;
    };
    let format = rust
        && kind_of(node) == "string_literal"
        && path
            .last()
            .is_some_and(|parent| kind_of(parent) == "token_tree");
    if format || is_name_leaf(node) {
        tokens.push(NameToken {
            start: node.start_byte(),
            end: node.end_byte(),
            format,
        });
    }
}

/// Whether a walk that reads only one spelling's spans, or only the nodes
/// touching some byte spans, goes into `child`.
fn entered(acc: &GraphAccumulator, child: Node<'_>) -> bool {
    acc.only_spelling
        .as_ref()
        .is_none_or(|spelled| spelled.within(child.start_byte(), child.end_byte()))
        && acc
            .only_spans
            .as_ref()
            .is_none_or(|spans| spans.touches(child.start_byte(), child.end_byte()))
}

/// Byte spans a narrowed walk enters ([`GraphAccumulator::only_spans`]): a
/// node is entered when its bytes touch one, end points included, so a node
/// that holds a byte of a span, or starts or ends at its edge, is read.
pub(super) struct Spans(Vec<(usize, usize)>);

impl Spans {
    /// `spans` as `(start, end)` byte pairs, merged where they touch.
    pub(super) fn new(mut spans: Vec<(usize, usize)>) -> Self {
        spans.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::with_capacity(spans.len());
        for (start, end) in spans {
            match merged.last_mut() {
                Some(last) if start <= last.1 => last.1 = last.1.max(end),
                _ => merged.push((start, end)),
            }
        }
        Self(merged)
    }

    /// Whether bytes `start..=end` touch a span. Merged spans are disjoint and
    /// sorted, so their ends ascend: the first span ending at or past `start`
    /// is the only one that can.
    pub(super) fn touches(&self, start: usize, end: usize) -> bool {
        let next = self.0.partition_point(|&(_, span_end)| span_end < start);
        self.0
            .get(next)
            .is_some_and(|&(span_start, _)| span_start <= end)
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
        /// A node, its carried Rust context, its depth below the root, and the
        /// start of the outer annotations written before it.
        Enter(Node<'tree>, RustContext, usize, Option<usize>),
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
    let mut frames = vec![Frame::Enter(root, RustContext::default(), 0, None)];
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
            Frame::Enter(node, context, depth, leading) => {
                path.truncate(depth);
                acc.leading_start = leading.map(|start| (node.id(), start));
                note_name_token(acc, node, &path, rust);
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
        let kind = kind_of(node);
        // One child scan serves the inner-attribute checks and the frames
        // below; a leaf has no children to read.
        children.clear();
        if node.named_child_count() > 0 {
            cursor.reset(node);
            children.extend(node.named_children(&mut cursor));
        }
        if rust {
            context.unsupported |= rust_inner_unsupported_among(&children, content);
            let owned = (!context.test)
                .then(|| super::rust::owned_test_start(kind, &children, content))
                .flatten();
            if !context.test
                && (context.attributes.cfg_test
                    || owned.is_some()
                    || super::rust_cfg::inner_cfg_requires_test_among(&children, content))
            {
                let range = line_index.range(node);
                let start = context
                    .attributes
                    .line_start
                    .or(owned)
                    .map_or(range.start.line, |at| line_index.position(at).line);
                acc.test_lines.push([start + 1, range.end.line + 1]);
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
        #[cfg(feature = "tree-sitter-scala")]
        if let Some(terms) = scala_terms.as_mut() {
            terms.collect_parameterless(&path, content, line_index, acc, declarations.last());
        }
        if acc.calls.len() > calls_before
            && let Some(receivers) = receivers.as_mut()
            && let Some(call) = acc.calls.last_mut()
        {
            call.receiver_type = receivers.receiver_type(node, &path, content);
            if rust {
                let typed = call.receiver_type.is_some();
                call.receiver_call = receivers.rust_receiver_call(node, &path, typed, content);
            } else if acc.ext == "go" {
                call.receiver_call = receivers.go_receiver_call(node, &path, content);
            }
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
            && kind == "mod_item"
            && let Some(name) = declaration_name(node, content)
        {
            module_scope.push(name);
            frames.push(Frame::ExitModule);
        }

        // Preserve preorder and declaration lifetimes without using the call stack.
        retain_graph_children(acc, node, content, &mut children);
        let children_start = frames.len();
        let contexts = child_contexts(rust, &context, kind, &children, content);
        frames.extend(
            children
                .iter()
                .zip(contexts)
                .zip(leading_annotation_starts(&children))
                .filter(|((child, _), _)| entered(acc, **child))
                .map(|((child, context), leading)| Frame::Enter(*child, context, depth, leading)),
        );
        frames[children_start..].reverse();
    }
    std::time::Instant::now() < deadline
}

/// The carried context of each of `children`: a Rust file's attribute and
/// test flags, one default for every other language.
fn child_contexts(
    rust: bool,
    context: &RustContext,
    kind: &str,
    children: &[Node<'_>],
    content: &str,
) -> Vec<RustContext> {
    if rust {
        rust_child_contexts(children, content, &context.for_children(kind))
    } else {
        std::iter::repeat_with(RustContext::default)
            .take(children.len())
            .collect()
    }
}

/// An outline and an enclosing-name lookup still read a disabled preprocessor
/// branch; a graph holds only what can compile.
fn retain_graph_children<'t>(
    acc: &GraphAccumulator,
    node: Node<'t>,
    content: &str,
    children: &mut Vec<Node<'t>>,
) {
    if !acc.declarations_only
        && matches!(
            acc.visibility_adapter,
            crate::signatures::visibility::VisibilityAdapter::CFamily
        )
    {
        super::c_family::retain_compiled(node, content, children);
    }
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
    if kind_of(node) != "import_declaration" {
        return false;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| matches!(kind_of(child), "import_spec" | "import_spec_list"))
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
        kind_of(node),
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
