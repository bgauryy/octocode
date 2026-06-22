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
    BindingPattern, Class, ClassElement, Declaration, ExportAllDeclaration,
    ExportDefaultDeclarationKind, ExportNamedDeclaration, Expression, ForStatementInit, Function,
    ImportDeclaration, ImportDeclarationSpecifier, ImportOrExportKind, MethodDefinitionKind,
    ModuleExportName, Program, PropertyKey, Statement, TSEnumDeclaration, TSEnumMemberName,
    TSInterfaceDeclaration, TSModuleDeclaration, TSModuleDeclarationBody, TSModuleDeclarationName,
    TSSignature, TSTypeAliasDeclaration, VariableDeclaration, VariableDeclarationKind,
};
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::{GetSpan, SourceType, Span};
use serde::Serialize;

use crate::file_extension::is_js_ts_extension;

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

/// Maps byte offsets to LSP `(line, character)` positions, where `character`
/// counts UTF-16 code units from the line start (the LSP wire convention).
struct LineIndex<'a> {
    content: &'a str,
    /// Byte offset of the first character of each 0-based line.
    line_starts: Vec<u32>,
}

impl<'a> LineIndex<'a> {
    fn new(content: &'a str) -> Self {
        let mut line_starts = vec![0u32];
        for (i, b) in content.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push((i + 1) as u32);
            }
        }
        Self {
            content,
            line_starts,
        }
    }

    fn position(&self, byte_offset: u32) -> Position {
        // Highest line whose start byte is <= byte_offset.
        let line = self
            .line_starts
            .partition_point(|&start| start <= byte_offset)
            .saturating_sub(1);
        let line_start = self.line_starts.get(line).copied().unwrap_or(0) as usize;
        let end = (byte_offset as usize).min(self.content.len());
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

    fn range(&self, span: Span) -> Range {
        Range {
            start: self.position(span.start),
            end: self.position(span.end),
        }
    }

    /// Inverse of [`position`]: an LSP `(line, character)` (0-based, UTF-16) to a
    /// byte offset into `content`. Clamps out-of-range input to a valid offset.
    fn byte_offset(&self, line: u32, character: u32) -> u32 {
        let line_start = self
            .line_starts
            .get(line as usize)
            .copied()
            .unwrap_or(self.content.len() as u32) as usize;
        let mut utf16 = 0u32;
        let mut byte = line_start;
        for ch in self.content.get(line_start..).unwrap_or("").chars() {
            if utf16 >= character || ch == '\n' {
                break;
            }
            utf16 += ch.len_utf16() as u32;
            byte += ch.len_utf8();
        }
        byte as u32
    }
}

fn span_contains(span: Span, offset: u32) -> bool {
    span.start <= offset && offset < span.end
}

fn source_type_for(ext: &str) -> SourceType {
    match ext {
        "ts" | "mts" | "cts" => SourceType::ts(),
        "tsx" => SourceType::tsx(),
        "jsx" => SourceType::jsx(),
        "mjs" => SourceType::mjs(),
        "cjs" => SourceType::cjs(),
        _ => SourceType::default(), // js
    }
}

/// Native JS/TS document symbols as a JSON `DocumentSymbol[]`.
///
/// Returns `None` for: oversized input, a hard parse failure (caller falls back
/// to tree-sitter), or a file with no extractable top-level symbols.
pub fn extract_js_symbols(content: &str, file_path: &str) -> Option<String> {
    if content.len() > crate::minifier::MAX_SIZE {
        return None;
    }
    // oxc can ICE on pathological input; contain the unwind so it never crosses
    // the napi FFI boundary and aborts Node (mirrors the minifier/signature
    // guards elsewhere in the crate).
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        extract_js_symbols_inner(content, file_path)
    }))
    .unwrap_or(None)
}

fn extract_js_symbols_inner(content: &str, file_path: &str) -> Option<String> {
    let ext = crate::file_extension::get_extension_internal(file_path, true, "ts");
    if !is_js_ts_extension(&ext) {
        return None;
    }
    let allocator = Allocator::default();
    let parser_ret = Parser::new(&allocator, content, source_type_for(&ext)).parse();

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
    if content.len() > crate::minifier::MAX_SIZE {
        return None;
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        find_in_file_references_inner(content, file_path, line, character)
    }))
    .unwrap_or(None)
}

/// Native JS/TS graph facts as JSON.
///
/// This is a syntax-level AST inventory: declarations, imports, exports,
/// function/class containment, and direct call expressions. It deliberately
/// avoids type inference and cross-file resolution; callers combine it with LSP
/// proof when they need semantic identity.
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
    let ext = crate::file_extension::get_extension_internal(file_path, true, "ts");
    if !is_js_ts_extension(&ext) {
        return None;
    }

    let allocator = Allocator::default();
    let parser_ret = Parser::new(&allocator, content, source_type_for(&ext)).parse();
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
    for call in &calls {
        edges.push(GraphEdge {
            id: format!(
                "{}:{}->{}:{}",
                file_path, call.caller, call.callee, call.line
            ),
            from: format!("symbol:{}#{}", file_path, call.caller),
            to: format!("symbol:{}#{}", file_path, call.callee),
            relation: call.kind,
            source: "ast",
            line: call.line,
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
    let ext = crate::file_extension::get_extension_internal(file_path, true, "ts");
    if !is_js_ts_extension(&ext) {
        return None;
    }

    let allocator = Allocator::default();
    let parser_ret = Parser::new(&allocator, content, source_type_for(&ext)).parse();
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
            Statement::ExportNamedDeclaration(decl) => {
                collect_export_named(decl, li, exports, export_names);
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
            let (local_name, imported_name) = match item {
                ImportDeclarationSpecifier::ImportSpecifier(spec) => (
                    Some(spec.local.name.as_str().to_string()),
                    module_export_name(&spec.imported),
                ),
                ImportDeclarationSpecifier::ImportDefaultSpecifier(spec) => (
                    Some(spec.local.name.as_str().to_string()),
                    Some("default".to_string()),
                ),
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(spec) => (
                    Some(spec.local.name.as_str().to_string()),
                    Some("*".to_string()),
                ),
            };
            out.push(GraphImport {
                id: format!("import:{}:{}:{}", specifier, line, index),
                specifier: specifier.clone(),
                line,
                import_kind: import_export_kind(decl.import_kind),
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

fn collect_export_named(
    decl: &ExportNamedDeclaration,
    li: &LineIndex,
    out: &mut Vec<GraphExport>,
    export_names: &mut Vec<String>,
) {
    if let Some(inner) = &decl.declaration {
        for name in declaration_names(inner) {
            let range = li.range(decl.span);
            export_names.push(name.clone());
            out.push(GraphExport {
                id: format!("export:{}:{}", name, range.start.line + 1),
                name,
                line: range.start.line + 1,
                export_kind: import_export_kind(decl.export_kind),
                local_name: None,
                source: decl.source.as_ref().map(|s| s.value.as_str().to_string()),
            });
        }
    }

    for (index, specifier) in decl.specifiers.iter().enumerate() {
        let name = module_export_name(&specifier.exported)
            .or_else(|| module_export_name(&specifier.local))
            .unwrap_or_else(|| "unknown".to_string());
        export_names.push(name.clone());
        let range = li.range(specifier.span);
        out.push(GraphExport {
            id: format!("export:{}:{}:{}", name, range.start.line + 1, index),
            name,
            line: range.start.line + 1,
            export_kind: import_export_kind(specifier.export_kind),
            local_name: module_export_name(&specifier.local),
            source: decl.source.as_ref().map(|s| s.value.as_str().to_string()),
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
        Declaration::TSModuleDeclaration(module) => match &module.id {
            TSModuleDeclarationName::Identifier(id) => vec![id.name.as_str().to_string()],
            TSModuleDeclarationName::StringLiteral(s) => vec![s.value.as_str().to_string()],
        },
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
        let id = format!("symbol:{}#{}", file_path, symbol.name);
        let line = symbol.selection_range.start.line + 1;
        declarations.push(GraphDeclaration {
            id: id.clone(),
            name: symbol.name.clone(),
            kind: symbol_kind_name(symbol.kind),
            line,
            range: symbol_range(symbol),
            selection_range: symbol_selection_range(symbol),
            exported: export_names.iter().any(|name| name == &symbol.name),
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

fn collect_program_calls(program: &Program, li: &LineIndex, calls: &mut Vec<GraphCall>) {
    for stmt in &program.body {
        collect_statement_calls(stmt, li, calls);
    }
}

fn collect_statement_calls(stmt: &Statement, li: &LineIndex, calls: &mut Vec<GraphCall>) {
    match stmt {
        Statement::FunctionDeclaration(function) => {
            if let Some(id) = &function.id {
                collect_function_calls(id.name.as_str(), function, li, calls);
            }
        }
        Statement::ClassDeclaration(class) => collect_class_calls(class, li, calls),
        Statement::VariableDeclaration(variable) => collect_variable_calls(variable, li, calls),
        Statement::ExportNamedDeclaration(decl) => {
            if let Some(inner) = &decl.declaration {
                collect_declaration_calls(inner, li, calls);
            }
        }
        Statement::ExportDefaultDeclaration(decl) => match &decl.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                let owner = function
                    .id
                    .as_ref()
                    .map(|id| id.name.as_str())
                    .unwrap_or("default");
                collect_function_calls(owner, function, li, calls);
            }
            ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                collect_class_calls(class, li, calls);
            }
            _ => {}
        },
        _ => {}
    }
}

fn collect_declaration_calls(decl: &Declaration, li: &LineIndex, calls: &mut Vec<GraphCall>) {
    match decl {
        Declaration::FunctionDeclaration(function) => {
            if let Some(id) = &function.id {
                collect_function_calls(id.name.as_str(), function, li, calls);
            }
        }
        Declaration::ClassDeclaration(class) => collect_class_calls(class, li, calls),
        Declaration::VariableDeclaration(variable) => collect_variable_calls(variable, li, calls),
        _ => {}
    }
}

fn collect_class_calls(class: &Class, li: &LineIndex, calls: &mut Vec<GraphCall>) {
    for element in &class.body.body {
        if let ClassElement::MethodDefinition(method) = element {
            if let Some((name, _span)) = property_key_name(&method.key) {
                collect_function_calls(&name, &method.value, li, calls);
            }
        }
    }
}

fn collect_variable_calls(
    variable: &VariableDeclaration,
    li: &LineIndex,
    calls: &mut Vec<GraphCall>,
) {
    for declarator in &variable.declarations {
        let BindingPattern::BindingIdentifier(id) = &declarator.id else {
            continue;
        };
        match &declarator.init {
            Some(Expression::ArrowFunctionExpression(arrow)) => {
                for stmt in &arrow.body.statements {
                    collect_owned_statement_calls(id.name.as_str(), stmt, li, calls);
                }
            }
            Some(Expression::FunctionExpression(function)) => {
                collect_function_calls(id.name.as_str(), function, li, calls);
            }
            Some(expression) => collect_expression_calls(id.name.as_str(), expression, li, calls),
            None => {}
        }
    }
}

fn collect_function_calls(
    owner: &str,
    function: &Function,
    li: &LineIndex,
    calls: &mut Vec<GraphCall>,
) {
    if let Some(body) = &function.body {
        for stmt in &body.statements {
            collect_owned_statement_calls(owner, stmt, li, calls);
        }
    }
}

fn collect_owned_statement_calls(
    owner: &str,
    stmt: &Statement,
    li: &LineIndex,
    calls: &mut Vec<GraphCall>,
) {
    match stmt {
        Statement::BlockStatement(block) => {
            for child in &block.body {
                collect_owned_statement_calls(owner, child, li, calls);
            }
        }
        Statement::ExpressionStatement(expression) => {
            collect_expression_calls(owner, &expression.expression, li, calls);
        }
        Statement::ReturnStatement(ret) => {
            if let Some(argument) = &ret.argument {
                collect_expression_calls(owner, argument, li, calls);
            }
        }
        Statement::VariableDeclaration(variable) => {
            for declarator in &variable.declarations {
                if let Some(init) = &declarator.init {
                    collect_expression_calls(owner, init, li, calls);
                }
            }
        }
        Statement::IfStatement(if_stmt) => {
            collect_expression_calls(owner, &if_stmt.test, li, calls);
            collect_owned_statement_calls(owner, &if_stmt.consequent, li, calls);
            if let Some(alternate) = &if_stmt.alternate {
                collect_owned_statement_calls(owner, alternate, li, calls);
            }
        }
        Statement::ForStatement(for_stmt) => {
            if let Some(ForStatementInit::VariableDeclaration(variable)) = &for_stmt.init {
                for declarator in &variable.declarations {
                    if let Some(init) = &declarator.init {
                        collect_expression_calls(owner, init, li, calls);
                    }
                }
            }
            if let Some(test) = &for_stmt.test {
                collect_expression_calls(owner, test, li, calls);
            }
            if let Some(update) = &for_stmt.update {
                collect_expression_calls(owner, update, li, calls);
            }
            collect_owned_statement_calls(owner, &for_stmt.body, li, calls);
        }
        Statement::WhileStatement(while_stmt) => {
            collect_expression_calls(owner, &while_stmt.test, li, calls);
            collect_owned_statement_calls(owner, &while_stmt.body, li, calls);
        }
        Statement::DoWhileStatement(do_while) => {
            collect_owned_statement_calls(owner, &do_while.body, li, calls);
            collect_expression_calls(owner, &do_while.test, li, calls);
        }
        Statement::ThrowStatement(throw_stmt) => {
            collect_expression_calls(owner, &throw_stmt.argument, li, calls);
        }
        _ => {}
    }
}

fn collect_expression_calls(
    owner: &str,
    expr: &Expression,
    li: &LineIndex,
    calls: &mut Vec<GraphCall>,
) {
    match expr {
        Expression::CallExpression(call) => {
            if let Some(callee) = callee_name(&call.callee) {
                let range = li.range(call.span);
                let line = range.start.line + 1;
                calls.push(GraphCall {
                    id: format!("call:{}:{}:{}", owner, callee, line),
                    caller: owner.to_string(),
                    callee,
                    line,
                    range,
                    kind: "calls",
                });
            }
        }
        Expression::NewExpression(new_expr) => {
            if let Some(callee) = callee_name(&new_expr.callee) {
                let range = li.range(new_expr.span);
                let line = range.start.line + 1;
                calls.push(GraphCall {
                    id: format!("construct:{}:{}:{}", owner, callee, line),
                    caller: owner.to_string(),
                    callee,
                    line,
                    range,
                    kind: "constructs",
                });
            }
        }
        _ => {}
    }
}

fn callee_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier(identifier) => Some(identifier.name.as_str().to_string()),
        Expression::StaticMemberExpression(member) => {
            let property = member.property.name.as_str();
            Some(
                expression_name(&member.object)
                    .map(|object| format!("{object}.{property}"))
                    .unwrap_or_else(|| property.to_string()),
            )
        }
        _ => None,
    }
}

fn expression_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier(identifier) => Some(identifier.name.as_str().to_string()),
        Expression::ThisExpression(_) => Some("this".to_string()),
        _ => None,
    }
}

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
        Statement::TSModuleDeclaration(m) => push_opt(out, namespace_symbol(m, li)),
        Statement::TSTypeAliasDeclaration(t) => push_opt(out, type_alias_symbol(t, li)),
        Statement::ExportNamedDeclaration(e) => {
            if let Some(decl) = &e.declaration {
                collect_declaration(decl, li, out);
            }
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
        Declaration::TSModuleDeclaration(m) => push_opt(out, namespace_symbol(m, li)),
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

fn namespace_symbol(decl: &TSModuleDeclaration, li: &LineIndex) -> Option<DocumentSymbol> {
    let (name, name_span) = match &decl.id {
        TSModuleDeclarationName::Identifier(id) => (id.name.as_str().to_string(), id.span),
        TSModuleDeclarationName::StringLiteral(s) => (s.value.as_str().to_string(), s.span),
    };
    let mut children = Vec::new();
    if let Some(body) = &decl.body {
        match body {
            TSModuleDeclarationBody::TSModuleBlock(block) => {
                for stmt in &block.body {
                    collect_statement(stmt, li, &mut children);
                }
            }
            TSModuleDeclarationBody::TSModuleDeclaration(inner) => {
                push_opt(&mut children, namespace_symbol(inner, li));
            }
        }
    }
    Some(container(
        &name,
        kind::NAMESPACE,
        decl.span,
        name_span,
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
mod tests {
    use super::*;
    use serde_json::Value;

    fn symbols(content: &str, path: &str) -> Value {
        let json = extract_js_symbols(content, path).expect("symbols expected");
        serde_json::from_str(&json).expect("valid json")
    }

    fn graph(content: &str, path: &str) -> Value {
        let json = extract_graph_facts(content, path).expect("graph facts expected");
        serde_json::from_str(&json).expect("valid json")
    }

    fn names(value: &Value) -> Vec<String> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["name"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn extracts_functions_classes_and_members() {
        let src = "export function add(a: number, b: number): number {\n  return a + b;\n}\n\nexport class Calc {\n  value = 0;\n  multiply(x: number) {\n    return this.value * x;\n  }\n  constructor() {}\n}\n";
        let v = symbols(src, "calc.ts");
        let top = names(&v);
        assert!(top.contains(&"add".to_string()), "function: {top:?}");
        assert!(top.contains(&"Calc".to_string()), "class: {top:?}");

        let calc = v
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "Calc")
            .unwrap();
        assert_eq!(calc["kind"], 5, "class kind");
        let members = names(&calc["children"]);
        assert!(members.contains(&"value".to_string()), "field: {members:?}");
        assert!(
            members.contains(&"multiply".to_string()),
            "method: {members:?}"
        );
        assert!(
            members.contains(&"constructor".to_string()),
            "ctor: {members:?}"
        );
    }

    #[test]
    fn extracts_graph_facts_for_imports_exports_and_calls() {
        let src = "import { dep } from './dep';\nexport function run() {\n  dep();\n  helper();\n}\nfunction helper() {}\n";
        let v = graph(src, "main.ts");

        let declarations = v["declarations"].as_array().unwrap();
        let run = declarations.iter().find(|d| d["name"] == "run").unwrap();
        assert_eq!(run["kind"], "function");
        assert_eq!(run["exported"], true);

        let imports = v["imports"].as_array().unwrap();
        assert_eq!(imports[0]["specifier"], "./dep");
        assert_eq!(imports[0]["localName"], "dep");

        let calls = v["calls"].as_array().unwrap();
        let callees = calls
            .iter()
            .map(|call| call["callee"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(callees.contains(&"dep"), "callee list: {callees:?}");
        assert!(callees.contains(&"helper"), "callee list: {callees:?}");
    }

    #[test]
    fn extracts_interface_enum_typealias_namespace() {
        let src = "export interface User {\n  id: string;\n  greet(): void;\n}\n\nexport enum Color { Red, Green }\n\nexport type Id = string;\n\nexport namespace NS {\n  export function inner() {}\n}\n";
        let v = symbols(src, "types.ts");
        let top = names(&v);
        for expected in ["User", "Color", "Id", "NS"] {
            assert!(top.contains(&expected.to_string()), "{expected} in {top:?}");
        }
        let user = v
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "User")
            .unwrap();
        assert_eq!(user["kind"], 11, "interface kind");
        let members = names(&user["children"]);
        assert!(members.contains(&"id".to_string()));
        assert!(members.contains(&"greet".to_string()));

        let ns = v
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "NS")
            .unwrap();
        assert!(names(&ns["children"]).contains(&"inner".to_string()));
    }

    #[test]
    fn arrow_const_is_a_function_const_value_is_constant() {
        let src =
            "export const handler = (req) => req;\nexport const MAX = 10;\nlet counter = 0;\n";
        let v = symbols(src, "h.js");
        let arr = v.as_array().unwrap();
        let handler = arr.iter().find(|s| s["name"] == "handler").unwrap();
        assert_eq!(handler["kind"], 12, "arrow → function");
        let max = arr.iter().find(|s| s["name"] == "MAX").unwrap();
        assert_eq!(max["kind"], 14, "const → constant");
        let counter = arr.iter().find(|s| s["name"] == "counter").unwrap();
        assert_eq!(counter["kind"], 13, "let → variable");
    }

    #[test]
    fn ranges_are_zero_based() {
        let src = "function first() {}\nfunction second() {}\n";
        let v = symbols(src, "a.ts");
        let first = &v.as_array().unwrap()[0];
        assert_eq!(first["range"]["start"]["line"], 0, "0-based first line");
        let second = v
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "second")
            .unwrap();
        assert_eq!(second["range"]["start"]["line"], 1);
    }

    #[test]
    fn tsx_and_jsx_parse() {
        let src = "export function App() {\n  return <div>hi</div>;\n}\n";
        let v = symbols(src, "App.tsx");
        assert!(names(&v).contains(&"App".to_string()));
    }

    #[test]
    fn empty_or_dataless_returns_none() {
        assert!(extract_js_symbols("", "empty.ts").is_none());
        // A hard parse failure must not abort; it returns None or a best-effort
        // outline — either is acceptable, just never a panic.
        let _ = extract_js_symbols("const x = 1 +;", "broken.ts");
    }

    fn refs(content: &str, path: &str, line: u32, character: u32) -> Value {
        let json =
            find_in_file_references(content, path, line, character).expect("references expected");
        serde_json::from_str(&json).expect("valid json")
    }

    #[test]
    fn finds_in_file_references_from_declaration() {
        // `count` declared on line 0; used on lines 1 and 2.
        let src = "const count = 1;\nconst a = count + 1;\nconsole.log(count);\n";
        // Cursor on the declaration identifier `count` (line 0, char 6).
        let v = refs(src, "m.ts", 0, 6);
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 3, "declaration + 2 uses: {arr:?}");
        // First range is the declaration (line 0).
        assert_eq!(arr[0]["start"]["line"], 0);
        let lines: Vec<i64> = arr
            .iter()
            .map(|r| r["start"]["line"].as_i64().unwrap())
            .collect();
        assert!(lines.contains(&1) && lines.contains(&2), "uses: {lines:?}");
    }

    #[test]
    fn finds_references_from_a_use_site() {
        let src = "function greet(name) {\n  return name + name;\n}\n";
        // Cursor on a `name` use inside the body (line 1).
        let v = refs(src, "m.js", 1, 9);
        let arr = v.as_array().unwrap();
        assert!(arr.len() >= 2, "param + uses: {arr:?}");
    }

    #[test]
    fn references_none_off_symbol() {
        let src = "const x = 1;\n";
        // Cursor in whitespace / on a keyword, not a binding.
        assert!(find_in_file_references(src, "m.ts", 0, 0).is_none());
    }

    #[test]
    fn never_aborts_on_adversarial_input() {
        for src in [
            "function broken( { [ unterminated",
            "class { { { {",
            "\u{0}\u{0}\u{0}",
            "import type type from from",
        ] {
            let _ = extract_js_symbols(src, "x.ts");
        }
    }
}
