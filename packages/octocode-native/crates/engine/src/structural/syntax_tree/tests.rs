use super::*;

fn inspect_source(
    source: &str,
    path: &str,
    options: Option<SyntaxTreeInspectOptions>,
) -> SyntaxTreeInspectResult {
    inspect_with_extension(source, path, None, options)
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

/// Each node names the grammar field it fills in its parent, the name a
/// rule's `has`/`inside` `field` matches; the root and unnamed children
/// have none.
#[test]
fn nodes_name_the_field_they_fill() {
    let result = inspect_source("fn run(a: u8) {}\n", "x.rs", None);
    let field_of = |kind: &str| {
        result
            .nodes
            .iter()
            .find(|node| node.kind == kind)
            .map(|node| node.field.as_deref())
    };
    assert_eq!(field_of("source_file"), Some(None));
    assert_eq!(field_of("function_item"), Some(None));
    assert_eq!(field_of("identifier"), Some(Some("name")));
    assert_eq!(field_of("parameters"), Some(Some("parameters")));
    assert_eq!(field_of("block"), Some(Some("body")));
}

/// `line` roots the walk at the smallest node covering that line's
/// text: a statement inside a body, or the declaration its header line
/// opens; a line past the end is an options error.
#[test]
fn a_line_roots_the_tree_at_the_node_covering_it() {
    let source = "const a = 1;\nfunction f(x: number) {\n  return x + a;\n}\n";
    let at = |line| {
        inspect_source(
            source,
            "x.ts",
            Some(SyntaxTreeInspectOptions {
                named_only: Some(true),
                line: Some(line),
                ..Default::default()
            }),
        )
    };
    let body = at(3);
    assert_eq!(body.status, "ok");
    assert_eq!(body.nodes[0].kind, "return_statement");
    assert_eq!(body.nodes[0].parent_id, None);
    assert!(body.nodes.iter().all(|node| node.start_line == 3));
    let header = at(2);
    assert_eq!(header.nodes[0].kind, "function_declaration");
    assert_eq!(header.nodes[0].end_line, 4);
    let past = at(9);
    assert_eq!(past.status, "error");
    assert_eq!(
        past.diagnostics[0].code,
        "syntaxTree.options.lineOutOfRange"
    );
}

/// A blank line covers no text: the tree starts at the next item inside
/// the node enclosing the line (else the item before it), and a warning
/// names that node instead of silently rooting at the file.
#[test]
fn a_blank_line_roots_at_the_nearest_item_and_says_so() {
    let source =
        "const a = 1;\n\n// doc\nfunction f(x: number) {\n  const y = x;\n\n  return y;\n}\n\n";
    let at = |line| {
        inspect_source(
            source,
            "x.ts",
            Some(SyntaxTreeInspectOptions {
                named_only: Some(true),
                line: Some(line),
                ..Default::default()
            }),
        )
    };
    let between = at(2);
    assert_eq!(between.status, "ok");
    assert_eq!(between.nodes[0].kind, "function_declaration");
    assert_eq!(between.nodes[0].start_line, 4);
    let note = &between.diagnostics[0];
    assert_eq!(note.code, "syntaxTree.options.lineBlank");
    assert_eq!(note.severity, "warning");
    assert!(note.message.contains("line 2 is blank"), "{}", note.message);
    let in_body = at(6);
    assert_eq!(in_body.nodes[0].kind, "return_statement");
    let trailing = at(9);
    assert_eq!(trailing.nodes[0].kind, "function_declaration");
    assert!(
        trailing.diagnostics[0]
            .message
            .contains("the item before it")
    );
}

/// Attributes are siblings of the Rust item they annotate: a blank line
/// before them roots at the item, not at its first attribute.
#[test]
fn a_blank_line_before_attributes_roots_at_the_annotated_item() {
    let source = "use std::fmt;\n\n/// Doc.\n#[derive(Clone, Copy, Debug)]\n#[non_exhaustive]\npub enum MatchQuery {\n    A,\n}\n";
    let result = inspect_source(
        source,
        "x.rs",
        Some(SyntaxTreeInspectOptions {
            named_only: Some(true),
            line: Some(2),
            ..Default::default()
        }),
    );
    assert_eq!(result.status, "ok");
    assert_eq!(result.nodes[0].kind, "enum_item");
    assert_eq!(result.nodes[0].start_line, 6);
    assert!(
        result.diagnostics[0]
            .message
            .contains("enum_item at line 6"),
        "{}",
        result.diagnostics[0].message
    );
}

#[test]
fn named_only_reparents_around_punctuation() {
    let result = inspect_source(
        "foo(1);",
        "x.ts",
        Some(SyntaxTreeInspectOptions {
            named_only: Some(true),
            node_offset: None,
            line: None,
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
            line: None,
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
                line: None,
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

/// A tree of more than a million nodes is walked whole: its last nodes
/// page like its first, and no node budget ends the listing.
#[test]
fn a_tree_past_a_million_nodes_pages_to_its_last_node() {
    let source = format!("[{}0]", "0,".repeat(600_000));
    let tail = inspect_source(
        &source,
        "big.js",
        Some(SyntaxTreeInspectOptions {
            named_only: Some(false),
            node_offset: Some(1_100_000),
            line: None,
            node_limit: Some(10),
        }),
    );
    assert_eq!(tail.status, "ok");
    assert!(tail.total_nodes > 1_100_010, "{}", tail.total_nodes);
    assert_eq!(tail.nodes.len(), 10);
    assert_eq!(tail.nodes[0].id, 1_100_000);
    assert!(
        tail.diagnostics
            .iter()
            .all(|d| !d.code.starts_with("syntaxTree.budget."))
    );
}

/// A diagnostic names a recovery only when it has one that fits it: a
/// failed or recovered parse is not fixed by paging.
#[test]
fn parse_diagnostics_carry_no_paging_recovery() {
    let unsupported = inspect_source("hello", "x.unknown", None);
    let recovered = inspect_source("const b = ;\n", "broken.ts", None);
    let diagnostics = unsupported.diagnostics.iter().chain(&recovered.diagnostics);
    assert_eq!(diagnostics.clone().count(), 2);
    for diagnostic in diagnostics {
        assert!(
            diagnostic.recovery.is_none(),
            "{}: {:?}",
            diagnostic.code,
            diagnostic.recovery
        );
    }
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
            line: None,
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
                line: None,
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
