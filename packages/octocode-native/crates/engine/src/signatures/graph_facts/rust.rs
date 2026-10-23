//! Rust-specific graph-fact collectors: `use`-tree expansion, module
//! attributes/scopes, and unsupported-context detection.

use tree_sitter::Node;

use crate::signatures::nodes::node_text;

use super::{GraphAccumulator, NodePositions};
use crate::graph::GraphFactImport;

/// Outer attributes that precede one item, folded as `(path, unsupported)`
/// plus whether a `cfg` compiles the item only under test.
#[derive(Clone, Debug, Default)]
pub(super) struct RustAttributes {
    pub(super) path: Option<String>,
    pub(super) unsupported: bool,
    pub(super) cfg_test: bool,
}

/// Context carried down the single graph-fact traversal, so no Rust collector
/// has to walk `Node::parent()` (a root-down search per call, O(n·depth)) or
/// `prev_named_sibling()` (a parent search plus a sibling scan).
#[derive(Clone, Debug, Default)]
pub(super) struct RustContext {
    /// Outer attributes directly preceding this node.
    pub(super) attributes: RustAttributes,
    /// Some strict ancestor is a `block`, `function_item` or `impl_item`.
    pub(super) block_local: bool,
    /// This node or an ancestor carries unsupported outer or inner attributes.
    pub(super) unsupported: bool,
    /// A strict ancestor compiles only under test (its span is recorded).
    pub(super) test: bool,
}

impl RustContext {
    /// Context for the children of `node`, whose own context is `self`.
    pub(super) fn for_children(&self, node: Node<'_>) -> Self {
        Self {
            attributes: RustAttributes::default(),
            block_local: self.block_local
                || matches!(node.kind(), "block" | "function_item" | "impl_item"),
            unsupported: self.unsupported,
            test: self.test,
        }
    }
}

/// Fold an attribute run (nearest attribute first) the same way the item's
/// preceding siblings were read: the farthest `#[path]` wins, a repeated
/// `#[path]` or any non-lint attribute is unsupported.
fn fold_rust_attributes<'t>(
    nearest_first: impl Iterator<Item = Node<'t>>,
    content: &str,
) -> RustAttributes {
    let mut folded = RustAttributes::default();
    for attribute in nearest_first {
        let text = node_text(attribute, content).unwrap_or_default();
        let inner = text
            .trim()
            .strip_prefix("#[")
            .and_then(|text| text.strip_suffix(']'))
            .unwrap_or_default()
            .trim();
        let name = inner.split(['(', '=']).next().unwrap_or_default().trim();
        match name {
            "path" => {
                let literal = inner.split_once('=').map(|(_, value)| value.trim());
                let value = literal
                    .and_then(|value| value.strip_prefix('"'))
                    .and_then(|value| value.strip_suffix('"'));
                if let Some(value) =
                    value.filter(|value| !value.contains('\\') && !value.contains('\0'))
                {
                    if folded.path.is_some() {
                        folded.unsupported = true;
                    }
                    folded.path = Some(value.to_owned());
                } else {
                    folded.unsupported = true;
                }
            }
            // `cfg` gates whether a module compiles, not which file it maps
            // to: the edge is conditional, not unknown.
            "cfg" => folded.cfg_test |= super::rust_cfg::cfg_requires_test(inner),
            "allow"
            | "warn"
            | "deny"
            | "forbid"
            | "expect"
            | "doc"
            | "deprecated"
            | "no_implicit_prelude"
            | "macro_use"
            | "test" => {}
            // A conditional attribute that rewrites the path does change it.
            "cfg_attr" if !inner.contains("path") => {}
            _ => folded.unsupported = true,
        }
    }
    folded
}

/// Contexts for the named children of `node` (in order), computed in one
/// sibling pass: each child gets the attribute run directly before it
/// (comments are skipped, anything else ends the run), plus the inherited
/// ancestor flags (`inherited` is the parent's [`RustContext::for_children`]).
/// The caller adds the child's own inner attributes when it enters the child.
pub(super) fn rust_child_contexts(
    children: &[Node<'_>],
    content: &str,
    inherited: &RustContext,
) -> Vec<RustContext> {
    let mut run_start: Option<usize> = None;
    let mut contexts = Vec::with_capacity(children.len());
    for (index, child) in children.iter().enumerate() {
        let attributes = match run_start {
            Some(start) => fold_rust_attributes(
                children[start..index]
                    .iter()
                    .rev()
                    .copied()
                    .filter(|item| item.kind() == "attribute_item"),
                content,
            ),
            None => RustAttributes::default(),
        };
        contexts.push(RustContext {
            unsupported: inherited.unsupported || attributes.unsupported,
            block_local: inherited.block_local,
            test: inherited.test,
            attributes,
        });
        match child.kind() {
            "attribute_item" => {
                run_start.get_or_insert(index);
            }
            "line_comment" | "block_comment" => {}
            _ => run_start = None,
        }
    }
    contexts
}

pub(super) fn rust_inner_unsupported(node: Node<'_>, content: &str) -> bool {
    let mut cursor = node.walk();

    node.named_children(&mut cursor).any(|child| {
        if child.kind() != "inner_attribute_item" {
            return false;
        }
        let text = node_text(child, content).unwrap_or_default();
        let inner = text
            .trim()
            .strip_prefix("#![")
            .and_then(|text| text.strip_suffix(']'))
            .unwrap_or_default()
            .trim();
        let name = inner.split(['(', '=']).next().unwrap_or_default().trim();
        !(matches!(
            name,
            "allow"
                | "warn"
                | "deny"
                | "forbid"
                | "expect"
                | "doc"
                | "deprecated"
                | "no_implicit_prelude"
                | "cfg"
                | "macro_use"
        ) || name == "cfg_attr" && !inner.contains("path"))
    })
}

/// Expand Rust use trees through grammar nodes, preserving aliases and multiline groups.
pub(super) fn collect_rust_imports(
    node: Node<'_>,
    module_scope: &[String],
    content: &str,
    index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    unsupported: bool,
    deadline: std::time::Instant,
) {
    let mut pending = vec![(node, String::new())];
    while let Some((node, prefix)) = pending.pop() {
        if std::time::Instant::now() >= deadline {
            return;
        }
        let text = node_text(node, content).unwrap_or_default();
        match node.kind() {
            "use_list" => {
                let mut cursor = node.walk();
                let children_start = pending.len();
                for child in node.named_children(&mut cursor) {
                    pending.push((child, prefix.clone()));
                }
                pending[children_start..].reverse();
            }
            "scoped_use_list" => {
                let path = node
                    .child_by_field_name("path")
                    .and_then(|n| node_text(n, content))
                    .unwrap_or_default();
                let joined = if prefix.is_empty() {
                    path.to_owned()
                } else {
                    format!("{prefix}::{path}")
                };
                if let Some(list) = node.child_by_field_name("list") {
                    pending.push((list, joined));
                }
            }
            _ => {
                let (path_node, alias_node) = if node.kind() == "use_as_clause" {
                    (
                        node.child_by_field_name("path").unwrap_or(node),
                        node.child_by_field_name("alias"),
                    )
                } else {
                    (node, None)
                };
                let path = node_text(path_node, content).unwrap_or(text);
                let alias = alias_node.and_then(|alias| node_text(alias, content));
                let imported_node = path_node.child_by_field_name("name").unwrap_or(path_node);
                let imported_node =
                    matches!(imported_node.kind(), "identifier" | "type_identifier")
                        .then_some(imported_node);
                let specifier = if path == "self" && !prefix.is_empty() {
                    prefix.to_owned()
                } else if prefix.is_empty() {
                    path.to_owned()
                } else {
                    format!("{prefix}::{path}")
                };
                let imported = specifier
                    .rsplit("::")
                    .next()
                    .unwrap_or(&specifier)
                    .to_owned();
                let line = index.range(node).start.line + 1;
                acc.imports.push(GraphFactImport {
                    id: format!("import:{specifier}:{line}:{}", acc.imports.len()),
                    specifier,
                    line,
                    import_kind: "value".to_owned(),
                    local_name: Some(alias.unwrap_or(&imported).to_owned()),
                    imported_name: Some(imported),
                    imported_range: imported_node.map(|name| index.range(name)),
                    local_range: alias_node.or(imported_node).map(|name| index.range(name)),
                    resolution_hint: unsupported.then(|| "unsupported".to_owned()),
                    module_scope: Some(module_scope.to_vec()),
                });
            }
        }
    }
}

/// The Rust qualified paths `node` holds outside call and `use` position,
/// recorded so the linker can depend on their module prefix: an outermost
/// `scoped_identifier`/`scoped_type_identifier` (a constant, type, variant,
/// or function used as a value), or a `a::b` token run inside a macro token
/// tree, which the grammar leaves as flat tokens. Call callees and macro
/// names are call facts already; `use` items are imports. `ancestors` are
/// the node's ancestors, root first.
pub(super) fn collect_rust_paths(
    node: Node<'_>,
    ancestors: &[Node<'_>],
    content: &str,
    index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    caller_id: Option<&str>,
    inline_depth: usize,
) {
    let mut record = |path: String, line: u32| {
        if path.contains("::") && !path.starts_with("Self::") && !path.starts_with('<') {
            acc.paths.push(crate::graph::GraphFactPath {
                line,
                path,
                caller_id: caller_id.map(str::to_owned),
                inline_depth: u32::try_from(inline_depth).unwrap_or(u32::MAX),
            });
        }
    };
    match node.kind() {
        "scoped_identifier" | "scoped_type_identifier" => {
            let parent = ancestors.last();
            let prefix_or_callee = parent.is_some_and(|parent| {
                matches!(
                    parent.kind(),
                    "scoped_identifier" | "scoped_type_identifier"
                ) || ["function", "macro"]
                    .iter()
                    .any(|field| parent.child_by_field_name(field) == Some(node))
            });
            if prefix_or_callee
                || ancestors
                    .iter()
                    .any(|ancestor| ancestor.kind() == "use_declaration")
            {
                return;
            }
            if let Some(text) = node_text(node, content) {
                record(
                    text.split_whitespace().collect(),
                    index.range(node).start.line + 1,
                );
            }
        }
        "token_tree" => {
            // (path so far, its line, the last token was `::`)
            let mut run: Option<(String, u32, bool)> = None;
            let mut cursor = node.walk();
            for token in node.children(&mut cursor) {
                let kind = token.kind();
                let name = matches!(kind, "identifier" | "crate" | "self" | "super");
                match (&mut run, kind, name) {
                    (Some((_, _, separated @ false)), "::", _) => *separated = true,
                    (Some((path, _, separated @ true)), _, true) => {
                        path.push_str("::");
                        path.push_str(node_text(token, content).unwrap_or_default());
                        *separated = false;
                    }
                    _ => {
                        if let Some((path, line, _)) = run.take() {
                            record(path, line);
                        }
                        if name {
                            run = Some((
                                node_text(token, content).unwrap_or_default().to_owned(),
                                index.range(token).start.line + 1,
                                false,
                            ));
                        }
                    }
                }
            }
            if let Some((path, line, _)) = run {
                record(path, line);
            }
        }
        _ => {}
    }
}
