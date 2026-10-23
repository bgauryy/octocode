//! Module-level facts: imports, exports and the bindings they name.

use oxc_ast::ast::{
    BindingPattern, Declaration, ExportAllDeclaration, ExportDeclaration,
    ExportDefaultDeclarationKind, ExportSpecifier, Expression, ImportDeclaration,
    ImportDeclarationSpecifier, ImportOrExportKind, Program, Statement, TSModuleReference,
};
use oxc_span::GetSpan;

use crate::graph::{GraphFactExport, GraphFactImport};
use crate::signatures::js_oxc_shared::{SpanPositions, module_export_name};

use super::LocalExports;

pub(super) fn collect_module_facts(
    program: &Program,
    li: &SpanPositions,
    imports: &mut Vec<GraphFactImport>,
    exports: &mut Vec<GraphFactExport>,
    local_exports: &mut LocalExports,
) {
    for stmt in &program.body {
        match stmt {
            Statement::ImportDeclaration(decl) => collect_import_declaration(decl, li, imports),
            Statement::ExportDeclaration(decl) => {
                collect_export_declaration(decl, li, exports, local_exports);
            }
            Statement::ExportNamedDeclaration(decl) => {
                collect_export_specifiers(
                    &decl.specifiers,
                    decl.export_kind,
                    None,
                    li,
                    exports,
                    Some(local_exports),
                );
            }
            Statement::ExportFromDeclaration(decl) => {
                collect_export_specifiers(
                    &decl.specifiers,
                    decl.export_kind,
                    Some(decl.source.value.as_str()),
                    li,
                    exports,
                    None,
                );
            }
            Statement::ExportDefaultDeclaration(decl) => {
                let range = li.range(decl.span);
                // The public name is always `default`; `local_name` is the
                // binding it exports, which `import x from` resolves to.
                let local_name = match &decl.declaration {
                    ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                        function.id.as_ref().map(|id| id.name.as_str().to_string())
                    }
                    ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                        class.id.as_ref().map(|id| id.name.as_str().to_string())
                    }
                    ExportDefaultDeclarationKind::Identifier(ident) => {
                        Some(ident.name.as_str().to_string())
                    }
                    _ => None,
                };
                if let Some(local) = &local_name {
                    local_exports.push(local.clone(), "default".to_string());
                }
                exports.push(GraphFactExport {
                    id: format!("export:default:{}", range.start.line + 1),
                    name: "default".to_string(),
                    line: range.start.line + 1,
                    export_kind: "value".to_owned(),
                    local_name,
                    source: None,
                });
            }
            Statement::ExportAllDeclaration(decl) => {
                collect_export_all(decl, li, exports);
            }
            // `export = expr;` (TS/CommonJS interop). Previously dropped, so a
            // module's single export vanished from the facts.
            Statement::TSExportAssignment(decl) => {
                let range = li.range(decl.span);
                let name = match &decl.expression {
                    Expression::Identifier(ident) => ident.name.as_str().to_string(),
                    _ => "export=".to_string(),
                };
                if matches!(&decl.expression, Expression::Identifier(_)) {
                    local_exports.push(name.clone(), name.clone());
                }
                exports.push(GraphFactExport {
                    id: format!("export:{}:{}", name, range.start.line + 1),
                    name,
                    line: range.start.line + 1,
                    export_kind: "value".to_owned(),
                    local_name: None,
                    source: None,
                });
            }
            // `import x = require('mod');` / `import x = ns.member;`. Previously
            // dropped, so the dependency edge was missing.
            Statement::TSImportEqualsDeclaration(decl) => {
                let range = li.range(decl.span);
                let local_name = decl.id.name.as_str().to_string();
                let specifier = match &decl.module_reference {
                    TSModuleReference::ExternalModuleReference(reference) => {
                        reference.expression.value.as_str().to_string()
                    }
                    TSModuleReference::IdentifierReference(reference) => {
                        reference.name.as_str().to_string()
                    }
                    TSModuleReference::QualifiedName(_) => local_name.clone(),
                };
                imports.push(GraphFactImport {
                    id: format!("import:{}:{}", local_name, range.start.line + 1),
                    specifier,
                    line: range.start.line + 1,
                    import_kind: import_export_kind(decl.import_kind).to_owned(),
                    local_name: Some(local_name),
                    imported_name: None,
                    imported_range: None,
                    local_range: Some(li.range(decl.id.span)),
                    resolution_hint: None,
                    module_scope: None,
                });
            }
            _ => {}
        }
    }
}

fn collect_import_declaration(
    decl: &ImportDeclaration,
    li: &SpanPositions,
    out: &mut Vec<GraphFactImport>,
) {
    let range = li.range(decl.span);
    let line = range.start.line + 1;
    let specifier = decl.source.value.as_str().to_string();
    if let Some(specifiers) = &decl.specifiers {
        for (index, item) in specifiers.iter().enumerate() {
            let (local_name, imported_name, import_kind, imported_range, local_range) = match item {
                ImportDeclarationSpecifier::ImportSpecifier(spec) => (
                    Some(spec.local.name.as_str().to_string()),
                    module_export_name(&spec.imported),
                    import_export_kind(if decl.import_kind == ImportOrExportKind::Type {
                        decl.import_kind
                    } else {
                        spec.import_kind
                    }),
                    Some(li.range(spec.imported.span())),
                    Some(li.range(spec.local.span)),
                ),
                ImportDeclarationSpecifier::ImportDefaultSpecifier(spec) => (
                    Some(spec.local.name.as_str().to_string()),
                    Some("default".to_string()),
                    import_export_kind(decl.import_kind),
                    None,
                    Some(li.range(spec.local.span)),
                ),
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(spec) => (
                    Some(spec.local.name.as_str().to_string()),
                    Some("*".to_string()),
                    import_export_kind(decl.import_kind),
                    None,
                    Some(li.range(spec.local.span)),
                ),
            };
            out.push(GraphFactImport {
                id: format!("import:{}:{}:{}", specifier, line, index),
                specifier: specifier.clone(),
                line,
                import_kind: import_kind.to_owned(),
                local_name,
                imported_name,
                imported_range,
                local_range,
                resolution_hint: None,
                module_scope: None,
            });
        }
    } else {
        out.push(GraphFactImport {
            id: format!("import:{}:{}", specifier, line),
            specifier,
            line,
            import_kind: import_export_kind(decl.import_kind).to_owned(),
            local_name: None,
            imported_name: None,
            imported_range: None,
            local_range: None,
            resolution_hint: None,
            module_scope: None,
        });
    }
}

fn collect_export_declaration(
    decl: &ExportDeclaration,
    li: &SpanPositions,
    out: &mut Vec<GraphFactExport>,
    local_exports: &mut LocalExports,
) {
    let export_kind = match &decl.declaration {
        Declaration::TSInterfaceDeclaration(_) | Declaration::TSTypeAliasDeclaration(_) => "type",
        _ => "value",
    };
    for name in declaration_names(&decl.declaration) {
        let range = li.range(decl.span);
        local_exports.push(name.clone(), name.clone());
        out.push(GraphFactExport {
            id: format!("export:{}:{}", name, range.start.line + 1),
            name,
            line: range.start.line + 1,
            export_kind: export_kind.to_owned(),
            local_name: None,
            source: None,
        });
    }
}

fn collect_export_specifiers(
    specifiers: &[ExportSpecifier],
    export_kind: ImportOrExportKind,
    source: Option<&str>,
    li: &SpanPositions,
    out: &mut Vec<GraphFactExport>,
    mut local_exports: Option<&mut LocalExports>,
) {
    for (index, specifier) in specifiers.iter().enumerate() {
        let name = module_export_name(&specifier.exported)
            .or_else(|| module_export_name(&specifier.local))
            .unwrap_or_else(|| "unknown".to_string());
        // `export { local as public }` exports the local binding; the flag
        // belongs to `local`, the public name is metadata (`exportedAs`).
        if let (Some(exports), Some(local)) = (
            local_exports.as_deref_mut(),
            module_export_name(&specifier.local),
        ) {
            exports.push(local, name.clone());
        }
        let range = li.range(specifier.span);
        out.push(GraphFactExport {
            id: format!("export:{}:{}:{}", name, range.start.line + 1, index),
            name,
            line: range.start.line + 1,
            export_kind: if export_kind == ImportOrExportKind::Type {
                "type".to_owned()
            } else {
                import_export_kind(specifier.export_kind).to_owned()
            },
            local_name: module_export_name(&specifier.local),
            source: source.map(str::to_owned),
        });
    }
}

fn collect_export_all(
    decl: &ExportAllDeclaration,
    li: &SpanPositions,
    out: &mut Vec<GraphFactExport>,
) {
    let range = li.range(decl.span);
    let name = decl
        .exported
        .as_ref()
        .and_then(module_export_name)
        .unwrap_or_else(|| "*".to_string());
    out.push(GraphFactExport {
        id: format!("export:{}:{}", name, range.start.line + 1),
        name,
        line: range.start.line + 1,
        export_kind: import_export_kind(decl.export_kind).to_owned(),
        local_name: None,
        source: Some(decl.source.value.as_str().to_string()),
    });
}

/// Collect every bound identifier from a binding pattern, recursing through
/// object/array destructuring, rest elements, and default-valued bindings. A
/// bare identifier yields one name; `{a, b: {c}, ...rest}` yields `a`, `c`,
/// `rest`; `[x, [y]]` yields `x`, `y`.
fn collect_binding_names(pattern: &BindingPattern, out: &mut Vec<String>) {
    match pattern {
        BindingPattern::BindingIdentifier(id) => out.push(id.name.as_str().to_string()),
        BindingPattern::ObjectPattern(object) => {
            for property in &object.properties {
                collect_binding_names(&property.value, out);
            }
            if let Some(rest) = &object.rest {
                collect_binding_names(&rest.argument, out);
            }
        }
        BindingPattern::ArrayPattern(array) => {
            for element in array.elements.iter().flatten() {
                collect_binding_names(element, out);
            }
            if let Some(rest) = &array.rest {
                collect_binding_names(&rest.argument, out);
            }
        }
        BindingPattern::AssignmentPattern(assignment) => {
            collect_binding_names(&assignment.left, out);
        }
    }
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
        Declaration::VariableDeclaration(variable) => {
            let mut names = Vec::new();
            for declarator in &variable.declarations {
                collect_binding_names(&declarator.id, &mut names);
            }
            names
        }
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

fn import_export_kind(kind: ImportOrExportKind) -> &'static str {
    match kind {
        ImportOrExportKind::Type => "type",
        ImportOrExportKind::Value => "value",
    }
}
