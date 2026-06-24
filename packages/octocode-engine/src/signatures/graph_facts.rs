//! Generic tree-sitter graph facts.
//!
//! This is the language-neutral inventory lane used when the richer OXC JS/TS
//! graph extractor is not available. It deliberately emits syntax facts only:
//! declarations, imports, direct calls, containment, and language-public export
//! hints. LSP remains responsible for semantic identity and reference proof.

use serde::Serialize;
use tree_sitter::{Node, Parser};

use crate::file_extension::get_extension_internal;

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

struct LineIndex<'a> {
    content: &'a str,
    line_starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    fn new(content: &'a str) -> Self {
        let mut line_starts = vec![0usize];
        for (i, b) in content.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self {
            content,
            line_starts,
        }
    }

    fn range(&self, node: Node<'_>) -> Range {
        Range {
            start: self.position(node.start_byte()),
            end: self.position(node.end_byte()),
        }
    }

    fn position(&self, byte_offset: usize) -> Position {
        let line = self
            .line_starts
            .partition_point(|&start| start <= byte_offset)
            .saturating_sub(1);
        let line_start = self.line_starts.get(line).copied().unwrap_or(0);
        let end = byte_offset.min(self.content.len());
        let character = if line_start <= end {
            self.content
                .get(line_start..end)
                .map(|slice| slice.chars().map(char::len_utf16).sum::<usize>() as u32)
                .unwrap_or(0)
        } else {
            0
        };
        Position {
            line: line as u32,
            character,
        }
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
            diagnostics: vec![
                "tree-sitter graph facts are syntax-only; use LSP references/callHierarchy for semantic proof".to_owned(),
            ],
        }
    }
}

pub fn extract_graph_facts(content: &str, file_path: &str) -> Option<String> {
    if content.len() > crate::minifier::MAX_SIZE {
        return None;
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        extract_graph_facts_inner(content, file_path)
    }))
    .unwrap_or(None)
}

fn extract_graph_facts_inner(content: &str, file_path: &str) -> Option<String> {
    let ext = get_extension_internal(file_path, true, "txt");
    if !graph_fact_extensions().iter().any(|item| item == &ext) {
        return None;
    }
    let entry = languages::find_entry(&ext)?;
    let mut parser = Parser::new();
    parser.set_language(&entry.language).ok()?;
    let tree = parser.parse(content.as_bytes(), None)?;
    let root = tree.root_node();
    if root.has_error() {
        // Keep parse-recovered facts, but make the uncertainty explicit.
        // Tree-sitter is intentionally error-tolerant, so an ERROR node does not
        // mean the whole file is unusable.
    }

    let line_index = LineIndex::new(content);
    let mut acc = GraphAccumulator::new(file_path, &ext);
    if root.has_error() {
        acc.diagnostics
            .push("tree-sitter recovered from parse errors; graph facts may be partial".to_owned());
    }
    visit_node(root, content, &line_index, &mut acc, None);

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
    node: Node<'_>,
    content: &str,
    line_index: &LineIndex<'_>,
    acc: &mut GraphAccumulator,
    active_decl: Option<&str>,
) {
    let decl = declaration_kind(node.kind()).and_then(|kind| {
        declaration_name(node, content).map(|name| {
            let range = line_index.range(node);
            let line = range.start.line + 1;
            let id = format!("symbol:{}#{}", acc.file_path, name);
            let exported =
                is_exported_declaration(&acc.ext, node, content, &name, active_decl);
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
    let next_decl_id: Option<String> = if let Some(declaration) = decl {
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
            });
        }
        if exported {
            acc.exports.push(GraphExport {
                id: format!("export:{}:{}", name, line),
                name: name.clone(),
                line,
                export_kind: "language-public",
                local_name: Some(name),
                source: None,
            });
        }
        acc.declarations.push(declaration);
        Some(id)
    } else {
        None
    };
    // Inherit the parent scope when no new declaration was established.
    let next_decl: Option<&str> = next_decl_id.as_deref().or(active_decl);

    if is_import_node(node.kind()) {
        if let Some(specifier) = import_specifier(node, content) {
            let line = line_index.range(node).start.line + 1;
            acc.imports.push(GraphImport {
                id: format!("import:{}:{}:{}", specifier, line, acc.imports.len()),
                specifier,
                line,
                import_kind: "value",
                local_name: None,
                imported_name: None,
            });
        }
    }

    if is_call_node(node.kind()) {
        if let (Some(caller), Some(callee)) =
            (next_decl, call_callee_name(node, content))
        {
            let range = line_index.range(node);
            let line = range.start.line + 1;
            let caller_name = caller
                .rsplit('#')
                .next()
                .map(str::to_owned)
                .unwrap_or_else(|| caller.to_owned());
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
                to: format!("symbol:{}#{}", acc.file_path, callee),
                relation: "calls",
                source: "ast",
                line,
            });
        }
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit_node(child, content, line_index, acc, next_decl);
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
        "class_definition" | "class_declaration" | "class" => Some("class"),
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
        "message" => Some("message"),
        "service" => Some("service"),
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
        "rb" | "ex" | "exs" | "scala" | "sc" | "sbt" => parent.is_none() && !name.starts_with('_'),
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
    language_id.unwrap_or(ext).to_owned()
}

fn fact_families_for_extension(ext: &str) -> Vec<&'static str> {
    let mut families = vec!["declarations", "contains", "calls"];
    match ext {
        "rs" | "py" | "pyi" | "go" | "java" | "c" | "h" | "cpp" | "hpp" | "cc" | "cxx" | "hh"
        | "hxx" | "rb" | "php" | "kt" | "kts" | "ex" | "exs" | "swift" | "scala" | "sc" | "sbt" => {
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

    fn facts(src: &str, path: &str) -> Value {
        let raw = extract_graph_facts(src, path).expect("graph facts expected");
        serde_json::from_str(&raw).expect("valid graph json")
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
