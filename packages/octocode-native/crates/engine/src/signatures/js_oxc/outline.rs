//! The declaration outline: document symbols flattened into graph-facts
//! declarations, with doc lines, containment and heritage edges.

use oxc_ast::ast::Program;

use crate::deep_stack::job_cancelled;

use crate::graph::{GraphFactDeclaration, GraphFactEdge, GraphFactExport, GraphFactImport};
use crate::signatures::js_oxc_shared::SpanPositions;

use super::module_facts::collect_module_facts;
use super::{DocumentSymbol, LocalExports, kind};

/// Declarations (with export names, doc lines, and containment edges) plus
/// the import and export facts of a parsed module, from its `symbols`.
pub(super) fn module_outline(
    program: &Program,
    line_index: &SpanPositions,
    file_path: &str,
    content: &str,
    symbols: &[DocumentSymbol],
) -> (
    Vec<GraphFactDeclaration>,
    Vec<GraphFactEdge>,
    Vec<GraphFactImport>,
    Vec<GraphFactExport>,
) {
    let mut local_exports = LocalExports::default();
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    collect_module_facts(
        program,
        line_index,
        &mut imports,
        &mut exports,
        &mut local_exports,
    );
    let mut declarations = Vec::new();
    let mut edges = Vec::new();
    flatten_symbols(
        file_path,
        symbols,
        None,
        &local_exports,
        &mut declarations,
        &mut edges,
    );
    attach_doc_lines(content, &mut declarations);
    (declarations, edges, imports, exports)
}

fn attach_doc_lines(content: &str, declarations: &mut [GraphFactDeclaration]) {
    let lines = content.lines().collect::<Vec<_>>();
    for declaration in declarations {
        if job_cancelled() {
            return;
        }
        declaration.doc_line = crate::signatures::leading_doc_line(
            &lines,
            declaration.range.start.line as usize,
            "ts",
        );
    }
}

/// `extends`/`implements` edges from a class/interface declaration id to the
/// base type name as written (`Base`, `ns.Base`). Syntax only: `to` is not a
/// resolved declaration. A clause whose declaring name has no outline
/// declaration (a class nested in a function body) is dropped.
pub(super) fn push_heritage_edges(
    declarations: &[GraphFactDeclaration],
    heritage: Vec<crate::signatures::js_oxc_calls::GraphHeritage>,
    edges: &mut Vec<GraphFactEdge>,
) {
    if heritage.is_empty() {
        return;
    }
    let by_name_start: std::collections::HashMap<(u32, u32, &str), &str> = declarations
        .iter()
        .map(|declaration| {
            (
                (
                    declaration.selection_range.start.line,
                    declaration.selection_range.start.character,
                    declaration.kind.as_str(),
                ),
                declaration.id.as_str(),
            )
        })
        .collect();
    for item in heritage {
        let key = (
            item.name_start.line,
            item.name_start.character,
            item.declaration_kind,
        );
        let Some(from) = by_name_start.get(&key) else {
            continue;
        };
        edges.push(GraphFactEdge {
            id: format!(
                "{from}->{}:{}:{}:{}",
                item.to,
                item.relation,
                item.line,
                edges.len()
            ),
            from: (*from).to_string(),
            to: item.to,
            relation: item.relation.to_owned(),
            source: "oxc".to_owned(),
            line: item.line,
            resolution: "syntax".to_owned(),
        });
    }
}

fn flatten_symbols(
    file_path: &str,
    symbols: &[DocumentSymbol],
    parent: Option<&str>,
    local_exports: &LocalExports,
    declarations: &mut Vec<GraphFactDeclaration>,
    edges: &mut Vec<GraphFactEdge>,
) {
    for symbol in symbols {
        if job_cancelled() {
            return;
        }
        let id = format!(
            "declaration:{}#{}@{}:{}:{}",
            file_path,
            symbol.name,
            symbol.selection_range.start.line,
            symbol.selection_range.start.character,
            symbol_kind_name(symbol.kind)
        );
        let line = symbol.selection_range.start.line + 1;
        let public_names = parent
            .is_none()
            .then(|| local_exports.public_names(&symbol.name))
            .flatten();
        let exported_as = match &public_names {
            Some(names) if names.iter().any(|public| public != &symbol.name) => names.clone(),
            _ => Vec::new(),
        };
        declarations.push(GraphFactDeclaration {
            id: id.clone(),
            name: symbol.name.clone(),
            kind: symbol_kind_name(symbol.kind).to_owned(),
            line,
            range: symbol.range.clone(),
            selection_range: symbol.selection_range.clone(),
            exported: public_names.is_some(),
            exported_as,
            parent: parent.map(str::to_string),
            doc_line: None,
        });
        if let Some(parent_id) = parent {
            edges.push(GraphFactEdge {
                id: format!("{}->{}:contains", parent_id, id),
                from: parent_id.to_string(),
                to: id.clone(),
                relation: "contains".to_owned(),
                source: "ast".to_owned(),
                line,
                resolution: "syntactic".to_owned(),
            });
        }
        flatten_symbols(
            file_path,
            &symbol.children,
            Some(&id),
            local_exports,
            declarations,
            edges,
        );
    }
}

pub(super) fn symbol_kind_name(kind: u8) -> &'static str {
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
        kind::TYPE_ALIAS => "type",
        _ => "symbol",
    }
}
