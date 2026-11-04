//! Declaration and call facts of one node.

use super::declarations::{declaration_fact, named_declaration, push_declaration};
use super::imports::{
    collect_c_include, collect_csharp_using, collect_rust_include, collect_rust_module,
    collect_rust_use,
};
use super::positions::NodePositions;
use super::python::collect_python_imports;
use super::rust_macro_args::record_rust_macro;
use super::scala::is_scala;
use super::walk::{RustNodeContext, is_grouped_go_import};
use super::{GraphAccumulator, heritage};
use tree_sitter::Node;

use crate::graph::{GraphFactCall, GraphFactEdge, GraphFactImport};

use crate::signatures::nodes::{import_specifier, is_import_node, last_name_leaf};

/// Caller label of calls outside any declaration (matches the OXC lane).
const MODULE_CALLER: &str = "module";

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
    if acc.ext == "go" {
        for (kind, name) in super::go::spec_names(node, ancestors) {
            if let Some(declaration) = named_declaration(
                kind,
                name,
                node,
                ancestors,
                content,
                line_index,
                acc,
                active_decl,
            ) {
                push_declaration(node, content, line_index, acc, declaration);
            }
        }
    }
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
    } else if acc.js_fallback && node.kind() == "import_statement" {
        super::js_fallback::collect_import(node, content, line_index, acc);
    } else if acc.js_fallback && node.kind() == "export_statement" {
        super::js_fallback::collect_export(node, content, line_index, acc);
    } else if acc.ext == "py" && matches!(node.kind(), "import_statement" | "import_from_statement")
    {
        collect_python_imports(node, ancestors, content, line_index, acc);
    } else if acc.ext == "py" && node.kind() == "assignment" {
        acc.typing.observe_assignment(node, ancestors, content);
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
