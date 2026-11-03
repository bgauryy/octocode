use super::*;
use crate::tools::variant_field::variant_field;
use octocode_contract_types::{
    AnchoredCallers, AnchoredHierarchy, AnchoredHover, AnchoredLocations, AnchoredLocationsFiles,
    AnchoredReferences, AnchoredReferencesFiles, AnchoredSubtypes, LspDocument,
    LspDocumentOperation, LspSearchQuery as Q, LspWorkspacePath, LspWorkspacePathOperation,
    LspWorkspaceRoot,
};

/// Binds `$field` from whichever anchored shape (a symbol at a line) `$query`
/// is; `$other` answers for the file and workspace shapes.
macro_rules! anchored_shape {
    ($query:expr, $field:ident => $value:expr, else => $other:expr) => {
        variant_field!($query, newtype [
            Q::AnchoredLocations => AnchoredLocations, Q::AnchoredLocationsFiles => AnchoredLocationsFiles,
            Q::AnchoredReferences => AnchoredReferences, Q::AnchoredReferencesFiles => AnchoredReferencesFiles,
            Q::AnchoredCallers => AnchoredCallers, Q::AnchoredHierarchy => AnchoredHierarchy,
            Q::AnchoredSubtypes => AnchoredSubtypes, Q::AnchoredHover => AnchoredHover,
        ] $field => $value, else => $other)
    };
}

/// Binds `$field` from whichever shape `$query` is.
macro_rules! each_shape {
    ($query:expr, $field:ident => $value:expr) => {
        variant_field!($query, newtype [
            Q::AnchoredLocations => AnchoredLocations, Q::AnchoredLocationsFiles => AnchoredLocationsFiles,
            Q::AnchoredReferences => AnchoredReferences, Q::AnchoredReferencesFiles => AnchoredReferencesFiles,
            Q::AnchoredCallers => AnchoredCallers, Q::AnchoredHierarchy => AnchoredHierarchy,
            Q::AnchoredSubtypes => AnchoredSubtypes, Q::AnchoredHover => AnchoredHover,
            Q::LspDocument => LspDocument, Q::LspWorkspacePath => LspWorkspacePath,
            Q::LspWorkspaceRoot => LspWorkspaceRoot,
        ] $field => $value)
    };
}

pub(crate) fn op(query: &Q) -> Op {
    use octocode_contract_types::*;
    match query {
        Q::AnchoredLocations(query) => match query.operation {
            AnchoredLocationsOperation::Definition => Op::Definition,
            AnchoredLocationsOperation::TypeDefinition => Op::TypeDefinition,
            AnchoredLocationsOperation::Implementation => Op::Implementation,
        },
        Q::AnchoredLocationsFiles(query) => match query.operation {
            AnchoredLocationsFilesOperation::Definition => Op::Definition,
            AnchoredLocationsFilesOperation::TypeDefinition => Op::TypeDefinition,
            AnchoredLocationsFilesOperation::Implementation => Op::Implementation,
        },
        Q::AnchoredReferences(query) => match query.operation {
            AnchoredReferencesOperation::References => Op::References,
        },
        Q::AnchoredReferencesFiles(query) => match query.operation {
            AnchoredReferencesFilesOperation::References => Op::References,
        },
        Q::AnchoredCallers(query) => match query.operation {
            AnchoredCallersOperation::Callers => Op::Callers,
        },
        Q::AnchoredHierarchy(query) => match query.operation {
            AnchoredHierarchyOperation::Callees => Op::Callees,
            AnchoredHierarchyOperation::Supertypes => Op::Supertypes,
        },
        Q::AnchoredSubtypes(query) => match query.operation {
            AnchoredSubtypesOperation::Subtypes => Op::Subtypes,
        },
        Q::AnchoredHover(query) => match query.operation {
            AnchoredHoverOperation::Hover => Op::Hover,
        },
        Q::LspDocument(query) => match query.operation {
            LspDocumentOperation::DocumentSymbols => Op::DocumentSymbols,
            LspDocumentOperation::Diagnostic => Op::Diagnostic,
        },
        Q::LspWorkspacePath(_) | Q::LspWorkspaceRoot(_) => Op::WorkspaceSymbol,
    }
}
pub(crate) fn operation(q: &Q) -> &'static str {
    op(q).name()
}
pub(crate) fn path(q: &Q) -> Option<&str> {
    anchored_shape!(q, path => Some(path.as_str()), else => variant_field!(q,
        newtype [Q::LspDocument => LspDocument, Q::LspWorkspacePath => LspWorkspacePath] path => Some(path.as_str()), else => None))
}
pub(crate) fn workspace_root(q: &Q) -> Option<&str> {
    anchored_shape!(q, workspace_root => workspace_root.as_ref().map(|value| value.as_str()),
        else => variant_field!(q, newtype [Q::LspDocument => LspDocument, Q::LspWorkspacePath => LspWorkspacePath]
            workspace_root => workspace_root.as_ref().map(|value| value.as_str()),
            else => variant_field!(q, newtype [Q::LspWorkspaceRoot => LspWorkspaceRoot]
                workspace_root => Some(workspace_root.as_str()), else => None)))
}
pub(crate) fn symbol_name(q: &Q) -> Option<&str> {
    anchored_shape!(q, symbol_name => Some(symbol_name.as_str()), else => None)
}
pub(crate) fn symbol_query(q: &Q) -> Option<&str> {
    variant_field!(q, newtype [Q::LspWorkspacePath => LspWorkspacePath, Q::LspWorkspaceRoot => LspWorkspaceRoot]
        symbol_query => Some(symbol_query.as_str()), else => None)
}
pub(crate) fn anchor_line(q: &Q) -> Option<u32> {
    anchored_shape!(q, line => Some(saturating_u32(line.get())), else => None)
}
pub(crate) fn occurrence(q: &Q) -> Option<u32> {
    anchored_shape!(q, occurrence => (*occurrence != 0).then(|| u32_of_signed(*occurrence)),
        else => None)
}
pub(crate) fn include_declaration(q: &Q) -> Option<bool> {
    variant_field!(q, each [Q::AnchoredReferences, Q::AnchoredReferencesFiles]
        query => Some(query.include_declaration), else => None)
}
/// The `testCode` scope of `references` and `callers`; `None` elsewhere.
pub(crate) fn test_code(q: &Q) -> Option<octocode_contract_types::TestCode> {
    variant_field!(q, each [Q::AnchoredReferences, Q::AnchoredReferencesFiles, Q::AnchoredCallers]
        query => query.test_code, else => None)
}
pub(crate) fn view(q: &Q) -> &'static str {
    if matches!(
        q,
        Q::AnchoredLocationsFiles(_) | Q::AnchoredReferencesFiles(_)
    ) {
        "files"
    } else {
        "content"
    }
}
pub(crate) fn depth(q: &Q) -> Option<u32> {
    variant_field!(q, each [Q::AnchoredCallers, Q::AnchoredHierarchy, Q::AnchoredSubtypes]
        query => Some(saturating_u32(query.depth.get())), else => None)
}
/// Rows per page (`limit`); `None` cuts pages by the budget alone.
pub(crate) fn limit(q: &Q) -> Option<usize> {
    each_shape!(q, limit => limit.map(|limit| octocode_engine::num::saturating_usize(limit.get())))
}
pub(crate) fn page(q: &Q) -> Option<u32> {
    each_shape!(q, page => Some(saturating_u32(page.get())))
}
pub(crate) fn importer_page(q: &Q) -> u32 {
    variant_field!(q, each [Q::AnchoredReferences, Q::AnchoredReferencesFiles, Q::AnchoredCallers]
        query => query.importer_page.map_or(1, |page| saturating_u32(page.get())), else => 1)
}
pub(crate) fn candidate_page(q: &Q) -> u32 {
    variant_field!(q, each [Q::AnchoredSubtypes]
        query => query.candidate_page.map_or(1, |page| saturating_u32(page.get())), else => 1)
}
pub(crate) fn snapshot(q: &Q) -> Option<&str> {
    each_shape!(q, snapshot => snapshot.as_ref().map(|value| value.as_str()))
}
pub(crate) fn context_lines(q: &Q) -> Option<octocode_engine::types::SearchContext> {
    variant_field!(q, each [Q::AnchoredLocations, Q::AnchoredReferences] query => query.context_lines.as_ref().map(
        |lines| octocode_engine::types::SearchContext {
            before: u32_of_signed(lines.before),
            after: u32_of_signed(lines.after),
        },
    ), else => None)
}
pub(crate) fn rust_context(q: &Q) -> Option<&octocode_contract_types::LspRustContext> {
    each_shape!(q, rust_context => rust_context.as_ref())
}
/// `q` with `changes` applied (a `null` change drops that field): the
/// request a continuation or lead runs. The edited fields are chosen at run
/// time and differ by shape (`page`, `importerPage`, `candidatePage`,
/// `snapshot`, the anchor), so a shape that lacks one is an error.
pub(crate) fn edited(q: &Q, changes: &Value) -> Result<Q, serde_json::Error> {
    crate::response::result::edited_query(q, changes)
}

/// A fresh `operation` lead over the document at `path`, in the workspace
/// (`workspaceRoot`, `rustContext`) `workspace` ran in (none: the default
/// one). `None` when `path` or the workspace root is not a representable
/// value.
pub(crate) fn document_lead(
    operation: LspDocumentOperation,
    path: &str,
    workspace: Option<&Q>,
) -> Option<Q> {
    Some(Q::LspDocument(LspDocument {
        limit: None,
        main_goal: None,
        operation,
        page: std::num::NonZeroU64::MIN,
        path: path.try_into().ok()?,
        reasoning: None,
        rust_context: workspace.and_then(rust_context).cloned(),
        snapshot: None,
        workspace_root: workspace
            .and_then(workspace_root)
            .map(TryInto::try_into)
            .transpose()
            .ok()?,
    }))
}

/// A fresh `workspaceSymbol` lead for `term` in the project of the file at
/// `path`, in the workspace `workspace` ran in (none: the default one).
/// `None` when a value is not representable.
pub(crate) fn workspace_symbol_lead(term: &str, path: &str, workspace: Option<&Q>) -> Option<Q> {
    Some(Q::LspWorkspacePath(LspWorkspacePath {
        limit: None,
        main_goal: None,
        operation: LspWorkspacePathOperation::WorkspaceSymbol,
        page: std::num::NonZeroU64::MIN,
        path: path.try_into().ok()?,
        reasoning: None,
        rust_context: workspace.and_then(rust_context).cloned(),
        snapshot: None,
        symbol_query: term.try_into().ok()?,
        workspace_root: workspace
            .and_then(workspace_root)
            .map(TryInto::try_into)
            .transpose()
            .ok()?,
    }))
}

/// The query as a row, without the fields at their schema default: a lead
/// or saved progress names only what the agent set, and the defaults
/// re-apply on parse.
pub(crate) fn to_row(q: &Q) -> Value {
    let mut row = serde_json::to_value(q).unwrap_or_else(|_| json!({}));
    if let Some(fields) = row.as_object_mut() {
        for (key, default) in [
            ("occurrence", json!(0)),
            ("view", json!("content")),
            ("contextLines", json!({"before": 0, "after": 0})),
            ("includeDeclaration", json!(true)),
            ("depth", json!(1)),
        ] {
            if fields.get(key) == Some(&default) {
                fields.remove(key);
            }
        }
    }
    row
}
