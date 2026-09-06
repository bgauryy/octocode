//! Native JS/TS symbol outline via `oxc_parser`.
//!
//! Produces an LSP-compatible `DocumentSymbol[]` tree (nested, numeric
//! `SymbolKind`, 0-based UTF-16 ranges) serialized as JSON — byte-for-byte the
//! shape a language server returns, so the existing `documentSymbols` flatten
//! path consumes it unchanged.
//!
//! **No type inference.** oxc parses ECMAScript/TypeScript *syntax*; it resolves
//! in-file scopes/bindings but not types. Callers stamp `source: "native"` so
//! the fidelity tier is explicit. Type-aware outlines still require a server.
//!
//! oxc is less error-tolerant than tree-sitter, so on a hard parse failure we
//! return `None` and the caller falls back to the tree-sitter signature path.

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    BindingPattern, Class, ClassElement, Declaration, ExportAllDeclaration, ExportDeclaration,
    ExportDefaultDeclarationKind, ExportSpecifier, Expression, Function, ImportDeclaration,
    ImportDeclarationSpecifier, ImportOrExportKind, MethodDefinitionKind, ModuleExportName,
    Program, PropertyKey, Statement, TSEnumDeclaration, TSEnumMemberName,
    TSExternalModuleDeclaration, TSGlobalDeclaration, TSInterfaceDeclaration,
    TSNamespaceDeclaration, TSNamespaceDeclarationBody, TSSignature, TSTypeAliasDeclaration,
    VariableDeclaration, VariableDeclarationKind,
};
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::{GetSpan, SourceType, Span};
use serde::Serialize;

use crate::text::file_extension::is_js_ts_extension;

use super::run_on_deep_stack;

// LSP SymbolKind numeric codes (subset we emit). The TS side maps these back to
// names via `symbolKindName`; keep them in sync with the LSP spec.
mod kind {
    pub const NAMESPACE: u8 = 3;
    pub const CLASS: u8 = 5;
    pub const METHOD: u8 = 6;
    pub const PROPERTY: u8 = 7;
    pub const CONSTRUCTOR: u8 = 9;
    pub const ENUM: u8 = 10;
    pub const INTERFACE: u8 = 11;
    pub const FUNCTION: u8 = 12;
    pub const VARIABLE: u8 = 13;
    pub const CONSTANT: u8 = 14;
    pub const ENUM_MEMBER: u8 = 22;
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
struct DocumentSymbol {
    name: String,
    kind: u8,
    range: Range,
    #[serde(rename = "selectionRange")]
    selection_range: Range,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<DocumentSymbol>,
}

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
    common_js: Vec<GraphCommonJsLoad>,
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
struct GraphCommonJsLoad {
    #[serde(skip_serializing_if = "Option::is_none")]
    specifier: Option<String>,
    line: u32,
    kind: &'static str,
    binding: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
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

/// Maps byte offsets to LSP `(line, character)` positions, where `character`
/// counts UTF-16 code units from the line start (the LSP wire convention).
/// Thin wrapper over the shared `text::utf8_offsets::LineIndex` — see that
/// type for the actual line-start/UTF-16 counting logic.
struct LineIndex<'a>(crate::text::utf8_offsets::LineIndex<'a>);

impl<'a> LineIndex<'a> {
    fn new(content: &'a str) -> Self {
        Self(crate::text::utf8_offsets::LineIndex::new(content))
    }

    fn position(&self, byte_offset: u32) -> Position {
        let (line, character) = self.0.byte_to_position(byte_offset);
        Position { line, character }
    }

    fn range(&self, span: Span) -> Range {
        Range {
            start: self.position(span.start),
            end: self.position(span.end),
        }
    }

    /// Inverse of [`position`]: an LSP `(line, character)` (0-based, UTF-16) to a
    /// byte offset into `content`. Clamps out-of-range input to a valid offset.
    fn byte_offset(&self, line: u32, character: u32) -> u32 {
        self.0.position_to_byte(line, character)
    }
}

fn span_contains(span: Span, offset: u32) -> bool {
    span.start <= offset && offset < span.end
}

fn source_type_for(ext: &str, file_path: &str) -> SourceType {
    let source_type = match ext {
        "ts" | "mts" | "cts" => SourceType::ts(),
        "tsx" => SourceType::tsx(),
        "jsx" => SourceType::jsx(),
        "mjs" => SourceType::mjs(),
        "cjs" => SourceType::cjs(),
        _ => SourceType::default(), // js
    };
    // The final extension alone loses ambient declaration-file mode (.d.ts,
    // .d.mts, .d.cts). Preserve the existing module/JSX policy and let OXC
    // identify declaration files from their complete path.
    let declaration_file = SourceType::from_path(file_path)
        .is_ok_and(|source_type| source_type.is_typescript_definition());
    source_type.with_typescript_definition(declaration_file)
}

/// Native JS/TS document symbols as a JSON `DocumentSymbol[]`.
///
/// Returns `None` for: oversized input, a hard parse failure (caller falls back
/// to tree-sitter), or a file with no extractable top-level symbols.
pub fn extract_js_symbols(content: &str, file_path: &str) -> Option<String> {
    if content.len() > crate::minify::minifier::MAX_SIZE {
        return None;
    }
    let content = content.to_owned();
    let file_path = file_path.to_owned();
    // oxc can ICE on pathological input; contain the unwind so it never crosses
    // the napi FFI boundary and aborts Node (mirrors the minifier/signature
    // guards elsewhere in the crate).
    run_on_deep_stack(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            extract_js_symbols_inner(&content, &file_path)
        }))
        .unwrap_or(None)
    })
}

fn extract_js_symbols_inner(content: &str, file_path: &str) -> Option<String> {
    let ext = crate::text::file_extension::get_extension_internal(file_path, true, "ts");
    if !is_js_ts_extension(&ext) {
        return None;
    }
    let allocator = Allocator::default();
    let parser_ret = Parser::new(&allocator, content, source_type_for(&ext, file_path)).parse();

    // Hard parse failure with nothing recovered → let the caller fall back to
    // the more error-tolerant tree-sitter path rather than emit a stub outline.
    if parser_ret.program.body.is_empty() && !parser_ret.diagnostics.is_empty() {
        return None;
    }

    let line_index = LineIndex::new(content);
    let mut symbols = Vec::new();
    collect_program(&parser_ret.program, &line_index, &mut symbols);

    if symbols.is_empty() {
        return None;
    }
    serde_json::to_string(&symbols).ok()
}

/// Native in-file references to the symbol under `(line, character)` (0-based,
/// UTF-16), as a JSON `Range[]` covering the declaration and every resolved
/// in-file reference. **Same-file only** — oxc resolves bindings within one
/// module, never across files (that needs a language server). The first range
/// is the declaration.
///
/// Returns `None` for non-JS/TS files, oversized content, a hard parse failure,
/// or when the cursor is not on a resolvable binding/reference.
pub fn find_in_file_references(
    content: &str,
    file_path: &str,
    line: u32,
    character: u32,
) -> Option<String> {
    if content.len() > crate::minify::minifier::MAX_SIZE {
        return None;
    }
    let content = content.to_owned();
    let file_path = file_path.to_owned();
    run_on_deep_stack(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            find_in_file_references_inner(&content, &file_path, line, character)
        }))
        .unwrap_or(None)
    })
}

/// Native JS/TS graph facts as JSON.
///
/// This is a syntax-level AST inventory: declarations, imports, exports,
/// function/class containment, and direct call expressions. It deliberately
/// avoids type inference and cross-file resolution; callers combine it with LSP
/// proof when they need semantic identity.
pub fn extract_graph_facts(content: &str, file_path: &str) -> Option<String> {
    if content.len() > crate::minify::minifier::MAX_SIZE {
        return None;
    }
    let content = content.to_owned();
    let file_path = file_path.to_owned();
    run_on_deep_stack(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            extract_graph_facts_inner::<true>(&content, &file_path)
        }))
        .unwrap_or(None)
    })
}

fn extract_graph_facts_inner<const COMMON_JS: bool>(
    content: &str,
    file_path: &str,
) -> Option<String> {
    let ext = crate::text::file_extension::get_extension_internal(file_path, true, "ts");
    if !is_js_ts_extension(&ext) {
        return None;
    }

    let allocator = Allocator::default();
    let parser = Parser::new(&allocator, content, source_type_for(&ext, file_path));
    let parser_ret = if COMMON_JS {
        parser
            .with_config(oxc_parser::config::TokensParserConfig)
            .parse()
    } else {
        parser.parse()
    };
    if parser_ret.program.body.is_empty() && !parser_ret.diagnostics.is_empty() {
        return None;
    }

    let line_index = LineIndex::new(content);
    let mut symbols = Vec::new();
    collect_program(&parser_ret.program, &line_index, &mut symbols);

    let mut export_names = Vec::new();
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    collect_module_facts(
        &parser_ret.program,
        &line_index,
        &mut imports,
        &mut exports,
        &mut export_names,
    );
    export_names.sort();
    export_names.dedup();

    let mut declarations = Vec::new();
    let mut edges = Vec::new();
    flatten_symbols(
        file_path,
        &symbols,
        None,
        &export_names,
        &mut declarations,
        &mut edges,
    );

    let mut calls = Vec::new();
    collect_program_calls(&parser_ret.program, &line_index, &mut calls);
    // OXC tokens make this an absence check, not a binding claim. Candidate
    // loaders still require the full native scope inventory, including escapes.
    let common_js = if COMMON_JS && commonjs::may_contain_loader(&parser_ret.tokens, content) {
        commonjs::collect_common_js_loads(&parser_ret.program, &line_index)
    } else {
        Vec::new()
    };
    let mut declarations_by_name: std::collections::HashMap<&str, Vec<&GraphDeclaration>> =
        std::collections::HashMap::new();
    for declaration in &declarations {
        declarations_by_name
            .entry(&declaration.name)
            .or_default()
            .push(declaration);
    }
    for (index, call) in calls.iter_mut().enumerate() {
        // Names are display labels. Source ranges identify the enclosing occurrence;
        // callee binding resolution requires semantics and remains explicitly unknown.
        call.id = format!(
            "call:{}@{}:{}:{index}",
            file_path, call.range.start.line, call.range.start.character
        );
        let caller = declarations_by_name
            .get(call.caller.as_str())
            .into_iter()
            .flat_map(|items| items.iter().copied())
            .filter(|declaration| {
                (
                    declaration.range.start.line,
                    declaration.range.start.character,
                ) <= (call.range.start.line, call.range.start.character)
                    && (declaration.range.end.line, declaration.range.end.character)
                        >= (call.range.end.line, call.range.end.character)
            })
            .min_by_key(|declaration| {
                (
                    declaration.range.end.line - declaration.range.start.line,
                    declaration
                        .range
                        .end
                        .character
                        .saturating_sub(declaration.range.start.character),
                )
            });
        edges.push(GraphEdge {
            id: format!("edge:{}", call.id),
            from: caller
                .map(|declaration| declaration.id.clone())
                .unwrap_or_else(|| format!("file:{file_path}")),
            to: format!(
                "reference:{}@{}:{}:{index}",
                file_path, call.range.start.line, call.range.start.character
            ),
            relation: call.kind,
            source: "ast",
            line: call.line,
            resolution: "unresolved",
        });
    }

    let facts = GraphFacts {
        kind: "graphFacts",
        source: "native-ast",
        language: ext,
        file: file_path.to_string(),
        declarations,
        imports,
        exports,
        calls,
        common_js,
        edges,
        diagnostics: parser_ret
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.message.to_string())
            .collect(),
    };
    serde_json::to_string(&facts).ok()
}

fn find_in_file_references_inner(
    content: &str,
    file_path: &str,
    line: u32,
    character: u32,
) -> Option<String> {
    let ext = crate::text::file_extension::get_extension_internal(file_path, true, "ts");
    if !is_js_ts_extension(&ext) {
        return None;
    }

    let allocator = Allocator::default();
    let parser_ret = Parser::new(&allocator, content, source_type_for(&ext, file_path)).parse();
    if parser_ret.program.body.is_empty() && !parser_ret.diagnostics.is_empty() {
        return None;
    }

    // `with_build_nodes` records the AST-node table so we can resolve a
    // reference's span via `nodes.kind(node_id).span()`; it is off by default.
    let semantic_ret = SemanticBuilder::new()
        .with_build_nodes(true)
        .build(&parser_ret.program);
    let semantic = semantic_ret.semantic;
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let line_index = LineIndex::new(content);
    let offset = line_index.byte_offset(line, character);

    // Resolve the symbol under the cursor: first try declarations, then any
    // resolved reference (so the cursor can sit on a use site too).
    let mut target = None;
    for symbol_id in scoping.symbol_ids() {
        if span_contains(scoping.symbol_span(symbol_id), offset) {
            target = Some(symbol_id);
            break;
        }
    }
    if target.is_none() {
        'outer: for symbol_id in scoping.symbol_ids() {
            for reference in scoping.get_resolved_references(symbol_id) {
                if span_contains(nodes.kind(reference.node_id()).span(), offset) {
                    target = Some(symbol_id);
                    break 'outer;
                }
            }
        }
    }
    let target = target?;

    // Declaration first, then every resolved in-file reference.
    let mut spans: Vec<Span> = vec![scoping.symbol_span(target)];
    for reference in scoping.get_resolved_references(target) {
        spans.push(nodes.kind(reference.node_id()).span());
    }
    spans.sort_by_key(|span| (span.start, span.end));
    spans.dedup_by_key(|span| (span.start, span.end));

    let ranges: Vec<Range> = spans
        .into_iter()
        .map(|span| line_index.range(span))
        .collect();
    serde_json::to_string(&ranges).ok()
}

fn collect_module_facts(
    program: &Program,
    li: &LineIndex,
    imports: &mut Vec<GraphImport>,
    exports: &mut Vec<GraphExport>,
    export_names: &mut Vec<String>,
) {
    for stmt in &program.body {
        match stmt {
            Statement::ImportDeclaration(decl) => collect_import_declaration(decl, li, imports),
            Statement::ExportDeclaration(decl) => {
                collect_export_declaration(decl, li, exports, export_names);
            }
            Statement::ExportNamedDeclaration(decl) => {
                collect_export_specifiers(
                    &decl.specifiers,
                    decl.export_kind,
                    None,
                    li,
                    exports,
                    export_names,
                );
            }
            Statement::ExportFromDeclaration(decl) => {
                collect_export_specifiers(
                    &decl.specifiers,
                    decl.export_kind,
                    Some(decl.source.value.as_str()),
                    li,
                    exports,
                    export_names,
                );
            }
            Statement::ExportDefaultDeclaration(decl) => {
                let range = li.range(decl.span);
                let name = match &decl.declaration {
                    ExportDefaultDeclarationKind::FunctionDeclaration(function) => function
                        .id
                        .as_ref()
                        .map(|id| id.name.as_str().to_string())
                        .unwrap_or_else(|| "default".to_string()),
                    ExportDefaultDeclarationKind::ClassDeclaration(class) => class
                        .id
                        .as_ref()
                        .map(|id| id.name.as_str().to_string())
                        .unwrap_or_else(|| "default".to_string()),
                    _ => "default".to_string(),
                };
                export_names.push(name.clone());
                exports.push(GraphExport {
                    id: format!("export:{}:{}", name, range.start.line + 1),
                    name,
                    line: range.start.line + 1,
                    export_kind: "value",
                    local_name: None,
                    source: None,
                });
            }
            Statement::ExportAllDeclaration(decl) => {
                collect_export_all(decl, li, exports, export_names);
            }
            _ => {}
        }
    }
}

fn collect_import_declaration(
    decl: &ImportDeclaration,
    li: &LineIndex,
    out: &mut Vec<GraphImport>,
) {
    let range = li.range(decl.span);
    let line = range.start.line + 1;
    let specifier = decl.source.value.as_str().to_string();
    if let Some(specifiers) = &decl.specifiers {
        for (index, item) in specifiers.iter().enumerate() {
            let (local_name, imported_name, import_kind) = match item {
                ImportDeclarationSpecifier::ImportSpecifier(spec) => (
                    Some(spec.local.name.as_str().to_string()),
                    module_export_name(&spec.imported),
                    import_export_kind(if decl.import_kind == ImportOrExportKind::Type {
                        decl.import_kind
                    } else {
                        spec.import_kind
                    }),
                ),
                ImportDeclarationSpecifier::ImportDefaultSpecifier(spec) => (
                    Some(spec.local.name.as_str().to_string()),
                    Some("default".to_string()),
                    import_export_kind(decl.import_kind),
                ),
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(spec) => (
                    Some(spec.local.name.as_str().to_string()),
                    Some("*".to_string()),
                    import_export_kind(decl.import_kind),
                ),
            };
            out.push(GraphImport {
                id: format!("import:{}:{}:{}", specifier, line, index),
                specifier: specifier.clone(),
                line,
                import_kind,
                local_name,
                imported_name,
            });
        }
    } else {
        out.push(GraphImport {
            id: format!("import:{}:{}", specifier, line),
            specifier,
            line,
            import_kind: import_export_kind(decl.import_kind),
            local_name: None,
            imported_name: None,
        });
    }
}

fn collect_export_declaration(
    decl: &ExportDeclaration,
    li: &LineIndex,
    out: &mut Vec<GraphExport>,
    export_names: &mut Vec<String>,
) {
    let export_kind = match &decl.declaration {
        Declaration::TSInterfaceDeclaration(_) | Declaration::TSTypeAliasDeclaration(_) => "type",
        _ => "value",
    };
    for name in declaration_names(&decl.declaration) {
        let range = li.range(decl.span);
        export_names.push(name.clone());
        out.push(GraphExport {
            id: format!("export:{}:{}", name, range.start.line + 1),
            name,
            line: range.start.line + 1,
            export_kind,
            local_name: None,
            source: None,
        });
    }
}

fn collect_export_specifiers(
    specifiers: &[ExportSpecifier],
    export_kind: ImportOrExportKind,
    source: Option<&str>,
    li: &LineIndex,
    out: &mut Vec<GraphExport>,
    export_names: &mut Vec<String>,
) {
    for (index, specifier) in specifiers.iter().enumerate() {
        let name = module_export_name(&specifier.exported)
            .or_else(|| module_export_name(&specifier.local))
            .unwrap_or_else(|| "unknown".to_string());
        export_names.push(name.clone());
        let range = li.range(specifier.span);
        out.push(GraphExport {
            id: format!("export:{}:{}:{}", name, range.start.line + 1, index),
            name,
            line: range.start.line + 1,
            export_kind: if export_kind == ImportOrExportKind::Type {
                "type"
            } else {
                import_export_kind(specifier.export_kind)
            },
            local_name: module_export_name(&specifier.local),
            source: source.map(str::to_owned),
        });
    }
}

fn collect_export_all(
    decl: &ExportAllDeclaration,
    li: &LineIndex,
    out: &mut Vec<GraphExport>,
    export_names: &mut Vec<String>,
) {
    let range = li.range(decl.span);
    let name = decl
        .exported
        .as_ref()
        .and_then(module_export_name)
        .unwrap_or_else(|| "*".to_string());
    export_names.push(name.clone());
    out.push(GraphExport {
        id: format!("export:{}:{}", name, range.start.line + 1),
        name,
        line: range.start.line + 1,
        export_kind: import_export_kind(decl.export_kind),
        local_name: None,
        source: Some(decl.source.value.as_str().to_string()),
    });
}

fn declaration_names(decl: &Declaration) -> Vec<String> {
    match decl {
        Declaration::FunctionDeclaration(function) => function
            .id
            .as_ref()
            .map(|id| vec![id.name.as_str().to_string()])
            .unwrap_or_default(),
        Declaration::ClassDeclaration(class) => class
            .id
            .as_ref()
            .map(|id| vec![id.name.as_str().to_string()])
            .unwrap_or_default(),
        Declaration::VariableDeclaration(variable) => variable
            .declarations
            .iter()
            .filter_map(|declarator| match &declarator.id {
                BindingPattern::BindingIdentifier(id) => Some(id.name.as_str().to_string()),
                _ => None,
            })
            .collect(),
        Declaration::TSInterfaceDeclaration(interface) => {
            vec![interface.id.name.as_str().to_string()]
        }
        Declaration::TSEnumDeclaration(en) => vec![en.id.name.as_str().to_string()],
        Declaration::TSNamespaceDeclaration(module) => vec![module.id.name.as_str().to_string()],
        Declaration::TSExternalModuleDeclaration(module) => {
            vec![module.id.value.as_str().to_string()]
        }
        Declaration::TSGlobalDeclaration(_) => vec!["global".to_string()],
        Declaration::TSTypeAliasDeclaration(alias) => vec![alias.id.name.as_str().to_string()],
        _ => Vec::new(),
    }
}

fn flatten_symbols(
    file_path: &str,
    symbols: &[DocumentSymbol],
    parent: Option<&str>,
    export_names: &[String],
    declarations: &mut Vec<GraphDeclaration>,
    edges: &mut Vec<GraphEdge>,
) {
    for symbol in symbols {
        let id = format!(
            "declaration:{}#{}@{}:{}:{}",
            file_path,
            symbol.name,
            symbol.selection_range.start.line,
            symbol.selection_range.start.character,
            symbol_kind_name(symbol.kind)
        );
        let line = symbol.selection_range.start.line + 1;
        declarations.push(GraphDeclaration {
            id: id.clone(),
            name: symbol.name.clone(),
            kind: symbol_kind_name(symbol.kind),
            line,
            range: symbol_range(symbol),
            selection_range: symbol_selection_range(symbol),
            exported: parent.is_none() && export_names.iter().any(|name| name == &symbol.name),
            parent: parent.map(str::to_string),
        });
        if let Some(parent_id) = parent {
            edges.push(GraphEdge {
                id: format!("{}->{}:contains", parent_id, id),
                from: parent_id.to_string(),
                to: id.clone(),
                relation: "contains",
                source: "ast",
                line,
                resolution: "syntactic",
            });
        }
        flatten_symbols(
            file_path,
            &symbol.children,
            Some(&id),
            export_names,
            declarations,
            edges,
        );
    }
}

#[cfg(test)]
mod graph_occurrence_tests {
    use super::extract_graph_facts;

    fn common_js(source: &str) -> serde_json::Value {
        let facts: serde_json::Value =
            serde_json::from_str(&extract_graph_facts(source, "entry.ts").unwrap()).unwrap();
        facts["commonJs"].clone()
    }

    #[test]
    fn commonjs_links_literal_loads_with_scope_provenance() {
        let loads =
            common_js("const value = require('./value.cjs'); module.require('./other.cjs');");
        assert_eq!(loads[0]["specifier"], "./value.cjs");
        assert_eq!(loads[0]["binding"], "unshadowed-global");
        assert_eq!(loads[0]["kind"], "commonjs-require");
        assert_eq!(loads[1]["specifier"], "./other.cjs");
    }

    #[test]
    fn commonjs_does_not_link_shadowed_loader_names() {
        let loads = common_js(
            "function run(require, module) { require('./fake'); module.require('./fake'); } const text = \"require('./fake')\";",
        );
        assert!(loads.as_array().unwrap().is_empty());
    }

    #[test]
    fn commonjs_covers_nested_escaped_and_computed_loader_syntax() {
        let loads = common_js(
            r#"function nested() { requ\u0069re('./nested.cjs'); } module['require']('./computed.cjs');"#,
        );
        assert_eq!(loads.as_array().unwrap().len(), 2);
        assert_eq!(loads[0]["specifier"], "./nested.cjs");
        assert_eq!(loads[1]["specifier"], "./computed.cjs");
    }

    #[test]
    fn commonjs_precheck_preserves_template_and_escaped_string_loaders() {
        for source in [
            "const text = `result: ${require /* comment */ ('./value.cjs')}`;",
            "module['require']('./value.cjs');",
            r#"module['requ\ire']('./value.cjs');"#,
            r#"import { 'creat\u0065Require' as factory } from 'node:module'; const load = factory(import.meta.url); load('./value.cjs');"#,
        ] {
            let loads = common_js(source);
            assert_eq!(loads.as_array().unwrap().len(), 1, "{source}");
            assert_eq!(loads[0]["specifier"], "./value.cjs", "{source}");
        }
    }

    #[test]
    fn commonjs_rejects_nonlocal_create_require_and_mutated_constants() {
        let loads = common_js(
            "import { createRequire } from 'node:module'; const load = createRequire('/elsewhere/index.js'); load('./value.cjs'); const target = './old.cjs'; target = changed; require(target);",
        );
        assert_eq!(loads[0]["reason"], "non-local-loader-base");
        assert!(loads[0].get("specifier").is_none());
        assert!(loads[1].get("specifier").is_none());
    }

    #[test]
    fn commonjs_tracks_create_require_and_constant_specifiers_by_binding() {
        let loads = common_js(
            "import { createRequire as makeLoader } from 'node:module'; const load = makeLoader(import.meta.url); const target = './value.cjs'; load(target); function nested(target) { load(target); }",
        );
        assert_eq!(loads[0]["specifier"], "./value.cjs");
        assert_eq!(loads[0]["binding"], "create-require");
        assert!(loads[1].get("specifier").is_none());
        assert_eq!(loads[1]["reason"], "non-literal-specifier");
    }

    #[test]
    fn commonjs_keeps_dynamic_and_reassigned_loader_coverage_explicit() {
        let loads = common_js("require(name); require = replacement; require('./value.cjs');");
        assert_eq!(loads.as_array().unwrap().len(), 2);
        assert!(loads[0].get("specifier").is_none());
        assert!(loads[1].get("specifier").is_none());
        assert_eq!(loads[1]["reason"], "loader-reassigned");
    }

    #[test]
    fn commonjs_member_writes_preserve_prior_calls_and_reject_later_calls() {
        for target in [
            "module.require",
            "module['require']",
            r#"module['requ\u0069re']"#,
        ] {
            let loads = common_js(&format!(
                "module.require('./before.cjs'); {target} = module.require('./rhs.cjs'); module.require('./after.cjs'); module['require']('./computed-after.cjs');"
            ));
            assert_eq!(loads.as_array().unwrap().len(), 4, "{target}");
            assert_eq!(loads[0]["specifier"], "./before.cjs", "{target}");
            assert_eq!(loads[1]["specifier"], "./rhs.cjs", "{target}");
            for load in &loads.as_array().unwrap()[2..] {
                assert!(load.get("specifier").is_none(), "{target}: {load}");
                assert_eq!(load["reason"], "loader-reassigned", "{target}");
            }
        }
    }

    #[test]
    fn commonjs_shadowed_member_writes_do_not_poison_global_loader() {
        let loads = common_js(
            "function replace(module) { module.require = replacement; module['require'] = replacement; } module.require('./real.cjs');",
        );
        assert_eq!(loads.as_array().unwrap().len(), 1);
        assert_eq!(loads[0]["specifier"], "./real.cjs");
    }

    #[test]
    fn commonjs_member_writes_do_not_assume_deferred_call_order() {
        for source in [
            "function later() { module.require('./fake.cjs'); } module.require = replacement; later();",
            "module.require = () => module.require('./fake.cjs'); module.require();",
            "function replace() { module.require = replacement; } replace(); module.require('./fake.cjs');",
        ] {
            let loads = common_js(source);
            for load in loads.as_array().unwrap() {
                assert!(load.get("specifier").is_none(), "{source}: {load}");
                assert_eq!(load["reason"], "loader-reassigned", "{source}");
            }
        }
    }

    #[test]
    fn ambient_declaration_files_do_not_report_missing_initializers() {
        for path in ["index.d.ts", "index.d.mts", "index.d.cts"] {
            let value: serde_json::Value = serde_json::from_str(
                &extract_graph_facts(
                    "export const value: string; export function read(): string;",
                    path,
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(value["diagnostics"], serde_json::json!([]), "{path}");
            assert!(value["declarations"]
                .as_array()
                .unwrap()
                .iter()
                .any(|declaration| declaration["name"] == "value"));
        }
        let value: serde_json::Value = serde_json::from_str(
            &extract_graph_facts("export const value: string;", "index.ts").unwrap(),
        )
        .unwrap();
        assert!(!value["diagnostics"].as_array().unwrap().is_empty());
    }

    #[test]
    fn equal_method_names_have_distinct_occurrences_and_unresolved_call_targets() {
        let value: serde_json::Value = serde_json::from_str(&extract_graph_facts(
            "export function run() {} class A { run() { work(); work(); } } class B { run() { work(); } }",
            "names.ts",
        ).unwrap()).unwrap();
        let declarations = value["declarations"].as_array().unwrap();
        let ids: std::collections::HashSet<_> = declarations
            .iter()
            .map(|d| d["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids.len(), declarations.len());
        assert_eq!(
            declarations
                .iter()
                .filter(|d| d["name"] == "run" && d["exported"] == true)
                .count(),
            1
        );
        let calls = value["calls"].as_array().unwrap();
        let call_ids: std::collections::HashSet<_> =
            calls.iter().map(|c| c["id"].as_str().unwrap()).collect();
        assert_eq!(call_ids.len(), calls.len());
        let mut callers = std::collections::HashSet::new();
        for edge in value["edges"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["relation"] == "calls")
        {
            assert!(ids.contains(edge["from"].as_str().unwrap()));
            callers.insert(edge["from"].as_str().unwrap());
            assert!(edge["to"].as_str().unwrap().starts_with("reference:"));
            assert_eq!(edge["resolution"], "unresolved");
        }
        assert_eq!(callers.len(), 2);
    }
}

#[path = "js_oxc_calls.rs"]
mod calls;
use calls::collect_program_calls;

#[path = "js_oxc_commonjs.rs"]
mod commonjs;

#[cfg(test)]
#[path = "js_oxc_commonjs_bench.rs"]
mod commonjs_bench;

fn module_export_name(name: &ModuleExportName) -> Option<String> {
    match name {
        ModuleExportName::IdentifierName(id) => Some(id.name.as_str().to_string()),
        ModuleExportName::IdentifierReference(id) => Some(id.name.as_str().to_string()),
        ModuleExportName::StringLiteral(s) => Some(s.value.as_str().to_string()),
    }
}

fn import_export_kind(kind: ImportOrExportKind) -> &'static str {
    match kind {
        ImportOrExportKind::Type => "type",
        ImportOrExportKind::Value => "value",
    }
}

fn symbol_kind_name(kind: u8) -> &'static str {
    match kind {
        kind::NAMESPACE => "namespace",
        kind::CLASS => "class",
        kind::METHOD => "method",
        kind::PROPERTY => "property",
        kind::CONSTRUCTOR => "constructor",
        kind::ENUM => "enum",
        kind::INTERFACE => "interface",
        kind::FUNCTION => "function",
        kind::VARIABLE => "variable",
        kind::CONSTANT => "constant",
        kind::ENUM_MEMBER => "enumMember",
        _ => "symbol",
    }
}

fn symbol_range(symbol: &DocumentSymbol) -> Range {
    Range {
        start: Position {
            line: symbol.range.start.line,
            character: symbol.range.start.character,
        },
        end: Position {
            line: symbol.range.end.line,
            character: symbol.range.end.character,
        },
    }
}

fn symbol_selection_range(symbol: &DocumentSymbol) -> Range {
    Range {
        start: Position {
            line: symbol.selection_range.start.line,
            character: symbol.selection_range.start.character,
        },
        end: Position {
            line: symbol.selection_range.end.line,
            character: symbol.selection_range.end.character,
        },
    }
}

fn collect_program(program: &Program, li: &LineIndex, out: &mut Vec<DocumentSymbol>) {
    for stmt in &program.body {
        collect_statement(stmt, li, out);
    }
}

fn collect_statement(stmt: &Statement, li: &LineIndex, out: &mut Vec<DocumentSymbol>) {
    match stmt {
        Statement::FunctionDeclaration(f) => push_opt(out, function_symbol(f, li)),
        Statement::ClassDeclaration(c) => push_opt(out, class_symbol(c, li)),
        Statement::VariableDeclaration(v) => collect_variable(v, li, out),
        Statement::TSInterfaceDeclaration(i) => push_opt(out, interface_symbol(i, li)),
        Statement::TSEnumDeclaration(e) => push_opt(out, enum_symbol(e, li)),
        Statement::TSNamespaceDeclaration(m) => push_opt(out, namespace_symbol(m, li)),
        Statement::TSExternalModuleDeclaration(m) => push_opt(out, external_module_symbol(m, li)),
        Statement::TSGlobalDeclaration(m) => push_opt(out, global_symbol(m, li)),
        Statement::TSTypeAliasDeclaration(t) => push_opt(out, type_alias_symbol(t, li)),
        Statement::ExportDeclaration(e) => {
            collect_declaration(&e.declaration, li, out);
        }
        Statement::ExportDefaultDeclaration(e) => match &e.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                push_opt(out, function_symbol(f, li))
            }
            ExportDefaultDeclarationKind::ClassDeclaration(c) => push_opt(out, class_symbol(c, li)),
            _ => {}
        },
        _ => {}
    }
}

fn collect_declaration(decl: &Declaration, li: &LineIndex, out: &mut Vec<DocumentSymbol>) {
    match decl {
        Declaration::FunctionDeclaration(f) => push_opt(out, function_symbol(f, li)),
        Declaration::ClassDeclaration(c) => push_opt(out, class_symbol(c, li)),
        Declaration::VariableDeclaration(v) => collect_variable(v, li, out),
        Declaration::TSInterfaceDeclaration(i) => push_opt(out, interface_symbol(i, li)),
        Declaration::TSEnumDeclaration(e) => push_opt(out, enum_symbol(e, li)),
        Declaration::TSNamespaceDeclaration(m) => push_opt(out, namespace_symbol(m, li)),
        Declaration::TSExternalModuleDeclaration(m) => push_opt(out, external_module_symbol(m, li)),
        Declaration::TSGlobalDeclaration(m) => push_opt(out, global_symbol(m, li)),
        Declaration::TSTypeAliasDeclaration(t) => push_opt(out, type_alias_symbol(t, li)),
        _ => {}
    }
}

fn push_opt(out: &mut Vec<DocumentSymbol>, symbol: Option<DocumentSymbol>) {
    if let Some(symbol) = symbol {
        out.push(symbol);
    }
}

fn function_symbol(f: &Function, li: &LineIndex) -> Option<DocumentSymbol> {
    let id = f.id.as_ref()?;
    Some(leaf(id.name.as_str(), kind::FUNCTION, f.span, id.span, li))
}

fn class_symbol(class: &Class, li: &LineIndex) -> Option<DocumentSymbol> {
    let id = class.id.as_ref()?;
    let mut children = Vec::new();
    for element in &class.body.body {
        match element {
            ClassElement::MethodDefinition(m) => {
                let symbol_kind = match m.kind {
                    MethodDefinitionKind::Constructor => kind::CONSTRUCTOR,
                    _ => kind::METHOD,
                };
                if let Some((name, name_span)) = property_key_name(&m.key) {
                    children.push(leaf(&name, symbol_kind, m.span, name_span, li));
                }
            }
            ClassElement::PropertyDefinition(p) => {
                if let Some((name, name_span)) = property_key_name(&p.key) {
                    children.push(leaf(&name, kind::PROPERTY, p.span, name_span, li));
                }
            }
            ClassElement::AccessorProperty(a) => {
                if let Some((name, name_span)) = property_key_name(&a.key) {
                    children.push(leaf(&name, kind::PROPERTY, a.span, name_span, li));
                }
            }
            _ => {}
        }
    }
    Some(container(
        id.name.as_str(),
        kind::CLASS,
        class.span,
        id.span,
        children,
        li,
    ))
}

fn interface_symbol(iface: &TSInterfaceDeclaration, li: &LineIndex) -> Option<DocumentSymbol> {
    let mut children = Vec::new();
    for signature in &iface.body.body {
        match signature {
            TSSignature::TSPropertySignature(p) => {
                if let Some((name, name_span)) = property_key_name(&p.key) {
                    children.push(leaf(&name, kind::PROPERTY, p.span, name_span, li));
                }
            }
            TSSignature::TSMethodSignature(m) => {
                if let Some((name, name_span)) = property_key_name(&m.key) {
                    children.push(leaf(&name, kind::METHOD, m.span, name_span, li));
                }
            }
            _ => {}
        }
    }
    Some(container(
        iface.id.name.as_str(),
        kind::INTERFACE,
        iface.span,
        iface.id.span,
        children,
        li,
    ))
}

fn enum_symbol(decl: &TSEnumDeclaration, li: &LineIndex) -> Option<DocumentSymbol> {
    let mut children = Vec::new();
    for member in &decl.body.members {
        if let Some((name, name_span)) = enum_member_name(&member.id) {
            children.push(leaf(&name, kind::ENUM_MEMBER, member.span, name_span, li));
        }
    }
    Some(container(
        decl.id.name.as_str(),
        kind::ENUM,
        decl.span,
        decl.id.span,
        children,
        li,
    ))
}

fn namespace_symbol(decl: &TSNamespaceDeclaration, li: &LineIndex) -> Option<DocumentSymbol> {
    let mut children = Vec::new();
    match &decl.body {
        TSNamespaceDeclarationBody::TSModuleBlock(block) => {
            for stmt in &block.body {
                collect_statement(stmt, li, &mut children);
            }
        }
        TSNamespaceDeclarationBody::TSNamespaceDeclaration(inner) => {
            push_opt(&mut children, namespace_symbol(inner, li));
        }
    }
    Some(container(
        decl.id.name.as_str(),
        kind::NAMESPACE,
        decl.span,
        decl.id.span,
        children,
        li,
    ))
}

fn external_module_symbol(
    decl: &TSExternalModuleDeclaration,
    li: &LineIndex,
) -> Option<DocumentSymbol> {
    let mut children = Vec::new();
    if let Some(body) = &decl.body {
        for stmt in &body.body {
            collect_statement(stmt, li, &mut children);
        }
    }
    Some(container(
        decl.id.value.as_str(),
        kind::NAMESPACE,
        decl.span,
        decl.id.span,
        children,
        li,
    ))
}

fn global_symbol(decl: &TSGlobalDeclaration, li: &LineIndex) -> Option<DocumentSymbol> {
    let mut children = Vec::new();
    for stmt in &decl.body.body {
        collect_statement(stmt, li, &mut children);
    }
    Some(container(
        "global",
        kind::NAMESPACE,
        decl.span,
        decl.global_span,
        children,
        li,
    ))
}

fn type_alias_symbol(decl: &TSTypeAliasDeclaration, li: &LineIndex) -> Option<DocumentSymbol> {
    // No dedicated LSP kind for a type alias; `Interface` groups named types and
    // is what most TS servers report.
    Some(leaf(
        decl.id.name.as_str(),
        kind::INTERFACE,
        decl.span,
        decl.id.span,
        li,
    ))
}

fn collect_variable(decl: &VariableDeclaration, li: &LineIndex, out: &mut Vec<DocumentSymbol>) {
    let is_const = matches!(
        decl.kind,
        VariableDeclarationKind::Const
            | VariableDeclarationKind::Using
            | VariableDeclarationKind::AwaitUsing
    );
    for declarator in &decl.declarations {
        let BindingPattern::BindingIdentifier(id) = &declarator.id else {
            // Destructuring patterns have no single name — skip.
            continue;
        };
        let symbol_kind = match &declarator.init {
            Some(Expression::ArrowFunctionExpression(_))
            | Some(Expression::FunctionExpression(_)) => kind::FUNCTION,
            Some(Expression::ClassExpression(_)) => kind::CLASS,
            _ if is_const => kind::CONSTANT,
            _ => kind::VARIABLE,
        };
        out.push(leaf(
            id.name.as_str(),
            symbol_kind,
            declarator.span,
            id.span,
            li,
        ));
    }
}

fn property_key_name(key: &PropertyKey) -> Option<(String, Span)> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some((id.name.as_str().to_string(), id.span)),
        PropertyKey::PrivateIdentifier(p) => Some((format!("#{}", p.name.as_str()), p.span)),
        PropertyKey::StringLiteral(s) => Some((s.value.as_str().to_string(), s.span)),
        _ => None, // computed / numeric / template keys
    }
}

fn enum_member_name(name: &TSEnumMemberName) -> Option<(String, Span)> {
    match name {
        TSEnumMemberName::Identifier(id) => Some((id.name.as_str().to_string(), id.span)),
        TSEnumMemberName::String(s) => Some((s.value.as_str().to_string(), s.span)),
        _ => None, // computed members
    }
}

fn leaf(name: &str, kind: u8, full: Span, selection: Span, li: &LineIndex) -> DocumentSymbol {
    container(name, kind, full, selection, Vec::new(), li)
}

fn container(
    name: &str,
    kind: u8,
    full: Span,
    selection: Span,
    children: Vec<DocumentSymbol>,
    li: &LineIndex,
) -> DocumentSymbol {
    DocumentSymbol {
        name: name.to_string(),
        kind,
        range: li.range(full),
        selection_range: li.range(selection),
        children,
    }
}

#[cfg(test)]
#[path = "js_oxc_tests.rs"]
mod tests;
