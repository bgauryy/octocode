//! `callers`/`callees` on a declaration no call can name (an enum, a trait,
//! an interface, a struct, a type alias, a module): the call hierarchy holds
//! nothing for it, so the page answers from the anchor file's own parse
//! before any server starts, and leads to the operations that list its uses
//! (`references`) and its implementors (`implementation`).
use super::super::*;
use super::File;
use crate::lsp_search::render::{wire_kind, with_article};

/// Declaration kinds of the graph facts that no call expression names.
const NON_CALLABLE: &[&str] = &[
    "enum",
    "interface",
    "module",
    "namespace",
    "struct",
    "trait",
    "type",
    "union",
];

/// The reason of the row, naming the declaration's kind.
fn reason(operation: &str, kind: &str) -> String {
    format!(
        "The anchor declares {}, which no call names, so {operation} has nothing to list; its uses come from references, its implementors from implementation.",
        with_article(kind)
    )
}

/// The empty `notCallable` row of a `callers`/`callees` page anchored on the
/// declaration of a non-callable name in `path`, or `None` (any other
/// operation, a later page, an anchor at a use, a name the file declares as
/// a callable too, or a file without a grammar).
pub(in super::super) fn non_callable(query: &LspSearchQuery, path: &str) -> Option<Value> {
    let operation = lsp_search_query::operation(query);
    if !matches!(operation, "callers" | "callees")
        || lsp_search_query::page(query).is_some_and(|page| page > 1)
        || lsp_search_query::snapshot(query).is_some()
    {
        return None;
    }
    let symbol = lsp_search_query::symbol_name(query)?;
    let line = lsp_search_query::anchor_line(query)?;
    let file = File::read(path)?;
    let outline = file.outline()?;
    let named = outline
        .declarations
        .iter()
        .filter(|declaration| file.same_name(&declaration.name, symbol))
        .collect::<Vec<_>>();
    // Only an anchor on the declaration itself decides; a name the file also
    // declares as a callable (a same-named function, a tuple struct's
    // constructor in another declaration) leaves the server to decide.
    let anchored = named.iter().find(|declaration| {
        declaration.kind != "impl" && declaration.selection_range.start.line + 1 == line
    })?;
    // An `impl` block names its type but is no callable either.
    if !named.iter().all(|declaration| {
        declaration.kind == "impl" || NON_CALLABLE.contains(&declaration.kind.as_str())
    }) {
        return None;
    }
    let mut row = failure::empty(
        query,
        "notCallable",
        Some(&reason(operation, wire_kind(&anchored.kind))),
        false,
    );
    row["kind"] = json!(wire_kind(&anchored.kind));
    for lead in ["references", "implementation"] {
        // A related request is fresh: it drops the cursors itself.
        if let Ok(related) =
            lsp_search_query::edited(query, &json!({"operation": lead, "depth": null}))
        {
            octocode_runtime_base::response::result::attach_related(
                &mut row,
                octocode_runtime_base::response::result::Continuation::related_query(related)
                    .build(),
            );
        }
    }
    Some(row)
}
