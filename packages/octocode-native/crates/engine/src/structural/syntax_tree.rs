//! Bounded, paginated tree-sitter syntax-tree inspection.
//!
//! This is deliberately a view over the structural parser's existing grammar
//! registry. It does not retain a parser or source snapshot between pages; the
//! caller supplies the same content and asks for the next preorder offset.

use std::time::Instant;

#[cfg(feature = "napi-addon")]
use napi_derive::napi;
use tree_sitter::Node;

use crate::signatures::extractor::AST_EXECUTION_TIMEOUT;
use crate::text::file_extension::get_extension_internal;
use crate::text::utf8_offsets::LineIndex;

use super::octo::parse_tree_with_deadline;
use super::types::StructuralDiagnostic;
use crate::signatures::languages;

const MAX_SYNTAX_TREE_NODES: usize = 1_000_000;
const DEFAULT_NODE_LIMIT: usize = 1_000;
const MAX_NODE_LIMIT: usize = 100_000;

#[cfg_attr(feature = "napi-addon", napi(object))]
#[derive(Clone, Debug, Default)]
pub struct SyntaxTreeInspectOptions {
    pub named_only: Option<bool>,
    pub node_offset: Option<u32>,
    pub node_limit: Option<u32>,
}

#[cfg_attr(feature = "napi-addon", napi(object))]
#[derive(Clone, Debug)]
pub struct SyntaxTreeNode {
    pub id: u32,
    pub parent_id: Option<u32>,
    pub kind: String,
    pub named: bool,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub start_byte: u32,
    pub end_byte: u32,
}

#[cfg_attr(feature = "napi-addon", napi(object))]
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
    included_parent: Option<u32>,
}

fn diagnostic(
    code: &str,
    severity: &str,
    stage: &str,
    message: impl Into<String>,
    path: &str,
) -> StructuralDiagnostic {
    StructuralDiagnostic::new(code, severity, stage, message)
        .with_path(path)
        .with_recovery("Use a supported source extension and keep the same content when requesting the next page.")
}

/// Inspect a bounded preorder view of the syntax tree. IDs are assigned only
/// to emitted nodes, so `named_only` pages remain contiguous and parent IDs
/// always point to the nearest emitted ancestor.
pub fn inspect(
    content: &str,
    file_path: &str,
    options: Option<SyntaxTreeInspectOptions>,
) -> SyntaxTreeInspectResult {
    inspect_with_extension(content, file_path, None, options)
}

pub fn inspect_with_extension(
    content: &str,
    file_path: &str,
    extension: Option<&str>,
    options: Option<SyntaxTreeInspectOptions>,
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
        .unwrap_or_else(|| get_extension_internal(file_path, true, "txt"));

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

    let line_index = LineIndex::new(content);
    let mut pending = vec![Pending {
        node: tree.root_node(),
        included_parent: None,
    }];
    // Only the requested window is materialized; every included node still gets
    // its preorder id so ids, parents and `total_nodes` are page-independent.
    let window_start = offset;
    let window_end = offset.saturating_add(limit);
    let mut page_nodes = Vec::with_capacity(limit.min(1_000));
    let mut next_id: usize = 0;
    let mut budget_truncated = false;
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

    while let Some(Pending {
        node,
        included_parent,
    }) = pending.pop()
    {
        if Instant::now() >= deadline {
            status = "partial".to_owned();
            budget_truncated = true;
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
            if next_id >= MAX_SYNTAX_TREE_NODES {
                status = "partial".to_owned();
                budget_truncated = true;
                diagnostics.push(diagnostic(
                    "syntaxTree.budget.nodeLimit",
                    "warning",
                    "walk",
                    format!(
                        "Syntax-tree traversal reached the {} node limit",
                        MAX_SYNTAX_TREE_NODES
                    ),
                    file_path,
                ));
                break;
            }
            let id = next_id as u32;
            if (window_start..window_end).contains(&next_id) {
                let start = line_index.byte_to_position(node.start_byte() as u32);
                let end = line_index.byte_to_position(node.end_byte() as u32);
                page_nodes.push(SyntaxTreeNode {
                    id,
                    parent_id: included_parent,
                    kind: node.kind().to_owned(),
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
        // One reused cursor: `node.child(i)` scans siblings linearly (O(k²)).
        children.clear();
        children.extend(node.children(&mut cursor));
        pending.extend(children.drain(..).rev().map(|child| Pending {
            node: child,
            included_parent: next_parent,
        }));
    }

    let total_nodes = next_id.min(u32::MAX as usize) as u32;
    let end = window_end.min(next_id);
    // A recovered parse (`partial` from syntax errors) still pages; only a
    // deadline or node-budget stop leaves the tail unknown.
    let next_offset = if !budget_truncated && end < next_id {
        Some(end as u32)
    } else {
        None
    };
    SyntaxTreeInspectResult {
        nodes: page_nodes,
        total_nodes,
        next_offset,
        status,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inspect_source(
        source: &str,
        path: &str,
        options: Option<SyntaxTreeInspectOptions>,
    ) -> SyntaxTreeInspectResult {
        inspect(source, path, options)
    }

    #[test]
    fn full_tree_is_preorder_and_unicode_columns_are_utf16() {
        let result = inspect_source("const 😀 = 1;\n", "x.ts", None);
        assert_eq!(result.status, "ok");
        assert!(result.nodes.windows(2).all(|pair| pair[0].id < pair[1].id));
        let identifier = result
            .nodes
            .iter()
            .find(|node| node.kind == "identifier")
            .expect("identifier");
        assert_eq!(identifier.start_column, 6);
    }

    #[test]
    fn named_only_reparents_around_punctuation() {
        let result = inspect_source(
            "foo(1);",
            "x.ts",
            Some(SyntaxTreeInspectOptions {
                named_only: Some(true),
                node_offset: None,
                node_limit: Some(100),
            }),
        );
        assert_eq!(result.status, "ok");
        assert!(result.nodes.iter().all(|node| node.named));
        assert!(
            result
                .nodes
                .iter()
                .skip(1)
                .all(|node| node.parent_id.is_some())
        );
    }

    #[test]
    fn pages_cover_the_same_fixture() {
        let source = "function f(a) { return a + 1; }";
        let all = inspect_source(
            source,
            "x.ts",
            Some(SyntaxTreeInspectOptions {
                named_only: Some(false),
                node_offset: None,
                node_limit: Some(1000),
            }),
        );
        let mut ids = Vec::new();
        let mut offset = 0;
        loop {
            let page = inspect_source(
                source,
                "x.ts",
                Some(SyntaxTreeInspectOptions {
                    named_only: Some(false),
                    node_offset: Some(offset),
                    node_limit: Some(2),
                }),
            );
            ids.extend(page.nodes.into_iter().map(|node| node.id));
            match page.next_offset {
                Some(next) => offset = next,
                None => break,
            }
        }
        assert_eq!(ids.len(), all.total_nodes as usize);
        assert_eq!(ids, (0..all.total_nodes).collect::<Vec<_>>());
    }

    #[test]
    fn unsupported_extension_is_an_explicit_error() {
        let result = inspect_source("hello", "x.unknown", None);
        assert_eq!(result.status, "error");
        assert_eq!(
            result.diagnostics[0].code,
            "syntaxTree.language.unsupported"
        );
    }

    #[test]
    fn deeply_nested_input_uses_iterative_walk() {
        let source = format!("{}0{}", "[".repeat(2_000), "]".repeat(2_000));
        let result = inspect_source(
            &source,
            "x.ts",
            Some(SyntaxTreeInspectOptions {
                named_only: Some(false),
                node_offset: None,
                node_limit: Some(100_000),
            }),
        );
        assert!(matches!(result.status.as_str(), "ok" | "partial"));
        assert!(!result.nodes.is_empty());
    }

    #[test]
    fn recovered_files_still_paginate() {
        // One syntax error must not freeze paging at page 1: `partial` from
        // recovery is not a budget truncation.
        let source = "const a = 1;\nconst b = ;\nconst c = 3;\nconst d = 4;\n";
        let page = |offset| {
            inspect_source(
                source,
                "broken.ts",
                Some(SyntaxTreeInspectOptions {
                    named_only: Some(true),
                    node_offset: Some(offset),
                    node_limit: Some(3),
                }),
            )
        };
        let first = page(0);
        assert_eq!(first.status, "partial");
        assert_eq!(first.nodes.len(), 3);
        let next = first.next_offset.expect("continuation for recovered file");
        assert_eq!(next, 3);
        let second = page(next);
        assert_eq!(second.nodes[0].id, 3);
        assert_eq!(second.total_nodes, first.total_nodes);
        // Parent links still point at earlier preorder ids across pages.
        assert!(
            second
                .nodes
                .iter()
                .all(|node| node.parent_id.is_none_or(|p| p < node.id))
        );
    }
}
