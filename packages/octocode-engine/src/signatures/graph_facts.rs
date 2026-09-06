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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphFacts {
    kind: &'static str,
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
            diagnostics: vec![
                "tree-sitter graph facts are syntax-only; use LSP references/callHierarchy for semantic proof".to_owned(),
            ],
        }
    }
}

pub fn extract_graph_facts(content: &str, file_path: &str) -> Option<String> {
    if content.len() > crate::minify::minifier::MAX_SIZE {
        return None;
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        extract_graph_facts_inner(content, file_path)
    }))
    .unwrap_or(None)
}

fn extract_graph_facts_inner(content: &str, file_path: &str) -> Option<String> {
    extract_graph_facts_before(
        content,
        file_path,
        std::time::Instant::now() + super::extractor::AST_EXECUTION_TIMEOUT,
    )
}

fn extract_graph_facts_before(
    content: &str,
    file_path: &str,
    deadline: std::time::Instant,
) -> Option<String> {
    let ext = get_extension_internal(file_path, true, "txt");
    if !graph_fact_extensions().iter().any(|item| item == &ext) {
        return None;
    }
    let entry = languages::find_entry(&ext)?;
    let mut acc = GraphAccumulator::new(file_path, &ext);
    let mut rust_root_unsupported = (ext == "rs").then_some(true);
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
        if !visit_node(root, content, &line_index, &mut acc, deadline) {
            // Partial module forests cannot safely establish negative import facts.
            acc = GraphAccumulator::new(file_path, &ext);
            acc.diagnostics.push("graph.traversal.deadlineExceeded: graph extraction exceeded its execution deadline; facts are incomplete".to_owned());
            rust_root_unsupported = (ext == "rs").then_some(true);
        }
    } else {
        acc.diagnostics.push(
            "graph.parse.deadlineExceeded: graph parsing was interrupted; facts are incomplete"
                .to_owned(),
        );
    }

    let facts = GraphFacts {
        kind: "graphFacts",
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
    serde_json::to_string(&facts).ok()
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
                signature_outline: !entry.body_query.is_empty(),
                graph_facts: true,
                fact_families: fact_families_for_extension(ext),
            })
        })
        .collect();
    serde_json::to_string(&capabilities).unwrap_or_else(|_| "[]".to_owned())
}

fn visit_node(
    root: Node<'_>,
    content: &str,
    line_index: &LineIndex<'_>,
    acc: &mut GraphAccumulator,
    deadline: std::time::Instant,
) -> bool {
    enum Frame<'tree> {
        Enter(Node<'tree>),
        ExitDeclaration,
    }

    let mut frames = vec![Frame::Enter(root)];
    let mut declarations: Vec<(String, String)> = Vec::new();
    while let Some(frame) = frames.pop() {
        if std::time::Instant::now() >= deadline {
            return false;
        }
        let node = match frame {
            Frame::Enter(node) => node,
            Frame::ExitDeclaration => {
                declarations.pop();
                continue;
            }
        };
        let active = declarations.last();
        if let Some(identity) = collect_node_facts(
            node,
            content,
            line_index,
            acc,
            active.map(|(id, _)| id.as_str()),
            active.map(|(_, name)| name.as_str()),
            deadline,
        ) {
            declarations.push(identity);
            frames.push(Frame::ExitDeclaration);
        }

        // Preserve preorder and declaration lifetimes without using the call stack.
        // A cursor enumerates wide sibling lists without repeated child indexing.
        let children_start = frames.len();
        let mut cursor = node.walk();
        frames.extend(node.named_children(&mut cursor).map(Frame::Enter));
        frames[children_start..].reverse();
    }
    std::time::Instant::now() < deadline
}

fn collect_node_facts(
    node: Node<'_>,
    content: &str,
    line_index: &LineIndex<'_>,
    acc: &mut GraphAccumulator,
    active_decl: Option<&str>,
    active_name: Option<&str>,
    deadline: std::time::Instant,
) -> Option<(String, String)> {
    if acc.ext == "rs" && node.kind() == "macro_invocation" {
        let message = "unsupported Rust macro expansion: macro-generated imports are not linked";
        if !acc
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic == message)
        {
            acc.diagnostics.push(message.to_owned());
        }
    }
    let decl = declaration_kind(node.kind()).and_then(|kind| {
        declaration_name(node, content).map(|name| {
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
            GraphDeclaration {
                id,
                name,
                kind,
                line,
                range,
                selection_range: line_index.range(name_node(node).unwrap_or(node)),
                exported,
                parent,
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

    if acc.ext == "rs" && node.kind() == "use_declaration" {
        if let Some(argument) = node.child_by_field_name("argument") {
            collect_rust_imports(
                argument,
                &rust_module_scope(node, content),
                content,
                line_index,
                acc,
                rust_unsupported_context(node, content),
                deadline,
            );
        }
    } else if acc.ext == "rs" && node.kind() == "mod_item" {
        if let Some(name) = declaration_name(node, content) {
            let line = line_index.range(node).start.line + 1;
            let (path, mut unsupported) = rust_module_attributes(node, content);
            unsupported |= rust_block_local(node);
            unsupported |= node
                .child_by_field_name("body")
                .is_some_and(|body| rust_inner_unsupported(body, content));
            let scope = rust_module_scope(node, content);
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
    } else if is_import_node(node.kind()) {
        if let Some(specifier) = import_specifier(node, content) {
            let line = line_index.range(node).start.line + 1;
            acc.imports.push(GraphImport {
                id: format!("import:{}:{}:{}", specifier, line, acc.imports.len()),
                specifier,
                line,
                import_kind: "value",
                local_name: None,
                imported_name: None,
                resolution_hint: None,
                module_scope: None,
            });
        }
    }

    if is_call_node(node.kind()) {
        if let (Some(caller), Some(callee)) = (next_decl, call_callee_name(node, content)) {
            let range = line_index.range(node);
            let line = range.start.line + 1;
            let caller_name = next_name.unwrap_or(caller).to_owned();
            let id = format!("call:{}:{}:{}", caller_name, callee, acc.calls.len());
            acc.calls.push(GraphCall {
                id: id.clone(),
                caller: caller_name,
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
    }

    next_decl_identity
}

fn rust_module_attributes(node: Node<'_>, content: &str) -> (Option<String>, bool) {
    let mut previous = node.prev_named_sibling();
    let mut path = None;
    let mut unsupported = false;
    while let Some(attribute) = previous {
        if matches!(attribute.kind(), "line_comment" | "block_comment") {
            previous = attribute.prev_named_sibling();
            continue;
        }
        if attribute.kind() != "attribute_item" {
            break;
        }
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
                    if path.is_some() {
                        unsupported = true;
                    }
                    path = Some(value.to_owned());
                } else {
                    unsupported = true;
                }
            }
            "allow"
            | "warn"
            | "deny"
            | "forbid"
            | "expect"
            | "doc"
            | "deprecated"
            | "no_implicit_prelude" => {}
            _ => unsupported = true,
        }
        previous = attribute.prev_named_sibling();
    }
    (path, unsupported)
}

fn rust_block_local(node: Node<'_>) -> bool {
    let mut parent = node.parent();
    while let Some(scope) = parent {
        if matches!(scope.kind(), "block" | "function_item" | "impl_item") {
            return true;
        }
        parent = scope.parent();
    }
    false
}

fn rust_module_scope(node: Node<'_>, content: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut parent = node.parent();
    while let Some(scope) = parent {
        if scope.kind() == "mod_item" {
            if let Some(name) = declaration_name(scope, content) {
                names.push(name);
            }
        }
        parent = scope.parent();
    }
    names.reverse();
    names
}

fn rust_inner_unsupported(node: Node<'_>, content: &str) -> bool {
    let mut cursor = node.walk();
    let unsupported = node.named_children(&mut cursor).any(|child| {
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
        !matches!(
            name,
            "allow"
                | "warn"
                | "deny"
                | "forbid"
                | "expect"
                | "doc"
                | "deprecated"
                | "no_implicit_prelude"
        )
    });
    unsupported
}

fn rust_unsupported_context(node: Node<'_>, content: &str) -> bool {
    let mut current = Some(node);
    while let Some(scope) = current {
        if rust_module_attributes(scope, content).1 || rust_inner_unsupported(scope, content) {
            return true;
        }
        current = scope.parent();
    }
    false
}

/// Expand Rust use trees through grammar nodes, preserving aliases and multiline groups.
fn collect_rust_imports(
    node: Node<'_>,
    module_scope: &[String],
    content: &str,
    index: &LineIndex<'_>,
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
                let (path, alias) = if node.kind() == "use_as_clause" {
                    (
                        node.child_by_field_name("path")
                            .and_then(|n| node_text(n, content))
                            .unwrap_or(text),
                        node.child_by_field_name("alias")
                            .and_then(|n| node_text(n, content)),
                    )
                } else {
                    (text, None)
                };
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
                acc.imports.push(GraphImport {
                    id: format!("import:{specifier}:{line}:{}", acc.imports.len()),
                    specifier,
                    line,
                    import_kind: "value",
                    local_name: Some(alias.unwrap_or(&imported).to_owned()),
                    imported_name: Some(imported),
                    resolution_hint: unsupported.then_some("unsupported"),
                    module_scope: Some(module_scope.to_vec()),
                });
            }
        }
    }
}

fn declaration_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "function_item"
        | "function_definition"
        | "function_declaration"
        | "method_declaration"
        | "method_definition"
        | "method"
        | "singleton_method"
        | "function_clause" => Some("function"),
        "constructor_declaration" => Some("constructor"),
        "class_definition" | "class_declaration" | "class_specifier" | "class" => Some("class"),
        "struct_item" | "struct_specifier" | "struct_declaration" => Some("struct"),
        "enum_item" | "enum_declaration" | "enum_specifier" => Some("enum"),
        "trait_item" => Some("trait"),
        "interface_declaration" | "interface_item" => Some("interface"),
        "impl_item" => Some("impl"),
        "mod_item" | "module_definition" => Some("module"),
        "const_item" | "const_declaration" | "constant_declaration" | "static_item" => {
            Some("constant")
        }
        "type_item" | "type_declaration" | "type_alias" | "type_definition" | "type_spec" => {
            Some("type")
        }
        "macro_definition" | "macro_rule" => Some("macro"),
        _ => None,
    }
}

fn name_node(node: Node<'_>) -> Option<Node<'_>> {
    for field in ["name", "type", "path"] {
        if let Some(child) = node.child_by_field_name(field) {
            if is_name_like(child.kind()) {
                return Some(child);
            }
            if let Some(descendant) = first_name_descendant(child, 2) {
                return Some(descendant);
            }
        }
    }
    first_name_descendant(node, 4)
}

fn declaration_name(node: Node<'_>, content: &str) -> Option<String> {
    let name = node_text(name_node(node)?, content)?;
    compact_identifier(name)
}

fn first_name_descendant(node: Node<'_>, depth: u8) -> Option<Node<'_>> {
    if depth == 0 {
        return None;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if is_name_like(child.kind()) {
            return Some(child);
        }
        if let Some(found) = first_name_descendant(child, depth - 1) {
            return Some(found);
        }
    }
    None
}

fn is_name_like(kind: &str) -> bool {
    matches!(
        kind,
        "identifier"
            | "type_identifier"
            | "field_identifier"
            | "property_identifier"
            | "scoped_identifier"
            | "scoped_type_identifier"
            | "namespace_identifier"
            | "module_name"
            | "simple_identifier"
            | "constant"
            | "alias"
            | "atom"
            | "word"
            | "name"
    )
}

fn is_import_node(kind: &str) -> bool {
    matches!(
        kind,
        "import_statement"
            | "import_from_statement"
            | "import_declaration"
            | "import_spec"
            | "use_declaration"
            | "extern_crate_declaration"
            | "preproc_include"
            | "require_command"
            | "source_command"
    )
}

fn push_language_import(
    acc: &mut GraphAccumulator,
    specifier: String,
    line: u32,
    import_kind: &'static str,
    local_name: Option<String>,
    imported_name: Option<String>,
    hint: &'static str,
) {
    acc.imports.push(GraphImport {
        id: format!("import:{}:{}", line, acc.imports.len()),
        specifier,
        line,
        import_kind,
        local_name,
        imported_name,
        resolution_hint: Some(hint),
        module_scope: None,
    });
}

fn collect_python_imports(
    node: Node<'_>,
    content: &str,
    li: &LineIndex,
    acc: &mut GraphAccumulator,
) {
    let module = node
        .child_by_field_name("module_name")
        .and_then(|module| node_text(module, content));
    let mut cursor = node.walk();
    let names: Vec<_> = node.children_by_field_name("name", &mut cursor).collect();
    let line = li.range(node).start.line + 1;
    for item in &names {
        let name_node = item.child_by_field_name("name").unwrap_or(*item);
        let Some(name) = node_text(name_node, content) else {
            continue;
        };
        let alias = item
            .child_by_field_name("alias")
            .and_then(|alias| node_text(alias, content));
        let specifier = module.unwrap_or(name);
        push_language_import(
            acc,
            specifier.to_owned(),
            line,
            "value",
            Some(
                alias
                    .unwrap_or_else(|| {
                        if module.is_some() {
                            name
                        } else {
                            name.split('.').next().unwrap_or(name)
                        }
                    })
                    .to_owned(),
            ),
            Some(if module.is_some() { name } else { "*" }.to_owned()),
            if specifier.starts_with('.') {
                "python-relative"
            } else {
                "python-absolute"
            },
        );
    }
    if names.is_empty() {
        if let Some(module) = module {
            push_language_import(
                acc,
                module.to_owned(),
                line,
                "value",
                Some("*".to_owned()),
                Some("*".to_owned()),
                if module.starts_with('.') {
                    "python-relative"
                } else {
                    "python-absolute"
                },
            );
        }
    }
}

fn import_specifier(node: Node<'_>, content: &str) -> Option<String> {
    if let Some(string_node) = first_string_descendant(node, 4) {
        return node_text(string_node, content).and_then(clean_specifier);
    }
    node_text(node, content).and_then(clean_specifier)
}

fn first_string_descendant(node: Node<'_>, depth: u8) -> Option<Node<'_>> {
    if depth == 0 {
        return None;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if is_string_like(child.kind()) {
            return Some(child);
        }
        if let Some(found) = first_string_descendant(child, depth - 1) {
            return Some(found);
        }
    }
    None
}

fn is_string_like(kind: &str) -> bool {
    matches!(
        kind,
        "string"
            | "string_literal"
            | "interpreted_string_literal"
            | "raw_string_literal"
            | "system_lib_string"
            | "string_content"
            | "quoted_string"
    )
}

fn is_call_node(kind: &str) -> bool {
    matches!(
        kind,
        "call_expression"
            | "call"
            | "method_invocation"
            | "function_call_expression"
            | "member_call_expression"
            | "macro_invocation"
            | "command"
            | "object_creation_expression"
            | "constructor_invocation"
    )
}

fn call_callee_name(node: Node<'_>, content: &str) -> Option<String> {
    for field in ["function", "name", "method", "macro", "constructor"] {
        if let Some(child) = node.child_by_field_name(field) {
            if let Some(name) = node_text(child, content).and_then(compact_identifier) {
                return Some(name);
            }
            if let Some(descendant) = first_name_descendant(child, 3) {
                if let Some(name) = node_text(descendant, content).and_then(compact_identifier) {
                    return Some(name);
                }
            }
        }
    }
    first_name_descendant(node, 3)
        .and_then(|child| node_text(child, content))
        .and_then(compact_identifier)
}

fn is_exported_declaration(
    ext: &str,
    node: Node<'_>,
    content: &str,
    name: &str,
    parent: Option<&str>,
) -> bool {
    let text = node_text(node, content).unwrap_or("").trim_start();
    match ext {
        "rs" => text.starts_with("pub ") || text.starts_with("pub("),
        "go" => name.chars().next().is_some_and(|ch| ch.is_uppercase()),
        "py" | "pyi" => parent.is_none() && !name.starts_with('_'),
        "java" | "kt" | "kts" | "cs" | "php" | "swift" => {
            text.starts_with("public ") || text.starts_with("export ")
        }
        "c" | "h" | "cpp" | "hpp" | "cc" | "cxx" | "hh" | "hxx" => {
            parent.is_none() && !text.starts_with("static ")
        }
        "rb" | "scala" | "sc" | "sbt" => parent.is_none() && !name.starts_with('_'),
        _ => parent.is_none() && !name.starts_with('_'),
    }
}

fn node_text<'a>(node: Node<'_>, content: &'a str) -> Option<&'a str> {
    content.get(node.start_byte()..node.end_byte())
}

fn compact_identifier(text: &str) -> Option<String> {
    let trimmed = text.trim().trim_end_matches('!').trim();
    if trimmed.is_empty() || trimmed.len() > 160 {
        return None;
    }
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return None;
    }
    let value = trimmed
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('`')
        .trim()
        .to_owned();
    if value.is_empty() || value.len() > 160 {
        None
    } else {
        Some(value)
    }
}

fn clean_specifier(text: &str) -> Option<String> {
    let mut value = text.trim();
    for prefix in [
        "import",
        "from",
        "use",
        "extern crate",
        "#include",
        "require",
        "source",
    ] {
        if let Some(rest) = value.strip_prefix(prefix) {
            value = rest.trim();
            break;
        }
    }
    value = value.trim_end_matches(';').trim();
    value = value
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('`')
        .trim_matches('<')
        .trim_matches('>')
        .trim();
    if value.is_empty() || value.len() > 200 || value.contains('\n') || value.contains('\r') {
        None
    } else {
        Some(value.to_owned())
    }
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
    let mut families = vec!["declarations", "contains", "calls"];
    match canonical_extension(ext) {
        // JS/TS (oxc lane) already emit import/export facts — advertise them so
        // `getGraphFactCapabilities` matches what `extractGraphFacts` returns.
        "ts" | "tsx" | "js" | "rs" | "py" | "go" | "java" | "c" | "cpp" | "rb" | "php" | "kt"
        | "swift" | "scala" => {
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

    #[test]
    fn rust_cfg_and_path_attributes_survive_comments_and_inner_attributes() {
        let value = facts(
            "#[cfg(feature = \"x\")]\n// note\nmod child;\n#[path = \"actual.rs\"]\n/// documented\nmod alias;\nmod gated { #![cfg(feature = \"x\")] use super::Thing; }",
            "src/lib.rs",
        );
        assert!(value["modules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|module| module["name"] == "child" && module["unsupported"] == true));
        assert!(value["modules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|module| module["name"] == "alias" && module["path"] == "actual.rs"));
        assert!(value["modules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|module| module["name"] == "gated" && module["unsupported"] == true));
        let root = facts("#![cfg(feature = \"x\")]\nmod child;", "src/lib.rs");
        assert_eq!(root["rustRootUnsupported"], true);
        let local = facts(
            "fn f() { mod hidden { #[path = \"child.rs\"] mod child; } }",
            "src/lib.rs",
        );
        assert!(local["modules"]
            .as_array()
            .unwrap()
            .iter()
            .all(|module| module["unsupported"] == true));
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
        assert!(modules
            .iter()
            .any(|module| module["name"] == "conditional" && module["unsupported"] == true));
        assert!(value["imports"]
            .as_array()
            .unwrap()
            .iter()
            .any(|import| import["specifier"] == "super::Thing"
                && import["moduleScope"] == serde_json::json!(["inside"])
                && import["resolutionHint"].is_null()));
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
        assert!(imports
            .iter()
            .any(|i| i["localName"] == "Alias" && i["importedName"] == "Thing"));
    }

    #[test]
    fn rust_nonconventional_modules_and_macros_remain_explicitly_unsupported() {
        let value = facts(
            "#[cfg(feature = \"x\")] #[path = \"other.rs\"] mod child;\nmod inline { use super::Thing; }\nmake_imports!();",
            "src/lib.rs",
        );
        let imports = value["imports"].as_array().unwrap();
        assert!(imports
            .iter()
            .any(|item| item["specifier"] == "self::child"
                && item["resolutionHint"] == "unsupported"));
        assert!(imports
            .iter()
            .any(|item| item["specifier"] == "super::Thing"
                && item["moduleScope"] == serde_json::json!(["inline"])));
        assert!(value["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item.as_str().unwrap().contains("macro expansion")));
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
        assert!(value["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|edge| edge["relation"] == "contains"
                && edge["from"] == class["id"]
                && edge["to"] == method["id"]));
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
        assert!(graph["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d.as_str().unwrap().contains("graph.parse.deadlineExceeded")));
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
            std::time::Instant::now()
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
        assert!(!graph["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d.as_str().unwrap().contains("parse errors")));
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
        assert!(graph
            .get("language")
            .is_some_and(|language| language == "rust"));
        assert!(graph
            .get("declarations")
            .and_then(Value::as_array)
            .is_some_and(|decls| decls
                .iter()
                .any(|decl| decl.get("name").is_some_and(|name| name == "Point"))));
        assert!(graph
            .get("declarations")
            .and_then(Value::as_array)
            .is_some_and(|decls| decls.iter().any(|decl| decl
                .get("name")
                .is_some_and(|name| name == "distance")
                && decl
                    .get("exported")
                    .is_some_and(|exported| exported == true))));
        assert!(graph
            .get("imports")
            .and_then(Value::as_array)
            .is_some_and(|imports| imports.iter().any(|import| import
                .get("specifier")
                .and_then(Value::as_str)
                .is_some_and(|specifier| specifier.contains("crate::other")))));
        assert!(graph
            .get("calls")
            .and_then(Value::as_array)
            .is_some_and(|calls| calls
                .iter()
                .any(|call| call.get("callee").is_some_and(|callee| callee == "helper"))));
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
        assert!(graph
            .get("language")
            .is_some_and(|language| language == "python"));
        assert!(graph
            .get("declarations")
            .and_then(Value::as_array)
            .is_some_and(|decls| decls
                .iter()
                .any(|decl| decl.get("name").is_some_and(|name| name == "Service"))));
        assert!(graph
            .get("declarations")
            .and_then(Value::as_array)
            .is_some_and(|decls| decls.iter().any(|decl| decl
                .get("name")
                .is_some_and(|name| name == "helper")
                && decl
                    .get("exported")
                    .is_some_and(|exported| exported == true))));
        assert!(graph
            .get("imports")
            .and_then(Value::as_array)
            .is_some_and(|imports| imports.iter().any(|import| import
                .get("specifier")
                .and_then(Value::as_str)
                .is_some_and(|specifier| specifier.contains("os")))));
        assert!(graph
            .get("calls")
            .and_then(Value::as_array)
            .is_some_and(|calls| calls
                .iter()
                .any(|call| call.get("callee").is_some_and(|callee| callee == "helper"))));
    }

    #[test]
    fn graph_fact_capabilities_include_rust_and_python() {
        let json = graph_fact_capabilities_json();
        assert!(json.contains("\"extension\":\"rs\""));
        assert!(json.contains("\"extension\":\"py\""));
    }
}
