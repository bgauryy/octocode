//! Generic tree-sitter graph facts.
//!
//! This is the language-neutral inventory lane used when the richer OXC JS/TS
//! graph extractor is not available. It deliberately emits syntax facts only:
//! declarations, imports, direct calls, containment, and language-public export
//! hints. LSP remains responsible for semantic identity and reference proof.

use serde::Serialize;
use tree_sitter::Node;

use crate::text::file_extension::get_extension_internal;

use super::languages;
use super::nodes::{
    call_callee, clean_specifier, compact_identifier, declaration, declaration_name,
    import_specifier, is_call_node, is_exported_declaration, is_import_node, is_name_leaf,
    last_name_leaf, node_text,
};

mod python;
mod rust;

use python::collect_python_imports;
use rust::{RustContext, collect_rust_imports, rust_child_contexts, rust_inner_unsupported};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphFacts {
    kind: &'static str,
    schema_version: u32,
    source: &'static str,
    language: String,
    file: String,
    declarations: Vec<GraphDeclaration>,
    imports: Vec<GraphImport>,
    exports: Vec<GraphExport>,
    calls: Vec<GraphCall>,
    edges: Vec<GraphEdge>,
    diagnostics: Vec<String>,
    modules: Vec<GraphRustModule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rust_root_unsupported: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphDeclaration {
    id: String,
    name: String,
    kind: &'static str,
    line: u32,
    range: Range,
    selection_range: Range,
    exported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<String>,
    /// 0-based first line of the comment block directly above.
    #[serde(skip_serializing_if = "Option::is_none")]
    doc_line: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphImport {
    id: String,
    specifier: String,
    line: u32,
    import_kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    imported_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    imported_range: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_range: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolution_hint: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    module_scope: Option<Vec<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphRustModule {
    name: String,
    line: u32,
    scope: Vec<String>,
    inline: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    unsupported: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphExport {
    id: String,
    name: String,
    line: u32,
    export_kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphCall {
    id: String,
    caller: String,
    /// Declaration id of the enclosing caller.
    caller_id: String,
    callee: String,
    line: u32,
    range: Range,
    kind: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphEdge {
    id: String,
    from: String,
    to: String,
    relation: &'static str,
    source: &'static str,
    line: u32,
    resolution: &'static str,
}

#[derive(Serialize)]
struct Position {
    line: u32,
    character: u32,
}

#[derive(Serialize)]
struct Range {
    start: Position,
    end: Position,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphFactCapability {
    extension: String,
    language: String,
    language_id: Option<String>,
    structural_search: bool,
    signature_outline: bool,
    graph_facts: bool,
    fact_families: Vec<&'static str>,
}

/// Thin wrapper over the shared `text::utf8_offsets::LineIndex` — see that
/// type for the actual line-start/UTF-16 counting logic.
struct LineIndex<'a>(crate::text::utf8_offsets::LineIndex<'a>);

impl<'a> LineIndex<'a> {
    fn new(content: &'a str) -> Self {
        Self(crate::text::utf8_offsets::LineIndex::new(content))
    }

    fn range(&self, node: Node<'_>) -> Range {
        Range {
            start: self.position(node.start_byte()),
            end: self.position(node.end_byte()),
        }
    }

    fn position(&self, byte_offset: usize) -> Position {
        let (line, character) = self.0.byte_to_position(byte_offset as u32);
        Position { line, character }
    }
}

struct GraphAccumulator {
    file_path: String,
    ext: String,
    declarations: Vec<GraphDeclaration>,
    imports: Vec<GraphImport>,
    exports: Vec<GraphExport>,
    calls: Vec<GraphCall>,
    edges: Vec<GraphEdge>,
    diagnostics: Vec<String>,
    modules: Vec<GraphRustModule>,
    /// Start bytes of name tokens that are not value references: declaration
    /// names and the callee tokens of recorded calls (call edges).
    non_reference_tokens: std::collections::HashSet<usize>,
    /// Bodies of Rust item-level macro calls (`cfg_rt! { mod x; }`) with
    /// their enclosing module scope, re-read as items after the main walk.
    macro_bodies: Vec<(tree_sitter::Range, Vec<String>)>,
}

impl GraphAccumulator {
    fn new(file_path: &str, ext: &str) -> Self {
        Self {
            file_path: file_path.to_owned(),
            ext: ext.to_owned(),
            declarations: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            calls: Vec::new(),
            edges: Vec::new(),
            modules: Vec::new(),
            non_reference_tokens: std::collections::HashSet::new(),
            macro_bodies: Vec::new(),
            diagnostics: vec![
                "tree-sitter graph facts are syntax-only; use LSP references/callHierarchy for semantic proof".to_owned(),
            ],
        }
    }
}

#[cfg(test)]
pub fn extract_graph_facts(content: &str, file_path: &str) -> Option<String> {
    extract_graph_facts_with_metadata(content, file_path)
        .and_then(|extraction| serde_json::to_string(&extraction.facts).ok())
}

pub(crate) fn extract_graph_facts_with_metadata(
    content: &str,
    file_path: &str,
) -> Option<super::GraphFactsExtraction> {
    let extension = get_extension_internal(file_path, true, "txt");
    extract_graph_facts_with_metadata_with_extension(content, file_path, &extension)
}

pub(crate) fn extract_graph_facts_with_metadata_with_extension(
    content: &str,
    file_path: &str,
    extension: &str,
) -> Option<super::GraphFactsExtraction> {
    if content.len() > crate::signatures::MAX_PARSE_SIZE {
        return None;
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        extract_graph_facts_with_metadata_inner(content, file_path, extension)
    }))
    .unwrap_or(None)
}

fn extract_graph_facts_with_metadata_inner(
    content: &str,
    file_path: &str,
    extension: &str,
) -> Option<super::GraphFactsExtraction> {
    extract_graph_facts_with_metadata_before(
        content,
        file_path,
        extension,
        std::time::Instant::now() + super::extractor::AST_EXECUTION_TIMEOUT,
    )
}

#[cfg(test)]
fn extract_graph_facts_before(
    content: &str,
    file_path: &str,
    deadline: std::time::Instant,
) -> Option<String> {
    let extension = get_extension_internal(file_path, true, "txt");
    extract_graph_facts_with_metadata_before(content, file_path, &extension, deadline)
        .and_then(|extraction| serde_json::to_string(&extraction.facts).ok())
}

fn extract_graph_facts_with_metadata_before(
    content: &str,
    file_path: &str,
    extension: &str,
    deadline: std::time::Instant,
) -> Option<super::GraphFactsExtraction> {
    let ext = extension.to_owned();
    if !graph_fact_extensions().iter().any(|item| item == &ext) {
        return None;
    }
    let entry = languages::find_entry(&ext)?;
    let mut acc = GraphAccumulator::new(file_path, &ext);
    let mut rust_root_unsupported = (ext == "rs").then_some(true);
    let mut reference_counts = Vec::new();
    if let Some(tree) = super::extractor::parse_before(content, &entry.language, deadline) {
        let root = tree.root_node();
        let line_index = LineIndex::new(content);
        rust_root_unsupported = (ext == "rs").then(|| rust_inner_unsupported(root, content));
        if rust_root_unsupported == Some(true) {
            acc.diagnostics
                .push("unsupported Rust inner conditional or custom crate attributes".to_owned());
        }
        if root.has_error() {
            acc.diagnostics.push(
                "tree-sitter recovered from parse errors; graph facts may be partial".to_owned(),
            );
        }
        if !visit_node(root, content, &line_index, &mut acc, deadline, &[])
            || !visit_macro_bodies(content, &entry.language, &line_index, &mut acc, deadline)
        {
            // Facts gathered before the deadline are positive syntax facts and
            // stay. The diagnostic marks the file incomplete, so consumers must
            // not read a missing import, call or module as absent.
            acc.diagnostics.push("graph.traversal.deadlineExceeded: graph extraction exceeded its execution deadline; facts are incomplete".to_owned());
            rust_root_unsupported = (ext == "rs").then_some(true);
        } else if let Some(counts) = count_name_references(root, content, &acc, deadline) {
            // A count cut short by the deadline would undercount; leaving it
            // out makes consumers treat every declaration as escaping.
            reference_counts = acc
                .declarations
                .iter()
                .map(|declaration| crate::types::GraphReferenceCount {
                    declaration_id: declaration.id.clone(),
                    count: counts.get(&declaration.name).copied().unwrap_or(0),
                })
                .collect();
        }
    } else {
        acc.diagnostics.push(
            "graph.parse.deadlineExceeded: graph parsing was interrupted; facts are incomplete"
                .to_owned(),
        );
    }

    let lines = content.lines().collect::<Vec<_>>();
    for declaration in &mut acc.declarations {
        declaration.doc_line =
            super::leading_doc_line(&lines, declaration.range.start.line as usize, &ext);
    }
    let facts = GraphFacts {
        kind: "graphFacts",
        schema_version: super::GRAPH_FACTS_SCHEMA_VERSION,
        source: "native-ast",
        language: language_label(&ext, entry.language_id),
        file: file_path.to_owned(),
        declarations: acc.declarations,
        imports: acc.imports,
        exports: acc.exports,
        calls: acc.calls,
        edges: acc.edges,
        diagnostics: acc.diagnostics,
        modules: acc.modules,
        rust_root_unsupported,
    };
    let facts_json = serde_json::to_string(&facts).ok()?;
    let facts = crate::graph::GraphFactsDocument::from_json(&facts_json).ok()?;
    Some(super::GraphFactsExtraction {
        facts,
        reference_counts,
    })
}

/// Per-name counts of identifier-kind tokens that reference a declared name,
/// skipping declaration names and call-edge callee tokens. Comments and string
/// contents are never identifier tokens, so they cannot count; Rust inline
/// format captures (`"{name}"` inside a macro) are the one string form that
/// names a binding and are counted. No scope resolution: equal names share a
/// count. `None` when the deadline cut the walk short.
fn count_name_references(
    root: Node<'_>,
    content: &str,
    acc: &GraphAccumulator,
    deadline: std::time::Instant,
) -> Option<std::collections::HashMap<String, u32>> {
    let mut counts: std::collections::HashMap<String, u32> = acc
        .declarations
        .iter()
        .map(|declaration| (declaration.name.clone(), 0))
        .collect();
    if counts.is_empty() {
        return Some(counts);
    }
    let rust = acc.ext == "rs";
    let mut pending = vec![root];
    let mut cursor = root.walk();
    while let Some(node) = pending.pop() {
        if std::time::Instant::now() >= deadline {
            return None;
        }
        if is_name_leaf(node) {
            if !acc.non_reference_tokens.contains(&node.start_byte())
                && let Some(count) = node_text(node, content).and_then(|text| counts.get_mut(text))
            {
                *count += 1;
            }
            continue;
        }
        if rust
            && node.kind() == "string_literal"
            && node
                .parent()
                .is_some_and(|parent| parent.kind() == "token_tree")
        {
            for name in node_text(node, content)
                .map(inline_format_captures)
                .unwrap_or_default()
            {
                if let Some(count) = counts.get_mut(name) {
                    *count += 1;
                }
            }
            continue;
        }
        pending.extend(node.named_children(&mut cursor));
    }
    Some(counts)
}

/// Identifiers captured by a Rust format string: `name` in `{name}` or
/// `{name:?}`; `{{` escapes and positional `{0}`/`{}` are skipped.
fn inline_format_captures(literal: &str) -> Vec<&str> {
    let mut names = Vec::new();
    let mut rest = literal;
    while let Some(open) = rest.find('{') {
        rest = &rest[open + 1..];
        if let Some(escaped) = rest.strip_prefix('{') {
            rest = escaped;
            continue;
        }
        let Some(end) = rest.find(['}', ':']) else {
            break;
        };
        let name = &rest[..end];
        if name.starts_with(|c: char| c.is_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
        {
            names.push(name);
        }
        rest = &rest[end..];
    }
    names
}

pub fn graph_fact_extensions() -> Vec<String> {
    let mut exts: Vec<String> = languages::signature_extensions()
        .into_iter()
        .map(str::to_owned)
        .collect();
    exts.sort();
    exts.dedup();
    exts
}

pub fn graph_fact_capabilities_json() -> String {
    let graph_exts = graph_fact_extensions();
    let capabilities: Vec<GraphFactCapability> = graph_exts
        .iter()
        .filter_map(|ext| {
            let entry = languages::find_entry(ext)?;
            Some(GraphFactCapability {
                extension: ext.clone(),
                language: language_label(ext, entry.language_id),
                language_id: entry.language_id.map(str::to_owned),
                structural_search: true,
                signature_outline: true,
                graph_facts: true,
                fact_families: fact_families_for_extension(ext),
            })
        })
        .collect();
    serde_json::to_string(&capabilities).unwrap_or_else(|_| "[]".to_owned())
}

/// A macro call in item position: directly in a file or module body, or as
/// an `expression_statement` there (`make_items!();`).
fn is_item_level(node: Node<'_>) -> bool {
    let item_parent = |node: Node<'_>| matches!(node.kind(), "source_file" | "declaration_list");
    node.parent().is_some_and(|parent| {
        item_parent(parent)
            || parent.kind() == "expression_statement" && parent.parent().is_some_and(item_parent)
    })
}

fn push_macro_gap(acc: &mut GraphAccumulator) {
    let message = "unsupported Rust macro expansion: macro-generated imports are not linked";
    if !acc
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic == message)
    {
        acc.diagnostics.push(message.to_owned());
    }
}

/// Read recorded Rust item-level macro bodies as items (nested bodies too,
/// bounded). A body that does not parse as items stays an explicit gap.
fn visit_macro_bodies(
    content: &str,
    language: &tree_sitter::Language,
    line_index: &LineIndex<'_>,
    acc: &mut GraphAccumulator,
    deadline: std::time::Instant,
) -> bool {
    const MAX_MACRO_BODIES: usize = 256;
    let mut visited = 0;
    while let Some((range, scope)) = acc.macro_bodies.pop() {
        visited += 1;
        let parsed = (visited <= MAX_MACRO_BODIES)
            .then(|| super::extractor::parse_ranges_before(content, language, &[range], deadline))
            .flatten();
        match parsed {
            Some(tree) if !tree.root_node().has_error() => {
                if !visit_node(tree.root_node(), content, line_index, acc, deadline, &scope) {
                    return false;
                }
            }
            _ => {
                if std::time::Instant::now() >= deadline {
                    return false;
                }
                push_macro_gap(acc);
            }
        }
    }
    true
}

fn visit_node(
    root: Node<'_>,
    content: &str,
    line_index: &LineIndex<'_>,
    acc: &mut GraphAccumulator,
    deadline: std::time::Instant,
    outer_scope: &[String],
) -> bool {
    enum Frame<'tree> {
        Enter(Node<'tree>, RustContext),
        ExitDeclaration,
        ExitModule,
    }

    let rust = acc.ext == "rs";
    let mut frames = vec![Frame::Enter(root, RustContext::default())];
    let mut declarations: Vec<(String, String)> = Vec::new();
    // Names of the enclosing `mod` items, outermost first.
    let mut module_scope: Vec<String> = outer_scope.to_vec();
    let mut children = Vec::new();
    let mut cursor = root.walk();
    while let Some(frame) = frames.pop() {
        if std::time::Instant::now() >= deadline {
            return false;
        }
        let (node, mut context) = match frame {
            Frame::Enter(node, context) => (node, context),
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
        }
        let active = declarations.last();
        let rust_node = rust.then_some(RustNodeContext {
            context: &context,
            module_scope: &module_scope,
        });
        if let Some(identity) = collect_node_facts(
            node,
            content,
            line_index,
            acc,
            active.map(|(id, _)| id.as_str()),
            active.map(|(_, name)| name.as_str()),
            rust_node,
            deadline,
        ) {
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
                    .map(|(child, context)| Frame::Enter(*child, context)),
            );
        } else {
            frames.extend(
                children
                    .iter()
                    .map(|child| Frame::Enter(*child, RustContext::default())),
            );
        }
        frames[children_start..].reverse();
    }
    std::time::Instant::now() < deadline
}

/// Rust traversal state for one node: its carried context and the names of
/// the `mod` items that enclose it (outermost first, the node itself excluded).
#[derive(Clone, Copy)]
struct RustNodeContext<'a> {
    context: &'a RustContext,
    module_scope: &'a [String],
}

/// A Go `import ( ... )` / `import "x"` declaration: its `import_spec`
/// children are the imports; the declaration itself would duplicate the
/// first one on the `import` line.
fn is_grouped_go_import(node: Node<'_>) -> bool {
    if node.kind() != "import_declaration" {
        return false;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|child| matches!(child.kind(), "import_spec" | "import_spec_list"))
}

/// Emits declaration/edge facts for `node`.
///
/// Note the intentional coordinate-basis split on every emitted `GraphDeclaration`:
/// `line` is 1-based (human-facing, computed as `range.start.line + 1`) while
/// `range`/`selection_range` are 0-based (`LineIndex` coordinates). Consumers must
/// not mix the two bases — use `range` for zero-based math and `line` for display.
fn collect_node_facts(
    node: Node<'_>,
    content: &str,
    line_index: &LineIndex<'_>,
    acc: &mut GraphAccumulator,
    active_decl: Option<&str>,
    active_name: Option<&str>,
    rust: Option<RustNodeContext<'_>>,
    deadline: std::time::Instant,
) -> Option<(String, String)> {
    // Only item-level macro calls can generate `mod`/`use` items; their
    // bodies are re-read as Rust items after the main walk.
    if acc.ext == "rs"
        && node.kind() == "macro_invocation"
        && is_item_level(node)
        && let Some(rust) = rust.as_ref()
    {
        let mut cursor = node.walk();
        let body = node
            .named_children(&mut cursor)
            .find(|child| child.kind() == "token_tree")
            .filter(|body| body.end_byte() > body.start_byte() + 2);
        if body.is_none() {
            // `make_imports!();` expands from its definition, which is not
            // visible here: the generated items stay an explicit gap.
            push_macro_gap(acc);
        }
        if let Some(body) = body {
            let inner = tree_sitter::Range {
                start_byte: body.start_byte() + 1,
                end_byte: body.end_byte() - 1,
                start_point: tree_sitter::Point {
                    row: body.start_position().row,
                    column: body.start_position().column + 1,
                },
                end_point: tree_sitter::Point {
                    row: body.end_position().row,
                    column: body.end_position().column.saturating_sub(1),
                },
            };
            acc.macro_bodies.push((inner, rust.module_scope.to_vec()));
        }
    }
    let decl = declaration(node, content).and_then(|(kind, name_token)| {
        node_text(name_token, content)
            .and_then(compact_identifier)
            .map(|name| {
                let range = line_index.range(node);
                let line = range.start.line + 1;
                // This identifies a declaration occurrence, not a canonical binding.
                // Location distinguishes overloads, impl blocks and equal names in scopes.
                let id = format!(
                    "declaration:{}#{}@{}:{}",
                    acc.file_path,
                    name,
                    node.start_byte(),
                    kind
                );
                let exported = is_exported_declaration(&acc.ext, node, content, &name, active_decl);
                let parent = active_decl.map(str::to_owned);
                acc.non_reference_tokens.insert(name_token.start_byte());
                GraphDeclaration {
                    id,
                    name,
                    kind,
                    line,
                    range,
                    selection_range: line_index.range(name_token),
                    exported,
                    parent,
                    doc_line: None,
                }
            })
    });

    // Keep the new declaration id alive for the entire child traversal so we
    // can pass it as &str without any per-child heap allocation.
    let next_decl_identity: Option<(String, String)> = if let Some(declaration) = decl {
        let id = declaration.id.clone();
        let name = declaration.name.clone();
        let line = declaration.line;
        let exported = declaration.exported;
        if let Some(parent) = &declaration.parent {
            acc.edges.push(GraphEdge {
                id: format!("{parent}->{id}:contains"),
                from: parent.clone(),
                to: id.clone(),
                relation: "contains",
                source: "ast",
                line,
                resolution: "syntactic",
            });
        }
        if exported {
            acc.exports.push(GraphExport {
                id: format!("export:{}:{}", name, line),
                name: name.clone(),
                line,
                export_kind: "language-public",
                local_name: Some(name.clone()),
                source: None,
            });
        }
        acc.declarations.push(declaration);
        Some((id, name))
    } else {
        None
    };
    // Inherit the parent scope when no new declaration was established.
    let next_decl = next_decl_identity
        .as_ref()
        .map(|(id, _)| id.as_str())
        .or(active_decl);
    let next_name = next_decl_identity
        .as_ref()
        .map(|(_, name)| name.as_str())
        .or(active_name);

    if let Some(rust) = rust
        && node.kind() == "use_declaration"
    {
        if let Some(argument) = node.child_by_field_name("argument") {
            collect_rust_imports(
                argument,
                rust.module_scope,
                content,
                line_index,
                acc,
                rust.context.unsupported,
                deadline,
            );
        }
    } else if let Some(rust) = rust
        && node.kind() == "mod_item"
    {
        if let Some(name) = declaration_name(node, content) {
            let line = line_index.range(node).start.line + 1;
            let path = rust.context.attributes.path.clone();
            let mut unsupported = rust.context.attributes.unsupported;
            unsupported |= rust.context.block_local;
            unsupported |= node
                .child_by_field_name("body")
                .is_some_and(|body| rust_inner_unsupported(body, content));
            let scope = rust.module_scope.to_vec();
            let inline = node.child_by_field_name("body").is_some();
            if unsupported {
                acc.diagnostics.push(format!(
                    "unsupported Rust conditional or custom module attributes at line {line}"
                ));
            }
            acc.modules.push(GraphRustModule {
                name: name.clone(),
                line,
                scope: scope.clone(),
                inline,
                path,
                unsupported,
            });
            if !inline {
                acc.imports.push(GraphImport {
                    id: format!("module:{}:{line}", name),
                    specifier: format!("self::{name}"),
                    line,
                    import_kind: "module",
                    local_name: Some(name.clone()),
                    imported_name: Some(name),
                    imported_range: None,
                    local_range: None,
                    resolution_hint: unsupported.then_some("unsupported"),
                    module_scope: Some(scope),
                });
            }
        }
    } else if matches!(acc.ext.as_str(), "py" | "pyi")
        && matches!(node.kind(), "import_statement" | "import_from_statement")
    {
        collect_python_imports(node, content, line_index, acc);
    } else if node.kind() == "preproc_include" {
        let path = node.child_by_field_name("path");
        let hint = match path.map(|item| item.kind()) {
            Some("string_literal") => "c-relative",
            Some("system_lib_string") => "c-system",
            _ => "unsupported",
        };
        if let Some(specifier) = path
            .and_then(|item| node_text(item, content))
            .and_then(clean_specifier)
        {
            push_language_import(
                acc,
                specifier,
                line_index.range(node).start.line + 1,
                "include",
                None,
                None,
                hint,
            );
        }
    } else if is_import_node(node.kind())
        && !is_grouped_go_import(node)
        && let Some(specifier) = import_specifier(node, content)
    {
        let line = line_index.range(node).start.line + 1;
        acc.imports.push(GraphImport {
            id: format!("import:{}:{}:{}", specifier, line, acc.imports.len()),
            specifier,
            line,
            import_kind: "value",
            local_name: None,
            imported_name: None,
            imported_range: None,
            local_range: None,
            resolution_hint: None,
            module_scope: None,
        });
    }

    if is_call_node(node.kind())
        && let (Some(caller), Some((callee, callee_node))) = (next_decl, call_callee(node, content))
    {
        let target = callee
            .rsplit(['.', ':'])
            .find(|segment| !segment.is_empty());
        if let Some(token) = target.and_then(|name| last_name_leaf(callee_node, content, name)) {
            acc.non_reference_tokens.insert(token.start_byte());
        }
        let range = line_index.range(node);
        let line = range.start.line + 1;
        let caller_name = next_name.unwrap_or(caller).to_owned();
        let id = format!("call:{}:{}:{}", caller_name, callee, acc.calls.len());
        acc.calls.push(GraphCall {
            id: id.clone(),
            caller: caller_name,
            caller_id: caller.to_owned(),
            callee: callee.to_owned(),
            line,
            range,
            kind: "calls",
        });
        acc.edges.push(GraphEdge {
            id: format!("{caller}->{callee}:calls:{line}:{}", acc.edges.len()),
            from: caller.to_owned(),
            to: format!(
                "reference:{}@{}:{}",
                acc.file_path,
                node.start_byte(),
                callee
            ),
            relation: "calls",
            source: "ast",
            line,
            resolution: "unresolved",
        });
    }

    next_decl_identity
}

fn push_language_import<'a>(
    acc: &'a mut GraphAccumulator,
    specifier: String,
    line: u32,
    import_kind: &'static str,
    local_name: Option<String>,
    imported_name: Option<String>,
    hint: &'static str,
) -> &'a mut GraphImport {
    let index = acc.imports.len();
    acc.imports.push(GraphImport {
        id: format!("import:{}:{}", line, acc.imports.len()),
        specifier,
        line,
        import_kind,
        local_name,
        imported_name,
        imported_range: None,
        local_range: None,
        resolution_hint: Some(hint),
        module_scope: None,
    });
    &mut acc.imports[index]
}

fn language_label(ext: &str, language_id: Option<&str>) -> String {
    language_id
        .unwrap_or_else(|| canonical_extension(ext))
        .to_owned()
}

fn canonical_extension(ext: &str) -> &str {
    languages::find_entry(ext).map_or(ext, |entry| entry.extensions[0])
}

fn fact_families_for_extension(ext: &str) -> Vec<&'static str> {
    if canonical_extension(ext) == "asm" {
        // The generic Assembly grammar provides reliable labels but no lexical
        // function scopes or dialect-neutral call semantics.
        return vec!["declarations"];
    }
    let mut families = vec!["declarations", "contains", "calls"];
    match canonical_extension(ext) {
        // JS/TS (oxc lane) already emit import/export facts — advertise them so
        // `getGraphFactCapabilities` matches what `extractGraphFacts` returns.
        "ts" | "tsx" | "js" | "rs" | "py" | "go" | "java" | "c" | "cpp" | "cu" | "scala" => {
            families.push("imports");
            families.push("exports");
        }
        _ => {}
    }
    families
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// Reference count of the first declaration named `name`.
    fn reference_count(source: &str, path: &str, name: &str) -> u32 {
        let extraction = extract_graph_facts_with_metadata(source, path).expect("graph facts");
        let id = &extraction
            .facts
            .declarations
            .iter()
            .find(|declaration| declaration.name == name)
            .unwrap_or_else(|| panic!("declaration {name}"))
            .id;
        extraction
            .reference_counts
            .iter()
            .find(|count| &count.declaration_id == id)
            .expect("counted")
            .count
    }

    #[test]
    fn rust_comments_and_strings_are_not_references() {
        let source = "/// `helper` is documented; see helper.\npub fn helper() {}\n// helper\n/* helper */\npub fn caller() -> &'static str { \"helper\" }\n";
        assert_eq!(reference_count(source, "lib.rs", "helper"), 0);
    }

    #[test]
    fn rust_value_uses_and_format_captures_count_but_calls_do_not() {
        let source = "pub fn callback() {}\npub fn called() {}\npub const LIMIT: u32 = 1;\npub struct Svc;\nimpl Svc { pub fn run(&self) {} pub fn go(&self) { self.run(); called(); register(callback); println!(\"{LIMIT} {{LIMIT}}\"); } }\n";
        assert_eq!(reference_count(source, "lib.rs", "callback"), 1);
        assert_eq!(reference_count(source, "lib.rs", "called"), 0);
        assert_eq!(reference_count(source, "lib.rs", "run"), 0);
        assert_eq!(reference_count(source, "lib.rs", "LIMIT"), 1);
        assert_eq!(
            reference_count(source, "lib.rs", "Svc"),
            0,
            "the impl names the struct as a declaration, not a use"
        );
    }

    #[test]
    fn inline_format_captures_skip_escapes_and_positional_arguments() {
        assert_eq!(
            inline_format_captures("\"{a} {b:?} {{c}} {0} {} {_d:>4}\""),
            ["a", "b", "_d"]
        );
    }

    #[test]
    fn rust_import_ranges_do_not_invent_synthetic_name_tokens() {
        let value = facts(
            "use crate::origin::{self as module_alias, *};\nuse crate::origin::plain;\n",
            "imports.rs",
        );
        let imports = value["imports"].as_array().unwrap();
        assert!(imports[0].get("importedRange").is_none());
        assert!(imports[0].get("localRange").is_some());
        assert!(imports[1].get("importedRange").is_none());
        assert!(imports[1].get("localRange").is_none());
        assert_eq!(imports[2]["localRange"], imports[2]["importedRange"]);
        assert!(imports[2].get("importedRange").is_some());
    }

    #[test]
    fn rust_import_binding_ranges_are_exact_utf16() {
        let value = facts(
            "/*😀*/ use crate::origin::{target as first, target as second};\nuse crate::origin::{\n target as third,\n};\nuse crate::origin::target as fourth;\n",
            "aliases.rs",
        );
        let imports = value["imports"].as_array().unwrap();
        assert_eq!(imports.len(), 4);
        for (index, (line, imported, local, length)) in [
            (0, 27, 37, 5),
            (0, 44, 54, 6),
            (2, 1, 11, 5),
            (4, 19, 29, 6),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                imports[index]["importedRange"],
                serde_json::json!({"start":{"line":line,"character":imported},"end":{"line":line,"character":imported+6}})
            );
            assert_eq!(
                imports[index]["localRange"],
                serde_json::json!({"start":{"line":line,"character":local},"end":{"line":line,"character":local+length}})
            );
        }
    }

    #[test]
    fn python_import_ranges_do_not_invent_synthetic_name_tokens() {
        let value = facts(
            "import package.sub as alias\nfrom origin import plain\nfrom origin import *\n",
            "imports.py",
        );
        let imports = value["imports"].as_array().unwrap();
        assert!(imports[0].get("importedRange").is_none());
        assert_eq!(
            imports[0]["localRange"],
            serde_json::json!({"start":{"line":0,"character":22},"end":{"line":0,"character":27}})
        );
        assert_eq!(imports[1]["localRange"], imports[1]["importedRange"]);
        assert!(imports[2].get("importedRange").is_none());
        assert!(imports[2].get("localRange").is_none());
    }

    #[test]
    fn python_import_binding_ranges_are_exact_utf16() {
        let value = facts(
            "marker = \"😀\"; from origin import target as first, target as second\nfrom origin import (\n    target as third,\n)\n",
            "aliases.py",
        );
        let imports = value["imports"].as_array().unwrap();
        assert_eq!(imports.len(), 3);
        for (index, (line, imported, local, length)) in
            [(0, 34, 44, 5), (0, 51, 61, 6), (2, 4, 14, 5)]
                .into_iter()
                .enumerate()
        {
            assert_eq!(
                imports[index]["importedRange"],
                serde_json::json!({"start":{"line":line,"character":imported},"end":{"line":line,"character":imported+6}})
            );
            assert_eq!(
                imports[index]["localRange"],
                serde_json::json!({"start":{"line":line,"character":local},"end":{"line":line,"character":local+length}})
            );
        }
    }

    #[test]
    fn rust_cfg_and_path_attributes_survive_comments_and_inner_attributes() {
        let value = facts(
            "#[cfg(feature = \"x\")]\n// note\nmod child;\n#[path = \"actual.rs\"]\n/// documented\nmod alias;\nmod gated { #![cfg(feature = \"x\")] use super::Thing; }",
            "src/lib.rs",
        );
        assert!(
            value["modules"]
                .as_array()
                .unwrap()
                .iter()
                .any(|module| module["name"] == "child" && module["unsupported"] != true)
        );
        assert!(
            value["modules"]
                .as_array()
                .unwrap()
                .iter()
                .any(|module| module["name"] == "alias" && module["path"] == "actual.rs")
        );
        // `cfg` gates compilation, not the module's file: the edge stays.
        assert!(
            value["modules"]
                .as_array()
                .unwrap()
                .iter()
                .any(|module| module["name"] == "gated" && module["unsupported"] != true)
        );
        let root = facts("#![cfg(feature = \"x\")]\nmod child;", "src/lib.rs");
        assert_ne!(root["rustRootUnsupported"], true);
        // A conditional attribute that rewrites the path is still unknown.
        let rewritten = facts(
            "#[cfg_attr(unix, path = \"u.rs\")] mod child;",
            "src/lib.rs",
        );
        assert!(
            rewritten["modules"]
                .as_array()
                .unwrap()
                .iter()
                .all(|module| module["unsupported"] == true)
        );
        let local = facts(
            "fn f() { mod hidden { #[path = \"child.rs\"] mod child; } }",
            "src/lib.rs",
        );
        assert!(
            local["modules"]
                .as_array()
                .unwrap()
                .iter()
                .all(|module| module["unsupported"] == true)
        );
    }

    #[test]
    fn rust_modules_preserve_literal_paths_inline_scopes_and_unknown_cfg() {
        let value = facts(
            "#[path = \"actual.rs\"] mod alias;\nmod inside { use super::Thing; #[path = \"nested.rs\"] mod child; }\n#[cfg(feature = \"x\")] mod conditional;",
            "src/lib.rs",
        );
        let modules = value["modules"].as_array().unwrap();
        assert!(modules.iter().any(|module| module["name"] == "alias"
            && module["path"] == "actual.rs"
            && module["unsupported"] != true));
        assert!(modules.iter().any(|module| module["name"] == "child"
            && module["scope"] == serde_json::json!(["inside"])
            && module["path"] == "nested.rs"));
        assert!(
            modules
                .iter()
                .any(|module| module["name"] == "conditional" && module["unsupported"] != true)
        );
        assert!(
            value["imports"]
                .as_array()
                .unwrap()
                .iter()
                .any(|import| import["specifier"] == "super::Thing"
                    && import["moduleScope"] == serde_json::json!(["inside"])
                    && import["resolutionHint"].is_null())
        );
    }

    #[test]
    fn declaration_occurrences_do_not_alias_equal_names_or_impl_blocks() {
        for (source, path) in [
            (
                "struct A; impl A { fn run() { work(); } } struct B; impl B { fn run() { work(); } }",
                "names.rs",
            ),
            (
                "class A:\n def run(self): work()\nclass B:\n def run(self): work()\n",
                "names.py",
            ),
        ] {
            let value = facts(source, path);
            let declarations = value["declarations"].as_array().unwrap();
            let ids: std::collections::HashSet<_> = declarations
                .iter()
                .map(|d| d["id"].as_str().unwrap())
                .collect();
            assert_eq!(ids.len(), declarations.len());
            for edge in value["edges"].as_array().unwrap() {
                if edge["relation"] == "calls" {
                    assert!(ids.contains(edge["from"].as_str().unwrap()));
                    assert!(edge["to"].as_str().unwrap().starts_with("reference:"));
                    assert_eq!(edge["resolution"], "unresolved");
                }
            }
        }
    }

    #[test]
    fn rust_use_trees_expand_multiline_groups_aliases_and_modules() {
        let value = facts(
            "mod child;\nuse super::{\n types::{Thing as Alias, Other},\n language::AgLanguage,\n};\n",
            "src/structural/files.rs",
        );
        let imports = value["imports"].as_array().unwrap();
        for expected in [
            "self::child",
            "super::types::Thing",
            "super::types::Other",
            "super::language::AgLanguage",
        ] {
            assert!(
                imports.iter().any(|i| i["specifier"] == expected),
                "missing {expected}: {imports:?}"
            );
        }
        assert!(
            imports
                .iter()
                .any(|i| i["localName"] == "Alias" && i["importedName"] == "Thing")
        );
    }

    #[test]
    fn use_heavy_rust_file_keeps_its_imports_inside_the_deadline() {
        // Per-`use` `parent()`/sibling walks made this O(n²) and the
        // walk hit the deadline, discarding every import.
        let mut source = String::from("mod item0 { pub struct Name; }\n");
        for index in 0..6_000 {
            source.push_str(&format!("use crate::item{index}::Name;\n"));
        }
        let started = std::time::Instant::now();
        let value = facts(&source, "src/lib.rs");
        let elapsed = started.elapsed();
        let diagnostics = value["diagnostics"].as_array().unwrap();
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.as_str().unwrap().contains("deadlineExceeded")),
            "{diagnostics:?}"
        );
        let imports = value["imports"].as_array().unwrap();
        assert_eq!(imports.len(), 6_000);
        assert_eq!(imports[5_999]["specifier"], "crate::item5999::Name");
        assert_eq!(imports[0]["moduleScope"], serde_json::json!([]));
        assert!(imports.iter().all(|i| i.get("resolutionHint").is_none()));
        assert!(
            elapsed < super::super::extractor::AST_EXECUTION_TIMEOUT / 2,
            "took {elapsed:?}"
        );
    }

    #[test]
    fn rust_item_macro_bodies_are_read_as_items_and_expression_macros_are_not_gaps() {
        // tokio-style `cfg_rt! { ... }` wrappers hold real mod/use items.
        let value = facts(
            "cfg_rt! {\n    pub mod runtime;\n    pub use crate::runtime::Handle;\n}\nmod outer { cfg_net! { mod tcp; } }\nfn f() { println!(\"{}\", 1); let v = vec![1]; }\n",
            "src/lib.rs",
        );
        let modules = value["modules"].as_array().unwrap();
        assert!(
            modules
                .iter()
                .any(|m| m["name"] == "runtime" && m["line"] == 2),
            "{modules:?}"
        );
        assert!(
            modules
                .iter()
                .any(|m| m["name"] == "tcp" && m["scope"] == serde_json::json!(["outer"])),
            "{modules:?}"
        );
        assert!(
            value["imports"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["specifier"] == "crate::runtime::Handle")
        );
        assert!(
            value["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .all(|d| !d.as_str().unwrap().contains("macro expansion")),
            "{}",
            value["diagnostics"]
        );
    }

    #[test]
    fn rust_nonconventional_modules_and_macros_remain_explicitly_unsupported() {
        let value = facts(
            "#[cfg_attr(unix, path = \"other.rs\")] mod child;\nmod inline { use super::Thing; }\nmake_imports!();",
            "src/lib.rs",
        );
        let imports = value["imports"].as_array().unwrap();
        assert!(
            imports.iter().any(|item| item["specifier"] == "self::child"
                && item["resolutionHint"] == "unsupported")
        );
        assert!(
            imports
                .iter()
                .any(|item| item["specifier"] == "super::Thing"
                    && item["moduleScope"] == serde_json::json!(["inline"]))
        );
        assert!(
            value["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item.as_str().unwrap().contains("macro expansion"))
        );
    }

    #[cfg(feature = "tree-sitter-cpp")]
    #[test]
    fn cpp_class_owns_its_method_declarations() {
        let value = facts(
            "class Fixture { public: int target(int value) { return value; } };",
            "fixture.cpp",
        );
        let declarations = value["declarations"].as_array().unwrap();
        let class = declarations
            .iter()
            .find(|item| item["name"] == "Fixture")
            .expect("class declaration");
        let method = declarations
            .iter()
            .find(|item| item["name"] == "target")
            .expect("method declaration");
        assert_eq!(class["kind"], "class");
        assert_eq!(method["parent"], class["id"]);
        assert!(
            value["edges"]
                .as_array()
                .unwrap()
                .iter()
                .any(|edge| edge["relation"] == "contains"
                    && edge["from"] == class["id"]
                    && edge["to"] == method["id"])
        );
    }

    fn facts(src: &str, path: &str) -> Value {
        let raw = extract_graph_facts(src, path).expect("graph facts expected");
        serde_json::from_str(&raw).expect("valid graph json")
    }

    #[test]
    fn expired_graph_budget_reports_incomplete_rust_root() {
        let raw = extract_graph_facts_before(
            "mod child; fn main() { work(); }",
            "lib.rs",
            std::time::Instant::now(),
        )
        .unwrap();
        let graph: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(graph["rustRootUnsupported"], true);
        assert_eq!(graph["imports"], serde_json::json!([]));
        assert!(
            graph["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d.as_str().unwrap().contains("graph.parse.deadlineExceeded"))
        );
    }

    #[test]
    fn expired_graph_walk_does_not_emit_complete_facts() {
        let source = "fn main() { work(); }";
        let language = tree_sitter_rust::LANGUAGE.into();
        let tree = super::super::extractor::parse_before(
            source,
            &language,
            std::time::Instant::now() + super::super::extractor::AST_EXECUTION_TIMEOUT,
        )
        .unwrap();
        let index = LineIndex::new(source);
        let mut acc = GraphAccumulator::new("main.rs", "rs");
        assert!(!visit_node(
            tree.root_node(),
            source,
            &index,
            &mut acc,
            std::time::Instant::now(),
            &[]
        ));
        assert!(acc.declarations.is_empty());
        assert!(acc.calls.is_empty());
    }

    #[test]
    fn deeply_nested_rust_use_groups_do_not_recurse_on_the_native_stack() {
        let source = format!("use {}std{};", "{".repeat(10_000), "}".repeat(10_000));
        let graph = facts(&source, "imports.rs");
        assert_eq!(graph["imports"].as_array().unwrap().len(), 1);
        assert_eq!(graph["imports"][0]["specifier"], "std");
        assert!(
            !graph["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d.as_str().unwrap().contains("parse errors"))
        );
    }

    #[test]
    fn deeply_nested_graph_traversal_preserves_calls() {
        let source = format!(
            "fn main() {{ let x = {}probe(){}; after(); }}",
            "[".repeat(10_000),
            "]".repeat(10_000)
        );
        let graph = facts(&source, "deep.rs");
        assert_eq!(
            graph["diagnostics"],
            serde_json::json!([
                "tree-sitter graph facts are syntax-only; use LSP references/callHierarchy for semantic proof"
            ])
        );
        let calls = graph["calls"].as_array().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["caller"], "main");
        assert_eq!(calls[0]["callee"], "probe");
        assert_eq!(calls[1]["caller"], "main");
        assert_eq!(calls[1]["callee"], "after");
    }

    #[test]
    fn graph_traversal_restores_declaration_context_after_nested_scopes() {
        let graph = facts(
            "fn outer() { fn inner() { inside(); } after(); } fn sibling() { next(); }",
            "scopes.rs",
        );
        let calls: Vec<_> = graph["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|call| {
                (
                    call["caller"].as_str().unwrap(),
                    call["callee"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            calls,
            [("inner", "inside"), ("outer", "after"), ("sibling", "next")]
        );
        let declarations = graph["declarations"].as_array().unwrap();
        assert_eq!(declarations[0]["name"], "outer");
        assert_eq!(declarations[1]["name"], "inner");
        assert_eq!(declarations[1]["parent"], declarations[0]["id"]);
        assert_eq!(declarations[2]["name"], "sibling");
        assert!(declarations[2]["parent"].is_null());
    }

    #[test]
    fn rust_graph_facts_include_pub_declarations_and_calls() {
        let src = r#"
use crate::other::helper;

pub struct Point {
    x: f64,
}

pub fn distance(point: Point) -> f64 {
    helper(point.x)
}
"#;
        let graph = facts(src, "geo.rs");
        assert_eq!(graph["schemaVersion"], 1);
        assert!(
            graph
                .get("language")
                .is_some_and(|language| language == "rust")
        );
        assert!(
            graph
                .get("declarations")
                .and_then(Value::as_array)
                .is_some_and(|decls| decls
                    .iter()
                    .any(|decl| decl.get("name").is_some_and(|name| name == "Point")))
        );
        assert!(
            graph
                .get("declarations")
                .and_then(Value::as_array)
                .is_some_and(|decls| decls.iter().any(|decl| decl
                    .get("name")
                    .is_some_and(|name| name == "distance")
                    && decl
                        .get("exported")
                        .is_some_and(|exported| exported == true)))
        );
        assert!(
            graph
                .get("imports")
                .and_then(Value::as_array)
                .is_some_and(|imports| imports.iter().any(|import| import
                    .get("specifier")
                    .and_then(Value::as_str)
                    .is_some_and(|specifier| specifier.contains("crate::other"))))
        );
        assert!(
            graph
                .get("calls")
                .and_then(Value::as_array)
                .is_some_and(|calls| calls
                    .iter()
                    .any(|call| call.get("callee").is_some_and(|callee| callee == "helper")))
        );
    }

    #[test]
    fn python_import_facts_preserve_module_paths_and_alias_bindings() {
        let graph = facts(
            "import os, pkg.worker as worker\nfrom .target import run as execute\nfrom . import sibling\n",
            "pkg/service.py",
        );
        let imports = graph["imports"].as_array().unwrap();
        assert_eq!(imports.len(), 4);
        assert_eq!(imports[0]["specifier"], "os");
        assert_eq!(imports[1]["specifier"], "pkg.worker");
        assert_eq!(imports[1]["localName"], "worker");
        assert_eq!(imports[1]["resolutionHint"], "python-absolute");
        assert_eq!(imports[2]["specifier"], ".target");
        assert_eq!(imports[2]["importedName"], "run");
        assert_eq!(imports[2]["localName"], "execute");
        assert_eq!(imports[2]["resolutionHint"], "python-relative");
        assert_eq!(imports[3]["specifier"], ".");
        assert_eq!(imports[3]["importedName"], "sibling");
    }

    #[test]
    fn c_import_facts_distinguish_quoted_system_and_computed_headers() {
        let graph = facts(
            "#include \"local.h\"\n#include <system.h>\n#include HEADER\n",
            "entry.c",
        );
        let imports = graph["imports"].as_array().unwrap();
        assert_eq!(imports.len(), 3);
        assert_eq!(imports[0]["specifier"], "local.h");
        assert_eq!(imports[0]["resolutionHint"], "c-relative");
        assert_eq!(imports[1]["resolutionHint"], "c-system");
        assert_eq!(imports[2]["resolutionHint"], "unsupported");
    }

    #[test]
    fn python_graph_facts_include_module_public_defs() {
        let src = r#"
import os

class Service:
    def run(self):
        helper()

def helper():
    return os.getcwd()
"#;
        let graph = facts(src, "service.py");
        assert!(
            graph
                .get("language")
                .is_some_and(|language| language == "python")
        );
        assert!(
            graph
                .get("declarations")
                .and_then(Value::as_array)
                .is_some_and(|decls| decls
                    .iter()
                    .any(|decl| decl.get("name").is_some_and(|name| name == "Service")))
        );
        assert!(
            graph
                .get("declarations")
                .and_then(Value::as_array)
                .is_some_and(|decls| decls.iter().any(|decl| decl
                    .get("name")
                    .is_some_and(|name| name == "helper")
                    && decl
                        .get("exported")
                        .is_some_and(|exported| exported == true)))
        );
        assert!(
            graph
                .get("imports")
                .and_then(Value::as_array)
                .is_some_and(|imports| imports.iter().any(|import| import
                    .get("specifier")
                    .and_then(Value::as_str)
                    .is_some_and(|specifier| specifier.contains("os"))))
        );
        assert!(
            graph
                .get("calls")
                .and_then(Value::as_array)
                .is_some_and(|calls| calls
                    .iter()
                    .any(|call| call.get("callee").is_some_and(|callee| callee == "helper")))
        );
    }

    #[test]
    fn graph_fact_capabilities_include_rust_and_python() {
        let json = graph_fact_capabilities_json();
        assert!(json.contains("\"extension\":\"rs\""));
        assert!(json.contains("\"extension\":\"py\""));
    }
}
