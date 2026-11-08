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
    /// The `path` a `cfg_attr` whose predicate requires test gives.
    pub(super) test_path: Option<String>,
    /// The `path` a `cfg_attr` whose predicate is false under test gives.
    pub(super) plain_path: Option<String>,
    pub(super) unsupported: bool,
    pub(super) cfg_test: bool,
    /// The start byte of the first attribute of the run that begins its
    /// line. The attributes belong to the item, so a test span starts there;
    /// one after other code on its line leaves that line to the code.
    pub(super) line_start: Option<usize>,
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
    pub(super) fn for_children(&self, kind: &str) -> Self {
        Self {
            attributes: RustAttributes::default(),
            block_local: self.block_local
                || matches!(kind, "block" | "function_item" | "impl_item"),
            unsupported: self.unsupported,
            test: self.test,
        }
    }
}

#[cfg(test)]
thread_local! {
    /// Attributes folded on this thread, for the work-bound test.
    static FOLDED_ATTRIBUTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Attributes folded on this thread so far: each should fold once.
#[cfg(test)]
pub(in crate::signatures) fn folded_attributes() -> usize {
    FOLDED_ATTRIBUTES.with(std::cell::Cell::get)
}

/// Fold one more attribute into the run read so far, in source order: the
/// farthest (first written) `#[path]` wins, a repeated `#[path]` or any
/// non-lint attribute is unsupported. Each attribute is folded once as the
/// run grows, so a run of any length costs linear work.
fn fold_rust_attribute(folded: &mut RustAttributes, attribute: Node<'_>, content: &str) {
    #[cfg(test)]
    FOLDED_ATTRIBUTES.with(|count| count.set(count.get() + 1));
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
            if let Some(value) = path_literal(inner) {
                if folded.path.is_some() {
                    folded.unsupported = true;
                } else {
                    folded.path = Some(value.to_owned());
                }
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
        // Each arm names the file for one build: an arm that names none
        // for a build leaves the file another arm gave it.
        "cfg_attr" => {
            if let Some(path) =
                super::rust_cfg::cfg_attr_under_test(inner, "path").and_then(path_literal)
            {
                folded.test_path.get_or_insert_with(|| path.to_owned());
            }
            if let Some(path) =
                super::rust_cfg::cfg_attr_outside_test(inner, "path").and_then(path_literal)
            {
                folded.plain_path.get_or_insert_with(|| path.to_owned());
            }
            folded.unsupported = true;
        }
        _ => folded.unsupported = true,
    }
}

/// The file a `path = "x.rs"` attribute body names, when it is a plain literal.
fn path_literal(attribute: &str) -> Option<&str> {
    attribute
        .split_once('=')?
        .1
        .trim()
        .strip_prefix('"')?
        .strip_suffix('"')
        .filter(|value| !value.contains('\\') && !value.contains('\0'))
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
    // The run's attributes folded so far; each attribute folds once.
    let mut run: Option<RustAttributes> = None;
    let mut contexts = Vec::with_capacity(children.len());
    for child in children {
        let attributes = run.clone().unwrap_or_default();
        contexts.push(RustContext {
            unsupported: inherited.unsupported || attributes.unsupported,
            block_local: inherited.block_local,
            test: inherited.test,
            attributes,
        });
        match child.kind() {
            "attribute_item" => {
                let folded = run.get_or_insert_with(RustAttributes::default);
                if folded.line_start.is_none() && begins_line(content, child.start_byte()) {
                    folded.line_start = Some(child.start_byte());
                }
                fold_rust_attribute(folded, *child, content);
            }
            "line_comment" | "block_comment" => {}
            _ => run = None,
        }
    }
    contexts
}

/// Whether only whitespace precedes byte `at` on its line.
fn begins_line(content: &str, at: usize) -> bool {
    let before = content.get(..at).unwrap_or_default();
    before
        .rsplit_once('\n')
        .map_or(before, |(_, line)| line)
        .trim()
        .is_empty()
}

pub(super) fn rust_inner_unsupported(node: Node<'_>, content: &str) -> bool {
    let mut cursor = node.walk();
    let children: Vec<Node<'_>> = node.named_children(&mut cursor).collect();
    rust_inner_unsupported_among(&children, content)
}

/// [`rust_inner_unsupported`] over a node's already-read named children.
pub(super) fn rust_inner_unsupported_among(children: &[Node<'_>], content: &str) -> bool {
    children.iter().any(|&child| {
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
                    scope_id: None,
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
