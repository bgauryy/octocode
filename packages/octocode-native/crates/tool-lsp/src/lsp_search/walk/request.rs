//! One hierarchy request: roots prepared at the anchor, the walk per direction,
//! and the page row with its disclosures.

use super::Expansion;
use super::bfs::Walk;
use super::caller_names::{ClassFacts, name_module_callers};
use super::compact::{CALLER_PATH, call_items_payload};
use super::importer_recovery::{ImporterRecovery, IncomingRecovery};
use super::nodes::NodeKeys;
use super::outcome::{
    builtin_lib_names, expansion_outcome, mark_partial_expansion, mark_truncation, withheld_names,
};
use super::public::public_edge;
use crate::lsp_search::anchor::Position;
use crate::lsp_search::failure::{IMPORTER_REQUEST_FAILED, LspFailure, RequestFailures};
use crate::lsp_search::importers::Importers;
use crate::lsp_search::locations::snapshot_changed;
use crate::lsp_search::render::{into_items, symbol_kind_name, uri_to_path};
use crate::lsp_search::scope::Scope;
use crate::lsp_search::source::{SourceCache, item_uri_is_authorized};
use crate::lsp_search::test_scope::{self, ScopeLeft};
use crate::lsp_search::{LspSearchQuery, Op, cancellable, lsp_search_query};
use octocode_engine::lsp::client::NativeLspClient;
use octocode_engine::symbol_kind;
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::policy::path::PathPolicy;
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
pub(in crate::lsp_search) fn class_uses_warning(
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
        "The anchor is {}: callers lists its uses (symbols that construct, hold or extend it), not calls; each row's `kind` and `detail` name the user.",
        crate::lsp_search::render::with_article(&kind)
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
pub(in crate::lsp_search) struct HierarchyRequest<'a> {
    pub(in crate::lsp_search) query: &'a LspSearchQuery,
    pub(in crate::lsp_search) scope: &'a Scope,
    pub(in crate::lsp_search) anchor: Position<'a>,
    pub(in crate::lsp_search) recovery: Option<ImporterRecovery<'a>>,
    pub(in crate::lsp_search) budget: usize,
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
pub(in crate::lsp_search) async fn hierarchy(
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
    let depth = lsp_search_query::depth(query).unwrap_or(1);
    let mut items = Vec::new();
    let mut failures = Vec::new();
    let mut walks = Vec::new();
    let mut verified = None;
    let mut importer_failures = RequestFailures::default();
    let mut anchor_answered = HashSet::new();
    let mut class_facts = ClassFacts::default();
    for &expansion in expansions {
        let mut walk = Walk::new(paths, &roots, expansion, depth);
        let sent_before = client.documents_sent();
        walk.step(client, cancel).await?;
        let mut derived_items = Vec::new();
        if expansion == Expansion::IncomingCalls
            && let Some(recovery) = &recovery
        {
            let incoming = IncomingRecovery {
                anchor,
                roots: &roots,
                scope,
                recovery,
                depth,
                sent_before,
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
    if lsp_search_query::op(query) == Op::Callers {
        scope.answer_lines(
            true,
            std::iter::once((anchor.path.to_owned(), anchor.line)).chain(caller_sites(&items)),
        );
    }
    for (_, walk) in &mut walks {
        failures.append(&mut walk.failures);
    }
    let (mut items, failures) = expansion_outcome(items, failures)?;
    let excluded = scope_callers(query, paths, &mut items);
    let walks = walks
        .iter()
        .map(|(expansion, walk)| (*expansion, walk))
        .collect::<Vec<_>>();
    // Built-in callees, and bases named outside the read policy, are the
    // answer: a walk that listed only those resolved, it is not empty.
    let mut row = if items.is_empty()
        && !(builtin_lib_names(&walks).is_empty() && withheld_names(&walks).is_empty())
    {
        json!({
            "path": lsp_search_query::path(query),
            "lsp": {"serverAvailable": true},
            "operation": lsp_search_query::operation(query),
            "files": [],
            "pagination": octocode_runtime_base::response::pages::PageFacts::counted(0, 0, 0).to_value()
        })
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
        octocode_runtime_base::response::paging::push_warnings(&mut row, [note]);
    }
    test_scope::disclose(&mut row, excluded);
    mark_partial_expansion(&mut row, query, &failures, paths);
    importer_failures.disclose(
        &mut row,
        query,
        IMPORTER_REQUEST_FAILED,
        "Importer recovery",
    );
    // Every walked direction is marked together: marking them one after
    // the other would let the second overwrite the first's
    // `unexpandedParents` and `continueWalk*`.
    mark_truncation(&mut row, query, &walks, paths);
    Ok((row, verified))
}

/// The step every rung's `callers` items take before their page (server,
/// editor and syntax alike): only the callers the query's `testCode` keeps
/// ([`test_scope::retain_callers`]), production before test code
/// ([`production_first`]). What the scope left out is returned for the row's
/// [`test_scope::disclose`]. Other operations have no `testCode` scope, so
/// their items stay as they are.
pub(in crate::lsp_search) fn scope_callers(
    query: &LspSearchQuery,
    paths: &PathPolicy,
    items: &mut Vec<Value>,
) -> ScopeLeft {
    if lsp_search_query::op(query) != Op::Callers {
        return ScopeLeft::default();
    }
    let left = test_scope::retain_callers(query, paths, items);
    production_first(items, paths);
    left
}

/// Order callers level by level, production before test code by the one
/// test-code rule ([`octocode_tools_shared::test_code::TestCodeIndex`]: path, test-only
/// mount, `#[cfg(test)]` item), keeping the walk's order within each: every
/// edge stays, and the first page leads with the callers that ship. The
/// verdicts are kept for the next request while what they read is unchanged.
fn production_first(items: &mut [Value], paths: &PathPolicy) {
    // The verdicts' proof reads through the request's observation.
    crate::lsp_search::observation::within(|| production_first_observed(items, paths));
}

fn production_first_observed(items: &mut [Value], paths: &PathPolicy) {
    let index = octocode_tools_shared::test_code::KeptTestCode::open(paths);
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
        let line = item
            .pointer("/from/line")
            .or_else(|| item.pointer("/fromRanges/0/line"))
            .and_then(Value::as_u64)
            .map(octocode_engine::num::saturating_u32);
        let file = std::path::Path::new(&file);
        let test = index.test_file(file) || line.is_some_and(|line| index.is_test_line(file, line));
        (level, test)
    });
    index.keep();
}

/// Every file a walk's answer covers: the anchor, each listed node and call
/// site file, the verified importers, and the candidates the server showed
/// to name another declaration.
/// The call sites of the anchor's direct callers, by caller file and
/// zero-based line: what a `callers` answer lists of each file it answers.
fn caller_sites(items: &[Value]) -> impl Iterator<Item = (String, u32)> + '_ {
    items
        .iter()
        .filter(|item| item["level"].as_u64().is_none_or(|level| level == 1))
        .filter_map(|item| {
            let path = item
                .pointer("/from/path")
                .or_else(|| item.get(CALLER_PATH))
                .and_then(Value::as_str)?;
            let ranges = item["fromRanges"].as_array()?;
            Some(ranges.iter().filter_map(move |range| {
                let line = range["line"].as_u64()?.checked_sub(1)?;
                Some((path.to_owned(), u32::try_from(line).ok()?))
            }))
        })
        .flatten()
}

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
mod tests;
