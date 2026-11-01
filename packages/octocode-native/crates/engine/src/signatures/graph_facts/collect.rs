//! Declaration and call facts of one node.

use super::imports::{
    collect_c_include, collect_csharp_using, collect_rust_include, collect_rust_module,
    collect_rust_use,
};
use super::positions::NodePositions;
use super::python::collect_python_imports;
use super::rust_macro_args::record_rust_macro;
use super::scala::is_scala;
use super::walk::{RustNodeContext, is_grouped_go_import, method_kind};
use super::{GraphAccumulator, heritage};
use tree_sitter::Node;

use crate::graph::{
    GraphFactCall, GraphFactDeclaration, GraphFactEdge, GraphFactExport, GraphFactImport,
};

use crate::signatures::nodes::{
    declaration, declaration_extent, declaration_label, import_specifier, is_import_node,
    last_name_leaf,
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
    ancestors: &[Node<'_>],
    content: &str,
    line_index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    active_decl: Option<&str>,
    active_name: Option<&str>,
    rust: Option<RustNodeContext<'_>>,
    deadline: std::time::Instant,
) -> Option<(String, String)> {
    acc.test_context.observe(node, content);
    if acc.ext == "rs"
        && node.kind() == "macro_invocation"
        && let Some(rust) = rust.as_ref()
    {
        record_rust_macro(
            node,
            ancestors,
            content,
            line_index,
            acc,
            active_decl,
            active_name,
            rust,
        );
    }
    let decl = declaration_fact(node, ancestors, content, line_index, acc, active_decl);

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
        collect_python_imports(node, ancestors, content, line_index, acc);
    } else if node.kind() == "preproc_include" {
        collect_c_include(node, content, line_index, acc);
    } else if acc.ext == "cs" && node.kind() == "using_directive" {
        collect_csharp_using(node, content, line_index, acc);
    } else if is_import_node(node.kind())
        && !is_grouped_go_import(node)
        && let Some(specifier) = import_specifier(node, content)
    {
        let (line, scala) = (line_index.range(node).start.line + 1, is_scala(&acc.ext));
        let (local_name, imported_name) = if acc.ext == "go" {
            super::go::import_binding(node, content)
        } else {
            (None, None)
        };
        acc.imports.push(GraphFactImport {
            id: format!("import:{}:{}:{}", specifier, line, acc.imports.len()),
            specifier,
            line,
            import_kind: "value".to_owned(),
            local_name,
            imported_name,
            imported_range: None,
            local_range: None,
            resolution_hint: None,
            module_scope: None,
            scope_id: active_decl.filter(|_| scala).map(str::to_owned),
        });
    }

    if node.kind() == "instance_expression" {
        heritage::collect_scala_anonymous(
            node,
            ancestors.last().copied(),
            content,
            line_index,
            acc,
            active_decl,
        );
    }
    if acc.export_forwarding_node == Some(node.kind()) {
        let mut diagnostic = super::note(crate::graph::GraphFactCode::UnsupportedExportForwarding);
        diagnostic.line = Some(line_index.range(node).start.line + 1);
        acc.diagnostics.push(diagnostic);
    }
    if !acc.declarations_only {
        collect_call(
            node, ancestors, content, line_index, acc, next_decl, next_name,
        );
    }
    if !acc.declarations_only {
        super::arguments::collect(
            node, ancestors, content, line_index, acc, next_decl, next_name, deadline,
        );
    }

    next_decl_identity
}

/// The declaration `node` introduces, if any; marks its name token as a
/// non-reference.
fn declaration_fact(
    node: Node<'_>,
    ancestors: &[Node<'_>],
    content: &str,
    line_index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    active_decl: Option<&str>,
) -> Option<GraphFactDeclaration> {
    declaration(node, content).and_then(|(kind, name_token)| {
        let kind = method_kind(kind, node, active_decl);
        let test = super::test_adapter::test_identity(
            node,
            content,
            &acc.ext,
            name_token,
            &acc.test_context,
        );
        let name_token = test
            .as_ref()
            .map_or(name_token, |(_, selection)| *selection);
        let test_identity = test.map(|(identity, _)| identity);
        test_identity
            .as_ref()
            .map(crate::graph::GraphTestIdentity::display_name)
            .or_else(|| declaration_label(node, name_token, content))
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
                let exported = acc.visibility_adapter.exported(
                    node,
                    content,
                    &name,
                    active_decl,
                    active_decl.is_some_and(|parent| acc.public_namespace_scopes.contains(parent)),
                );
                let parent = active_decl.map(str::to_owned);
                let (return_type, try_type) = if acc.ext == "rs" {
                    super::receiver::rust_return_types(node, ancestors, content)
                } else {
                    (None, None)
                };
                acc.non_reference_tokens.insert(name_token.start_byte());
                GraphFactDeclaration {
                    id,
                    name,
                    kind: kind.to_owned(),
                    line,
                    range,
                    selection_range: line_index.range(name_token),
                    source_range: Some(super::validity::byte_range(declaration_extent(node))),
                    selection_byte_range: Some(super::validity::byte_range(name_token)),
                    parse_assessment: super::validity::assess(node, ancestors, line_index),
                    test_identity,
                    exported,
                    parent,
                    owner: match acc.visibility_adapter {
                        _ if acc.ext == "go" => super::go::method_owner(node, content),
                        crate::signatures::visibility::VisibilityAdapter::CFamily => {
                            super::c_family::out_of_line_scope(node, content)
                        }
                        _ => None,
                    },
                    doc_line: None,
                    exported_as: Vec::new(),
                    prototype: matches!(
                        acc.visibility_adapter,
                        crate::signatures::visibility::VisibilityAdapter::CFamily
                    ) && super::c_family::is_prototype(node, kind),
                    return_type,
                    try_type,
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
    if declaration.exported && acc.visibility_adapter.transparent_namespace(node) {
        acc.public_namespace_scopes.insert(id.clone());
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
    ancestors: &[Node<'_>],
    content: &str,
    line_index: &NodePositions<'_>,
    acc: &mut GraphAccumulator,
    caller_decl: Option<&str>,
    caller_name: Option<&str>,
) {
    let Some(observation) = acc
        .call_adapter
        .and_then(|adapter| adapter.observe(node, content))
    else {
        return;
    };
    let callee = observation.callee;
    let target = callee
        .rsplit(['.', ':'])
        .find(|segment| !segment.is_empty());
    if let Some(token) = target.and_then(|name| {
        observation
            .name_token
            .and_then(|token| last_name_leaf(token, content, name))
    }) {
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
        source_range: Some(super::validity::byte_range(node)),
        parse_assessment: super::validity::assess(node, ancestors, line_index),
        kind: observation.kind.to_owned(),
        binding: observation.binding,
        receiver_type: None,
        term_binding: false,
        self_template: None,
        macros: acc.macro_chain.clone().unwrap_or_default(),
        receiver_call: None,
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
