use super::*;
pub(crate) fn op(query: &LspSearchQuery) -> Op {
    use octocode_contract_types::*;
    match query {
        LspSearchQuery::Variant0(query) => match query.operation {
            AnchoredLocationsOperation::Definition => Op::Definition,
            AnchoredLocationsOperation::TypeDefinition => Op::TypeDefinition,
            AnchoredLocationsOperation::Implementation => Op::Implementation,
        },
        LspSearchQuery::Variant1(query) => match query.operation {
            AnchoredLocationsFilesOperation::Definition => Op::Definition,
            AnchoredLocationsFilesOperation::TypeDefinition => Op::TypeDefinition,
            AnchoredLocationsFilesOperation::Implementation => Op::Implementation,
        },
        LspSearchQuery::Variant2(query) => match query.operation {
            AnchoredReferencesOperation::References => Op::References,
        },
        LspSearchQuery::Variant3(query) => match query.operation {
            AnchoredReferencesFilesOperation::References => Op::References,
        },
        LspSearchQuery::Variant4(query) => match query.operation {
            AnchoredCallersOperation::Callers => Op::Callers,
        },
        LspSearchQuery::Variant5(query) => match query.operation {
            AnchoredHierarchyOperation::Callees => Op::Callees,
            AnchoredHierarchyOperation::Supertypes => Op::Supertypes,
        },
        LspSearchQuery::Variant6(query) => match query.operation {
            AnchoredSubtypesOperation::Subtypes => Op::Subtypes,
        },
        LspSearchQuery::Variant7(query) => match query.operation {
            AnchoredHoverOperation::Hover => Op::Hover,
        },
        LspSearchQuery::Variant8 { operation, .. } => match operation {
            LspSearchQueryVariant8Operation::DocumentSymbols => Op::DocumentSymbols,
            LspSearchQueryVariant8Operation::Diagnostic => Op::Diagnostic,
        },
        LspSearchQuery::Variant9 { .. } | LspSearchQuery::Variant10 { .. } => Op::WorkspaceSymbol,
    }
}
pub(crate) fn operation(q: &LspSearchQuery) -> &'static str {
    op(q).name()
}
pub(crate) fn path(q: &LspSearchQuery) -> Option<&str> {
    anchored_shape!(q,path=>Some(path.as_str()),else=>match q {LspSearchQuery::Variant8{path,..}=>Some(path.as_str()),LspSearchQuery::Variant9{path,..}=>Some(path.as_str()),_=>None})
}
pub(crate) fn workspace_root(q: &LspSearchQuery) -> Option<&str> {
    anchored_shape!(q,workspace_root=>workspace_root.as_ref().map(|value|value.as_str()),else=>match q {LspSearchQuery::Variant8{workspace_root,..}=>workspace_root.as_ref().map(|value|value.as_str()),LspSearchQuery::Variant9{workspace_root,..}=>workspace_root.as_ref().map(|value|value.as_str()),LspSearchQuery::Variant10{workspace_root,..}=>Some(workspace_root.as_str()),_=>None})
}
pub(crate) fn symbol_name(q: &LspSearchQuery) -> Option<&str> {
    anchored_shape!(q,symbol_name=>Some(symbol_name.as_str()),else=>None)
}
pub(crate) fn symbol_query(q: &LspSearchQuery) -> Option<&str> {
    match q {
        LspSearchQuery::Variant9 { symbol_query, .. } => Some(symbol_query.as_str()),
        LspSearchQuery::Variant10 { symbol_query, .. } => Some(symbol_query.as_str()),
        _ => None,
    }
}
pub(crate) fn anchor_line(q: &LspSearchQuery) -> Option<u32> {
    anchored_shape!(q,line=>Some(u32_of(line.get())),else=>None)
}
pub(crate) fn occurrence(q: &LspSearchQuery) -> Option<u32> {
    anchored_shape!(q,occurrence=>(*occurrence !=0).then(||u32_of_signed(*occurrence)),else=>None)
}
pub(crate) fn include_declaration(q: &LspSearchQuery) -> Option<bool> {
    match q {
        LspSearchQuery::Variant2(query) => Some(query.include_declaration),
        LspSearchQuery::Variant3(query) => Some(query.include_declaration),
        _ => None,
    }
}
/// The `testCode` scope of `references` and `callers`; `None` elsewhere.
pub(crate) fn test_code(q: &LspSearchQuery) -> Option<octocode_contract_types::TestCode> {
    match q {
        LspSearchQuery::Variant2(query) => query.test_code,
        LspSearchQuery::Variant3(query) => query.test_code,
        LspSearchQuery::Variant4(query) => query.test_code,
        _ => None,
    }
}
pub(crate) fn view(q: &LspSearchQuery) -> &'static str {
    if matches!(q, LspSearchQuery::Variant1(_) | LspSearchQuery::Variant3(_)) {
        "files"
    } else {
        "content"
    }
}
pub(crate) fn depth(q: &LspSearchQuery) -> Option<u32> {
    match q {
        LspSearchQuery::Variant4(query) => Some(u32_of(query.depth.get())),
        LspSearchQuery::Variant5(query) => Some(u32_of(query.depth.get())),
        LspSearchQuery::Variant6(query) => Some(u32_of(query.depth.get())),
        _ => None,
    }
}
/// Rows per page (`limit`); `None` cuts pages by the budget alone.
pub(crate) fn limit(q: &LspSearchQuery) -> Option<usize> {
    each_shape!(q,limit=>limit.map(|limit| crate::num::usize_of(limit.get())))
}
pub(crate) fn page(q: &LspSearchQuery) -> Option<u32> {
    each_shape!(q,page=>Some(u32_of(page.get())))
}
pub(crate) fn importer_page(q: &LspSearchQuery) -> u32 {
    match q {
        LspSearchQuery::Variant2(query) => query.importer_page.map_or(1, |page| u32_of(page.get())),
        LspSearchQuery::Variant3(query) => query.importer_page.map_or(1, |page| u32_of(page.get())),
        LspSearchQuery::Variant4(query) => query.importer_page.map_or(1, |page| u32_of(page.get())),
        _ => 1,
    }
}
pub(crate) fn candidate_page(q: &LspSearchQuery) -> u32 {
    match q {
        LspSearchQuery::Variant6(query) => {
            query.candidate_page.map_or(1, |page| u32_of(page.get()))
        }
        _ => 1,
    }
}
pub(crate) fn snapshot(q: &LspSearchQuery) -> Option<&str> {
    each_shape!(q,snapshot=>snapshot.as_ref().map(|value|value.as_str()))
}
pub(crate) fn context_lines(q: &LspSearchQuery) -> Option<octocode_engine::types::SearchContext> {
    match q {
        LspSearchQuery::Variant0(query) => {
            query
                .context_lines
                .as_ref()
                .map(|lines| octocode_engine::types::SearchContext {
                    before: u32_of_signed(lines.before),
                    after: u32_of_signed(lines.after),
                })
        }
        LspSearchQuery::Variant2(query) => {
            query
                .context_lines
                .as_ref()
                .map(|lines| octocode_engine::types::SearchContext {
                    before: u32_of_signed(lines.before),
                    after: u32_of_signed(lines.after),
                })
        }
        _ => None,
    }
}
pub(crate) fn rust_context(q: &LspSearchQuery) -> Option<&octocode_contract_types::LspRustContext> {
    each_shape!(q,rust_context=>rust_context.as_ref())
}
/// The query as a row, without the fields at their schema default: a lead
/// or saved progress names only what the agent set, and the defaults
/// re-apply on parse.
pub(crate) fn to_row(q: &LspSearchQuery) -> Value {
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
