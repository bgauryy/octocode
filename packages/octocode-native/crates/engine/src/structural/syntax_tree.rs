//! Bounded, paginated tree-sitter syntax-tree inspection.
//!
//! This is deliberately a view over the structural parser's existing grammar
//! registry. It does not retain a parser or source snapshot between pages; the
//! caller supplies the same content and asks for the next preorder offset.

use std::time::Instant;

use tree_sitter::Node;

use crate::signatures::extractor::AST_EXECUTION_TIMEOUT;
use crate::text::file_extension::extension_of;
use crate::text::utf8_offsets::LineIndex;

use super::octo::parse_tree_with_deadline;
use super::types::StructuralDiagnostic;
use crate::signatures::languages;

mod line_root;

use line_root::{LineRoot, line_node};

const MAX_SYNTAX_TREE_NODES: usize = 1_000_000;
const DEFAULT_NODE_LIMIT: usize = 1_000;
const MAX_NODE_LIMIT: usize = 100_000;

#[derive(Clone, Debug, Default)]
pub struct SyntaxTreeInspectOptions {
    pub named_only: Option<bool>,
    pub node_offset: Option<u32>,
    pub node_limit: Option<u32>,
    /// 1-based line: the walk starts at the smallest node (named, with
    /// `named_only`) covering that line's text instead of the file root.
    pub line: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct SyntaxTreeNode {
    pub id: u32,
    pub parent_id: Option<u32>,
    pub kind: String,
    /// The grammar field this node fills in its parent (`name`, `body`), the
    /// name a rule's `field` matches; `None` for the walk's root and for a
    /// child no field names.
    pub field: Option<String>,
    pub named: bool,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub start_byte: u32,
    pub end_byte: u32,
}

#[derive(Clone)]
pub struct SyntaxTreeInspectResult {
    pub nodes: Vec<SyntaxTreeNode>,
    pub total_nodes: u32,
    pub next_offset: Option<u32>,
    pub status: String,
    pub diagnostics: Vec<StructuralDiagnostic>,
}

struct Pending<'tree> {
    node: Node<'tree>,
    field: Option<&'tree str>,
    included_parent: Option<u32>,
}

fn diagnostic(
    code: &str,
    severity: &str,
    stage: &str,
    message: impl Into<String>,
    path: &str,
) -> StructuralDiagnostic {
    StructuralDiagnostic::new(code, severity, stage, message).with_path(path)
}

/// Inspect a bounded preorder view of the syntax tree, parsed with the
/// grammar of `extension` (default: the path's own). IDs are assigned only to
/// emitted nodes, so `named_only` pages remain contiguous and parent IDs
/// always point to the nearest emitted ancestor.
pub fn inspect_with_extension(
    content: &str,
    file_path: &str,
    extension: Option<&str>,
    options: Option<SyntaxTreeInspectOptions>,
) -> SyntaxTreeInspectResult {
    inspect_bounded(
        content,
        file_path,
        extension,
        options,
        MAX_SYNTAX_TREE_NODES,
    )
}

/// [`inspect_with_extension`] with the walk's node budget as a parameter.
fn inspect_bounded(
    content: &str,
    file_path: &str,
    extension: Option<&str>,
    options: Option<SyntaxTreeInspectOptions>,
    max_nodes: usize,
) -> SyntaxTreeInspectResult {
    let options = options.unwrap_or_default();
    let named_only = options.named_only.unwrap_or(false);
    let offset = options.node_offset.unwrap_or(0) as usize;
    let requested_limit = options.node_limit.unwrap_or(DEFAULT_NODE_LIMIT as u32) as usize;
    if requested_limit == 0 {
        return SyntaxTreeInspectResult {
            nodes: Vec::new(),
            total_nodes: 0,
            next_offset: None,
            status: "error".to_owned(),
            diagnostics: vec![diagnostic(
                "syntaxTree.options.invalidLimit",
                "error",
                "options",
                "nodeLimit must be greater than zero",
                file_path,
            )],
        };
    }
    let limit = requested_limit.min(MAX_NODE_LIMIT);
    let ext = extension
        .map(str::to_owned)
        .unwrap_or_else(|| extension_of(file_path, true, "txt"));

    if content.len() > crate::signatures::MAX_PARSE_SIZE {
        return SyntaxTreeInspectResult {
            nodes: Vec::new(),
            total_nodes: 0,
            next_offset: None,
            status: "error".to_owned(),
            diagnostics: vec![diagnostic(
                "syntaxTree.content.tooLarge",
                "error",
                "parse",
                format!(
                    "Syntax-tree content exceeds {} byte limit",
                    crate::signatures::MAX_PARSE_SIZE
                ),
                file_path,
            )],
        };
    }

    let Some(entry) = languages::find_entry(&ext) else {
        return SyntaxTreeInspectResult {
            nodes: Vec::new(),
            total_nodes: 0,
            next_offset: None,
            status: "error".to_owned(),
            diagnostics: vec![diagnostic(
                "syntaxTree.language.unsupported",
                "error",
                "parse",
                format!("No registered tree-sitter grammar for .{ext} files"),
                file_path,
            )],
        };
    };

    let deadline = Instant::now() + AST_EXECUTION_TIMEOUT;
    let tree = match parse_tree_with_deadline(&entry.language, content, deadline) {
        Ok(tree) => tree,
        Err(error) => {
            return SyntaxTreeInspectResult {
                nodes: Vec::new(),
                total_nodes: 0,
                next_offset: None,
                status: "error".to_owned(),
                diagnostics: vec![diagnostic(
                    error.code,
                    "error",
                    error.stage,
                    error.message,
                    file_path,
                )],
            };
        }
    };

    let line_index = LineIndex::tree_sitter(content);
    let mut line_note = None;
    let root = match options.line {
        None => tree.root_node(),
        Some(line) => match line_node(tree.root_node(), content, line, named_only) {
            Some(LineRoot::Covering(node)) => node,
            Some(LineRoot::Blank { node, note }) => {
                line_note = Some(diagnostic(
                    "syntaxTree.options.lineBlank",
                    "warning",
                    "options",
                    note,
                    file_path,
                ));
                node
            }
            None => {
                return SyntaxTreeInspectResult {
                    nodes: Vec::new(),
                    total_nodes: 0,
                    next_offset: None,
                    status: "error".to_owned(),
                    diagnostics: vec![diagnostic(
                        "syntaxTree.options.lineOutOfRange",
                        "error",
                        "options",
                        format!("line {line} is past the end of the file"),
                        file_path,
                    )],
                };
            }
        },
    };
    let mut pending = vec![Pending {
        node: root,
        field: None,
        included_parent: None,
    }];
    // Only the requested window is materialized; every included node still gets
    // its preorder id so ids, parents and `total_nodes` are page-independent.
    let window_start = offset;
    let window_end = offset.saturating_add(limit);
    let mut page_nodes = Vec::with_capacity(limit.min(1_000));
    let mut next_id: usize = 0;
    let mut cursor = tree.walk();
    let mut children = Vec::new();
    let mut status = if tree.root_node().has_error() {
        "partial".to_owned()
    } else {
        "ok".to_owned()
    };
    let mut diagnostics = if tree.root_node().has_error() {
        vec![diagnostic(
            "syntaxTree.parse.recovered",
            "warning",
            "parse",
            "Tree-sitter recovered from one or more syntax errors",
            file_path,
        )]
    } else {
        Vec::new()
    };
    diagnostics.extend(line_note);

    while let Some(Pending {
        node,
        field,
        included_parent,
    }) = pending.pop()
    {
        if Instant::now() >= deadline {
            status = "partial".to_owned();
            diagnostics.push(diagnostic(
                "syntaxTree.budget.deadline",
                "warning",
                "walk",
                "Syntax-tree traversal exceeded its execution deadline",
                file_path,
            ));
            break;
        }
        let included = !named_only || node.is_named();
        let next_parent = if included {
            if next_id >= max_nodes {
                status = "partial".to_owned();
                diagnostics.push(diagnostic(
                    "syntaxTree.budget.nodeLimit",
                    "warning",
                    "walk",
                    format!("Syntax-tree traversal reached the {max_nodes} node limit"),
                    file_path,
                ));
                break;
            }
            let id = next_id as u32;
            if (window_start..window_end).contains(&next_id) {
                let start = line_index.byte_to_position(content, node.start_byte() as u32);
                let end = line_index.byte_to_position(content, node.end_byte() as u32);
                page_nodes.push(SyntaxTreeNode {
                    id,
                    parent_id: included_parent,
                    kind: node.kind().to_owned(),
                    field: field.map(str::to_owned),
                    named: node.is_named(),
                    start_line: start.0 + 1,
                    start_column: start.1,
                    end_line: end.0 + 1,
                    end_column: end.1,
                    start_byte: node.start_byte().min(u32::MAX as usize) as u32,
                    end_byte: node.end_byte().min(u32::MAX as usize) as u32,
                });
            }
            next_id += 1;
            Some(id)
        } else {
            included_parent
        };

        // Push children in reverse source order so the stack visits preorder.
        // One reused cursor: `node.child(i)` scans siblings linearly (O(k²));
        // the cursor also names each child's field.
        children.clear();
        cursor.reset(node);
        if cursor.goto_first_child() {
            loop {
                children.push((cursor.node(), cursor.field_name()));
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        pending.extend(children.drain(..).rev().map(|(child, field)| Pending {
            node: child,
            field,
            included_parent: next_parent,
        }));
    }

    let total_nodes = next_id.min(u32::MAX as usize) as u32;
    let end = window_end.min(next_id);
    // Every numbered node pages, whether the walk finished, recovered from
    // syntax errors, or stopped at its deadline or node budget; only nodes
    // past a budget stop are unknown.
    let next_offset = (end < next_id).then_some(end as u32);
    SyntaxTreeInspectResult {
        nodes: page_nodes,
        total_nodes,
        next_offset,
        status,
        diagnostics,
    }
}

#[cfg(test)]
mod tests;
