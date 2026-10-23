use super::*;
pub(crate) fn op(query: &LspSearchQuery) -> Op {
    use wire::{
        AnchoredHierarchyOperation as H, AnchoredLocationsOperation as L, DocumentOperation as D,
    };
    match query {
        LspSearchQuery::AnchoredLocations(query) => match query.operation {
            L::Definition => Op::Definition,
            L::TypeDefinition => Op::TypeDefinition,
            L::Implementation => Op::Implementation,
        },
        LspSearchQuery::AnchoredReferences(query) => match query.operation {
            wire::AnchoredReferencesOperation::References => Op::References,
        },
        LspSearchQuery::AnchoredCallers(query) => match query.operation {
            wire::AnchoredCallersOperation::Callers => Op::Callers,
        },
        LspSearchQuery::AnchoredHierarchy(query) => match query.operation {
            H::Callees => Op::Callees,
            H::Supertypes => Op::Supertypes,
        },
        LspSearchQuery::AnchoredSubtypes(query) => match query.operation {
            wire::AnchoredSubtypesOperation::Subtypes => Op::Subtypes,
        },
        LspSearchQuery::AnchoredHover(query) => match query.operation {
            wire::AnchoredHoverOperation::Hover => Op::Hover,
        },
        LspSearchQuery::Document(query) => match query.operation {
            D::DocumentSymbols => Op::DocumentSymbols,
            D::Diagnostic => Op::Diagnostic,
        },
        LspSearchQuery::WorkspacePath(_) | LspSearchQuery::WorkspaceRoot(_) => Op::WorkspaceSymbol,
    }
}
/// The operation's wire spelling.
pub(crate) fn operation(query: &LspSearchQuery) -> &'static str {
    op(query).name()
}
/// The file the query names (a path or `file://` URI).
pub(crate) fn path(query: &LspSearchQuery) -> Option<&str> {
    if let LspSearchQuery::WorkspaceRoot(query) = query {
        return query.path.as_deref();
    }
    anchored_shape!(query, path => Some(path.as_str()), else => match query {
        LspSearchQuery::Document(query) => Some(query.path.as_str()),
        LspSearchQuery::WorkspacePath(query) => Some(query.path.as_str()),
        _ => None,
    })
}
pub(crate) fn workspace_root(query: &LspSearchQuery) -> Option<&str> {
    if let LspSearchQuery::WorkspaceRoot(query) = query {
        return Some(query.workspace_root.as_str());
    }
    anchored_shape!(query, workspace_root => workspace_root.as_deref(), else => match query {
        LspSearchQuery::Document(query) => query.workspace_root.as_deref(),
        LspSearchQuery::WorkspacePath(query) => query.workspace_root.as_deref(),
        _ => None,
    })
}
pub(crate) fn symbol_name(query: &LspSearchQuery) -> Option<&str> {
    anchored_shape!(query, symbol_name => Some(symbol_name.as_str()), else => match query {
        LspSearchQuery::WorkspacePath(query) => Some(query.symbol_name.as_str()),
        LspSearchQuery::WorkspaceRoot(query) => Some(query.symbol_name.as_str()),
        _ => None,
    })
}
pub(crate) fn anchor_line(query: &LspSearchQuery) -> Option<u32> {
    anchored_shape!(query, line => Some(u32_of(line.get())), else => None)
}
/// The occurrence index; the default (first occurrence) reads as unset.
pub(crate) fn order_hint(query: &LspSearchQuery) -> Option<u32> {
    anchored_shape!(query, order_hint => (*order_hint != 0).then(|| u32_of_signed(*order_hint)), else => None)
}
pub(crate) fn include_declaration(query: &LspSearchQuery) -> Option<bool> {
    match query {
        LspSearchQuery::AnchoredReferences(query) => query.include_declaration,
        _ => None,
    }
}
pub(crate) fn group_by_file(query: &LspSearchQuery) -> Option<bool> {
    match query {
        LspSearchQuery::AnchoredLocations(query) => query.group_by_file,
        LspSearchQuery::AnchoredReferences(query) => query.group_by_file,
        _ => None,
    }
}
pub(crate) fn graph_depth(query: &LspSearchQuery) -> Option<u32> {
    match query {
        LspSearchQuery::AnchoredCallers(query) => {
            query.graph_depth.map(|depth| u32_of(depth.get()))
        }
        LspSearchQuery::AnchoredHierarchy(query) => {
            query.graph_depth.map(|depth| u32_of(depth.get()))
        }
        LspSearchQuery::AnchoredSubtypes(query) => {
            query.graph_depth.map(|depth| u32_of(depth.get()))
        }
        _ => None,
    }
}
pub(crate) fn page(query: &LspSearchQuery) -> Option<u32> {
    each_shape!(query, page => Some(u32_of(page.get())))
}
/// Rows per page when the caller sets `pageSize`; unset pages fill the
/// response window instead.
pub(crate) fn page_size(query: &LspSearchQuery) -> Option<u32> {
    each_shape!(query, page_size => page_size.as_ref().map(|size| u32_of(size.get())))
}
/// The importer window (`importerPage`, one-based) of an anchored query.
pub(crate) fn importer_page(query: &LspSearchQuery) -> u32 {
    match query {
        LspSearchQuery::AnchoredReferences(query) => {
            query.importer_page.map_or(1, |page| u32_of(page.get()))
        }
        LspSearchQuery::AnchoredCallers(query) => {
            query.importer_page.map_or(1, |page| u32_of(page.get()))
        }
        _ => 1,
    }
}
/// The window of the anchor type's implementation candidates
/// (`candidatePage`, one-based) a fallback `subtypes` walk checks.
pub(crate) fn candidate_page(query: &LspSearchQuery) -> u32 {
    match query {
        LspSearchQuery::AnchoredSubtypes(query) => {
            query.candidate_page.map_or(1, |page| u32_of(page.get()))
        }
        _ => 1,
    }
}
pub(crate) fn snapshot(query: &LspSearchQuery) -> Option<&str> {
    each_shape!(query, snapshot => snapshot.as_ref().map(|snapshot| snapshot.as_str()))
}
pub(crate) fn context_lines(query: &LspSearchQuery) -> Option<u32> {
    match query {
        LspSearchQuery::AnchoredLocations(query) => query.context_lines.map(u32_of_signed),
        LspSearchQuery::AnchoredReferences(query) => query.context_lines.map(u32_of_signed),
        _ => None,
    }
}
/// The Rust build context as JSON, decoded by [`receipt`].
pub(crate) fn rust_context(query: &LspSearchQuery) -> Option<Value> {
    each_shape!(query, rust_context => rust_context
        .as_ref()
        .and_then(|context| serde_json::to_value(context).ok()))
}
/// The query as a replayable JSON row.
pub(crate) fn to_row(query: &LspSearchQuery) -> Value {
    serde_json::to_value(query).unwrap_or_else(|_| serde_json::json!({}))
}
