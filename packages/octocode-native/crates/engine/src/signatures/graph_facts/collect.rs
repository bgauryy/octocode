//! Declaration and call facts of one node.

use super::imports::{
    collect_c_include, collect_csharp_using, collect_rust_include, collect_rust_module,
    collect_rust_use,
};
use super::positions::NodePositions;
use super::python::collect_python_imports;
use super::walk::{
    RustNodeContext, is_grouped_go_import, is_item_level, method_kind, push_macro_gap,
};
use super::{GraphAccumulator, heritage};
use tree_sitter::Node;

use crate::graph::{
    GraphFactCall, GraphFactDeclaration, GraphFactEdge, GraphFactExport, GraphFactImport,
};

use crate::signatures::nodes::{
    call_callee, compact_identifier, declaration, declaration_extent, import_specifier,
    is_call_node, is_exported_declaration, is_import_node, last_name_leaf, node_text,
};
use crate::text::{JS_TS_EXTENSIONS, extension_of};

/// Caller label of calls outside any declaration (matches the OXC lane).
const MODULE_CALLER: &str = "module";

/// Statement keywords that error recovery turns into declaration names:
/// `if (x) { .. }` after a syntax error reads as a method `if() {}`.
const STATEMENT_KEYWORDS: &[&str] = &[
    "if", "else", "for", "while", "do", "switch", "case", "catch", "try", "finally", "return",
    "throw", "new", "typeof", "function", "var", "let", "const", "with",
];

/// Grammars whose error recovery reads a statement as a declaration. In every
/// other language these words are ordinary names (`fn new`, `def new`,
/// `def case`), so filtering them silently drops real declarations.
fn recovers_statements_as_declarations(file_path: &str) -> bool {
    let extension = extension_of(file_path, true, "");
    JS_TS_EXTENSIONS.contains(&extension.as_str())
        || matches!(
            extension.as_str(),
            "vue" | "svelte" | "astro" | "html" | "htm"
        )
}

/// In a JS/TS-family file a keyword-named declaration is a recovery artifact
/// unless it is a class member (`class A { if() {} }` is legal) parsed without
/// errors around it. Membership comes from the raw node kind: the normalized
/// declaration kind collapses methods into `function`.
fn is_recovered_keyword_declaration(node: Node<'_>, name: &str, file_path: &str) -> bool {
    if !STATEMENT_KEYWORDS.contains(&name) || !recovers_statements_as_declarations(file_path) {
        return false;
    }
    let member = matches!(
        node.kind(),
        "method_definition"
            | "method_signature"
            | "abstract_method_signature"
            | "public_field_definition"
            | "property_signature"
    );
    !member || node.has_error() || node.parent().is_some_and(|parent| parent.is_error())
}

/// Emits declaration/edge facts for `node`.
///
/// Note the intentional coordinate-basis split on every emitted `GraphFactDeclaration`:
/// `line` is 1-based (human-facing, computed as `range.start.line + 1`) while
/// `range`/`selection_range` are 0-based (`NodePositions` coordinates). Consumers must
/// not mix the two bases — use `range` for zero-based math and `line` for display.
pub(super) fn collect_node_facts(
    node: Node<'_>,
    content: &str,
    line_index: &NodePositions<'_>,
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
        record_macro_body(node, acc, active_decl, active_name, rust);
    }
    let decl = declaration_fact(node, content, line_index, acc, active_decl);

    // Keep the new declaration id alive for the entire child traversal so we
    // can pass it as &str without any per-child heap allocation.
    let next_decl_identity: Option<(String, String)> =
        decl.map(|declaration| push_declaration(node, content, line_index, acc, declaration));
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
        collect_rust_use(node, content, line_index, acc, rust, deadline);
    } else if let Some(rust) = rust
        && node.kind() == "mod_item"
    {
        collect_rust_module(node, content, line_index, acc, rust);
    } else if rust.is_some() && node.kind() == "macro_invocation" {
        collect_rust_include(node, content, line_index, acc);
    } else if acc.ext == "py" && matches!(node.kind(), "import_statement" | "import_from_statement")
    {
        collect_python_imports(node, content, line_index, acc);
    } else if node.kind() == "preproc_include" {
        collect_c_include(node, content, line_index, acc);
    } else if acc.ext == "cs" && node.kind() == "using_directive" {
        collect_csharp_using(node, content, line_index, acc);
    } else if is_import_node(node.kind())
        && !is_grouped_go_import(node)
        && let Some(specifier) = import_specifier(node, content)
    {
        let line = line_index.range(node).start.line + 1;
        acc.imports.push(GraphFactImport {
            id: format!("import:{}:{}:{}", specifier, line, acc.imports.len()),
            specifier,
            line,
            import_kind: "value".to_owned(),
            local_name: None,
            imported_name: None,
            imported_range: None,
            local_range: None,
            resolution_hint: None,
            module_scope: None,
        });
    }

    if !acc.declarations_only && is_call_node(node.kind()) {
        collect_call(node, content, line_index, acc, next_decl, next_name);
    }

    next_decl_identity
}

/// Record a Rust item-level macro call's body to re-read as items, or an
/// explicit gap when it has no body.
fn record_macro_body(
    node: Node<'_>,
    acc: &mut GraphAccumulator,
    active_decl: Option<&str>,
    active_name: Option<&str>,
    rust: &RustNodeContext<'_>,
) {
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
        let container = active_decl
            .zip(active_name)
            .map(|(id, name)| (id.to_owned(), name.to_owned()));
        acc.macro_bodies
            .push((inner, rust.module_scope.to_vec(), container));
    }
}

/// The declaration `node` introduces, if any; marks its name token as a
/// non-reference.
fn declaration_fact(
    node: Node<'_>,
    content: &str,
    line_index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    active_decl: Option<&str>,
) -> Option<GraphFactDeclaration> {
    declaration(node, content).and_then(|(kind, name_token)| {
        let kind = method_kind(kind, node, active_decl);
        node_text(name_token, content)
            .and_then(compact_identifier)
            .filter(|name| !is_recovered_keyword_declaration(node, name, &acc.file_path))
            .map(|name| {
                let range = line_index.declaration_range(declaration_extent(node));
                // `line` stays where the declaration node itself starts; the
                // range also holds its template or decorator header.
                let line = line_index.range(node).start.line + 1;
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
                GraphFactDeclaration {
                    id,
                    name,
                    kind: kind.to_owned(),
                    line,
                    range,
                    selection_range: line_index.range(name_token),
                    exported,
                    parent,
                    doc_line: None,
                    exported_as: Vec::new(),
                }
            })
    })
}

/// Push `declaration` with its `contains` edge, export and heritage facts;
/// returns its `(id, name)`.
fn push_declaration(
    node: Node<'_>,
    content: &str,
    line_index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    declaration: GraphFactDeclaration,
) -> (String, String) {
    let id = declaration.id.clone();
    let name = declaration.name.clone();
    let line = declaration.line;
    let exported = declaration.exported;
    if let Some(parent) = &declaration.parent {
        acc.edges.push(GraphFactEdge {
            id: format!("{parent}->{id}:contains"),
            from: parent.clone(),
            to: id.clone(),
            relation: "contains".to_owned(),
            source: "ast".to_owned(),
            line,
            resolution: "syntactic".to_owned(),
        });
    }
    if exported {
        acc.exports.push(GraphFactExport {
            id: format!("export:{}:{}", name, line),
            name: name.clone(),
            line,
            export_kind: "language-public".to_owned(),
            local_name: Some(name.clone()),
            source: None,
        });
    }
    acc.declarations.push(declaration);
    acc.declaration_spans
        .push((node.start_byte(), node.end_byte()));
    heritage::collect_heritage(node, content, line_index, acc, &id);
    (id, name)
}

/// A call node's `calls` fact and edge, owned by `caller_decl` or, outside
/// any declaration, the module placeholder.
fn collect_call(
    node: Node<'_>,
    content: &str,
    line_index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    caller_decl: Option<&str>,
    caller_name: Option<&str>,
) {
    let Some((callee, callee_node)) = call_callee(node, content) else {
        return;
    };
    let target = callee
        .rsplit(['.', ':'])
        .find(|segment| !segment.is_empty());
    if let Some(token) = target.and_then(|name| last_name_leaf(callee_node, content, name)) {
        acc.non_reference_tokens.insert(token.start_byte());
    }
    let range = line_index.range(node);
    let line = range.start.line + 1;
    // Module-level code (a Python decorator or `main()` guard, a Go
    // package `var` initializer, a Rust item-level macro) has no
    // enclosing declaration: it is owned by the `module` placeholder
    // with no caller id, the same shape the OXC lane emits.
    let caller = match caller_decl {
        Some(caller) => caller_name.unwrap_or(caller).to_owned(),
        None => MODULE_CALLER.to_owned(),
    };
    let from = caller_decl.map_or_else(|| format!("file:{}", acc.file_path), str::to_owned);
    let id = format!("call:{}:{}:{}", caller, callee, acc.calls.len());
    acc.calls.push(GraphFactCall {
        id: id.clone(),
        caller,
        caller_id: caller_decl.map(str::to_owned),
        callee: callee.to_owned(),
        line,
        range,
        kind: "calls".to_owned(),
        receiver_type: None,
    });
    acc.edges.push(GraphFactEdge {
        id: format!("{from}->{callee}:calls:{line}:{}", acc.edges.len()),
        from,
        to: format!(
            "reference:{}@{}:{}",
            acc.file_path,
            node.start_byte(),
            callee
        ),
        relation: "calls".to_owned(),
        source: "ast".to_owned(),
        line,
        resolution: "unresolved".to_owned(),
    });
}
