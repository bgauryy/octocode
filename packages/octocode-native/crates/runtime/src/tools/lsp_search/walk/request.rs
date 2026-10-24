//! One hierarchy request: roots prepared at the anchor, the walk per direction,
//! and the page row with its disclosures.

use super::Expansion;
use super::bfs::Walk;
use super::caller_names::{ClassFacts, name_module_callers};
use super::compact::{CALLER_PATH, call_items_payload};
use super::importer_recovery::{ImporterRecovery, IncomingRecovery};
use super::nodes::NodeKeys;
use super::outcome::{
    builtin_lib_names, expansion_outcome, mark_partial_expansion, mark_truncation,
};
use super::public::public_edge;
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::tools::lsp_search::anchor::Position;
use crate::tools::lsp_search::failure::{IMPORTER_REQUEST_FAILED, LspFailure, RequestFailures};
use crate::tools::lsp_search::importers::Importers;
use crate::tools::lsp_search::locations::snapshot_changed;
use crate::tools::lsp_search::render::{into_items, symbol_kind_name, uri_to_path};
use crate::tools::lsp_search::scope::Scope;
use crate::tools::lsp_search::source::{SourceCache, item_uri_is_authorized};
use crate::tools::lsp_search::{LspSearchQuery, Op, cancellable, lsp_search_query};
use octocode_engine::lsp::client::NativeLspClient;
use octocode_engine::lsp::symbol_kind;
use serde_json::{Value, json};
use std::collections::HashSet;

/// LSP `SymbolKind`s of a type: class, enum, interface, struct.
const TYPE_KINDS: [u64; 4] = [
    symbol_kind::CLASS as u64,
    symbol_kind::ENUM as u64,
    symbol_kind::INTERFACE as u64,
    symbol_kind::STRUCT as u64,
];

/// The label of `callers` on a type: a call hierarchy has no calls into a
/// type, so a server answers with the symbols that use it (construct it,
/// hold or extend it). `None` for any other operation or anchor.
pub(in crate::tools::lsp_search) fn class_uses_warning(
    operation: &str,
    roots: &[Value],
) -> Option<String> {
    if operation != "callers" {
        return None;
    }
    let kind = roots.iter().find_map(|root| {
        let code = root.get("kind")?.as_u64()?;
        TYPE_KINDS
            .contains(&code)
            .then(|| symbol_kind_name(root.get("kind")))
    })?;
    Some(format!(
        "The anchor is a {kind}: callers lists its uses (symbols that construct, hold or extend it), not calls; each row's `kind` and `detail` name the user."
    ))
}

/// The walk's authorized, deduplicated roots, prepared at the anchor, and
/// the directions to expand.
async fn hierarchy_roots(
    client: &NativeLspClient,
    query: &LspSearchQuery,
    paths: &PathPolicy,
    Position {
        path,
        line,
        character,
    }: Position<'_>,
    cancel: &dyn CancellationCheck,
) -> Result<(Vec<Value>, &'static [Expansion]), LspFailure> {
    let (prepared, expansions): (Value, &[Expansion]) = match lsp_search_query::op(query) {
        Op::Callers | Op::Callees => (
            cancellable(
                cancel,
                client.prepare_call_hierarchy(path.to_owned(), line, character),
            )
            .await??,
            if lsp_search_query::op(query) == Op::Callers {
                &[Expansion::IncomingCalls]
            } else {
                &[Expansion::OutgoingCalls]
            },
        ),
        _ => (
            cancellable(
                cancel,
                client.prepare_type_hierarchy(path.to_owned(), line, character),
            )
            .await??,
            if lsp_search_query::op(query) == Op::Supertypes {
                &[Expansion::Supertypes]
            } else {
                &[Expansion::Subtypes]
            },
        ),
    };
    let mut keys = NodeKeys::new(paths);
    let mut root_keys = HashSet::new();
    let roots = into_items(prepared)
        .into_iter()
        .filter(|root| item_uri_is_authorized(root, paths))
        .filter(|root| root_keys.insert(keys.key(root)))
        .collect::<Vec<_>>();
    Ok((roots, expansions))
}

/// One hierarchy request: the query, the scope its lexical scans search
/// (and where the answer's files are recorded), the anchor, importer
/// recovery for an incoming walk, and the characters a page may spend.
pub(in crate::tools::lsp_search) struct HierarchyRequest<'a> {
    pub(in crate::tools::lsp_search) query: &'a LspSearchQuery,
    pub(in crate::tools::lsp_search) scope: &'a Scope,
    pub(in crate::tools::lsp_search) anchor: Position<'a>,
    pub(in crate::tools::lsp_search) recovery: Option<ImporterRecovery<'a>>,
    pub(in crate::tools::lsp_search) budget: usize,
}

/// A call or type hierarchy walk from the anchor. With `recovery`, an
/// incoming walk first takes the server's level-1 answer, then verifies
/// only the importer files that answer does not cover (the same contract as
/// `references`), roots the walk at their call sites too, and derives
/// callers from their references where the call hierarchy cannot cross the
/// importer's binding. The files the answer covers are recorded on `scope`.
///
/// Importer page 1 lists the anchor's walk and the first importer window; a
/// later page (`importerPage` > 1) walks from its window's importer roots
/// only. Level-1 callers in a candidate file belong to that file's window;
/// deeper levels are not windowed, so with `depth` > 1 subtrees reached from
/// different windows may repeat.
pub(in crate::tools::lsp_search) async fn hierarchy(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    cancel: &dyn CancellationCheck,
    request: HierarchyRequest<'_>,
) -> Result<(Value, Option<Importers>), LspFailure> {
    let HierarchyRequest {
        query,
        scope,
        anchor,
        recovery,
        budget,
    } = request;
    let paths = sources.policy();
    let (roots, expansions) = hierarchy_roots(client, query, paths, anchor, cancel).await?;
    let depth = lsp_search_query::graph_depth(query).unwrap_or(1);
    let mut items = Vec::new();
    let mut failures = Vec::new();
    let mut walks = Vec::new();
    let mut verified = None;
    let mut importer_failures = RequestFailures::default();
    let mut anchor_answered = HashSet::new();
    let mut class_facts = ClassFacts::default();
    for &expansion in expansions {
        let mut walk = Walk::new(paths, &roots, expansion, depth);
        walk.step(client, cancel).await?;
        let mut derived_items = Vec::new();
        if expansion == Expansion::IncomingCalls
            && let Some(recovery) = &recovery
        {
            let incoming = IncomingRecovery {
                anchor,
                scope,
                recovery,
                depth,
            };
            let Some(recovered) = incoming
                .run(client, sources, cancel, &mut walk, &mut class_facts)
                .await?
            else {
                return Ok((snapshot_changed(query), None));
            };
            derived_items = recovered.items;
            importer_failures.extend(recovered.failures);
            verified = Some(recovered.importers);
            anchor_answered = recovered.answered;
        }
        let mut walk = walk.finish(client, cancel).await?;
        if expansion == Expansion::IncomingCalls {
            walk.edges =
                name_module_callers(client, std::mem::take(&mut walk.edges), cancel).await?;
        }
        for edge in &walk.edges {
            let narrowed = if expansion == Expansion::IncomingCalls {
                class_facts.narrow(edge, sources).await
            } else {
                None
            };
            items.push(public_edge(expansion, edge, narrowed.as_ref()));
        }
        items.extend(derived_items);
        walks.push((expansion, walk));
    }
    scope.answer(
        answer_files(anchor.path, &items, verified.as_ref())
            .into_iter()
            .chain(anchor_answered),
    );
    for (_, walk) in &mut walks {
        failures.append(&mut walk.failures);
    }
    let (mut items, failures) = expansion_outcome(items, failures)?;
    if lsp_search_query::op(query) == Op::Callers {
        production_first(&mut items, &scope.root);
    }
    let walks = walks
        .iter()
        .map(|(expansion, walk)| (*expansion, walk))
        .collect::<Vec<_>>();
    // Built-in callees are the answer: a walk that listed only those
    // resolved, it is not empty.
    let mut row = if items.is_empty() && !builtin_lib_names(&walks).is_empty() {
        json!({"path": lsp_search_query::path(query), "lsp": {"serverAvailable": true}})
    } else {
        call_items_payload(
            query,
            &items,
            verified.as_ref().and_then(Importers::digest),
            budget,
            paths,
        )
    };
    if let Some(note) = class_uses_warning(lsp_search_query::operation(query), &roots) {
        crate::response::paging::push_warnings(&mut row, [note]);
    }
    mark_partial_expansion(&mut row, query, &failures);
    importer_failures.disclose(
        &mut row,
        query,
        IMPORTER_REQUEST_FAILED,
        "Importer recovery",
    );
    // Every walked direction is marked together: marking them one after
    // the other would let the second overwrite the first's
    // `unexpandedParents` and `continueWalk*`.
    mark_truncation(&mut row, query, &walks);
    Ok((row, verified))
}

/// Order callers level by level, production files before test files
/// (`is_test_path`, relative to the scope root), keeping the walk's order
/// within each: every edge stays, and the first page leads with the
/// callers that ship.
fn production_first(items: &mut [Value], root: &str) {
    items.sort_by_cached_key(|item| {
        let level = item.get("level").and_then(Value::as_u64).unwrap_or(1);
        let file = [
            &format!("/{CALLER_PATH}"),
            "/from/path",
            "/to/path",
            "/path",
        ]
        .into_iter()
        .find_map(|pointer| item.pointer(pointer).and_then(Value::as_str))
        .map(uri_to_path)
        .unwrap_or_default();
        let relative = std::path::Path::new(&file)
            .strip_prefix(root)
            .map_or(file.clone(), |path| path.to_string_lossy().into_owned());
        (
            level,
            octocode_engine::text::test_paths::is_test_path(&relative),
        )
    });
}

/// Every file a walk's answer covers: the anchor, each listed node and call
/// site file, the verified importers, and the candidates the server showed
/// to name another declaration.
fn answer_files(anchor: &str, items: &[Value], importers: Option<&Importers>) -> Vec<String> {
    let mut files = vec![anchor.to_owned()];
    for item in items {
        for pointer in [
            "/from/path",
            "/to/path",
            "/path",
            &format!("/{CALLER_PATH}"),
        ] {
            if let Some(path) = item.pointer(pointer).and_then(Value::as_str) {
                files.push(path.to_owned());
            }
        }
    }
    if let Some(importers) = importers {
        files.extend(importers.per_file().map(|anchor| anchor.path.clone()));
        files.extend(importers.rejected.iter().cloned());
        files.extend(importers.covered());
    }
    files
}

#[cfg(test)]
mod tests {
    use super::production_first;
    use serde_json::json;

    /// Callers in test files follow the production callers of their level;
    /// every edge stays, in walk order within each group.
    #[test]
    fn production_callers_lead_each_level() {
        let edge = |path: &str, level: u64| json!({"from": {"path": path}, "level": level});
        let mut items = vec![
            edge("/r/src/a.ts", 1),
            edge("/r/tests/a.test.ts", 1),
            edge("/r/src/b.spec.ts", 1),
            edge("/r/src/c.ts", 1),
            json!({"to": {"path": "/r/src/d.ts"}, "callerPath": "/r/src/e.ts", "level": 1}),
            edge("/r/src/f.ts", 2),
        ];
        production_first(&mut items, "/r");
        let order = items
            .iter()
            .map(|item| {
                item.pointer("/callerPath")
                    .or_else(|| item.pointer("/from/path"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            order,
            [
                "/r/src/a.ts",
                "/r/src/c.ts",
                "/r/src/e.ts",
                "/r/tests/a.test.ts",
                "/r/src/b.spec.ts",
                "/r/src/f.ts"
            ]
        );
        // A root that is itself under a test directory does not mark all.
        let mut nested = vec![
            edge("/t/tests/repo/b.test.ts", 1),
            edge("/t/tests/repo/a.ts", 1),
        ];
        production_first(&mut nested, "/t/tests/repo");
        assert_eq!(nested[0]["from"]["path"], "/t/tests/repo/a.ts");
    }
}
