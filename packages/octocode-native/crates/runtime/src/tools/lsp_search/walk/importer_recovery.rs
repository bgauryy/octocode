//! Importer recovery of an incoming walk: verify the importer window the
//! server's answer misses, root the walk at its call sites, and derive callers
//! from its references.

use super::bfs::Walk;
use super::caller_names::ClassFacts;
use super::public::public_edge;
use super::{Expansion, HierarchyEdge};
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::tools::lsp_search::anchor::Position;
use crate::tools::lsp_search::call_sites::callers_from_references;
use crate::tools::lsp_search::cancellable;
use crate::tools::lsp_search::failure::{LspFailure, RequestFailures};
use crate::tools::lsp_search::importers::{
    Anchor, CONCURRENT_FILE_VERIFICATIONS, ImporterSearch, Importers, SiteProof, WindowRequest,
    ordered_concurrently, verified_anchors,
};
use crate::tools::lsp_search::render::into_items;
use crate::tools::lsp_search::scope::Scope;
use crate::tools::lsp_search::source::{SourceCache, item_uri_is_authorized};
use octocode_engine::lsp::client::{NativeLspClient, SnippetReadPolicy};
use serde_json::{Value, json};
use std::collections::HashSet;

/// Label for caller edges derived from verified importer references.
const RECOVERED_FROM_REFERENCES: &str = "recoveredFromReferences";

/// Call-hierarchy items prepared at verified importer call sites (TS/JS
/// recovery); an item prepared there resolves in the importer's program.
/// A server that pipelines requests is asked for several at once; the roots
/// keep call-site order. A call site that fails to prepare is recorded in
/// `failures`.
async fn importer_roots(
    client: &NativeLspClient,
    paths: &PathPolicy,
    importers: &Importers,
    cancel: &dyn CancellationCheck,
    failures: &mut RequestFailures,
) -> Result<Vec<Value>, LspFailure> {
    let prepare = |anchor: &Anchor| {
        let request =
            client.prepare_call_hierarchy(anchor.path.clone(), anchor.line, anchor.character);
        async move { cancellable(cancel, request).await }
    };
    let sites = importers.call_sites();
    let answers = if client.serves_requests_concurrently() {
        ordered_concurrently(sites, CONCURRENT_FILE_VERIFICATIONS, prepare).await?
    } else {
        let mut answers = Vec::with_capacity(sites.len());
        for anchor in sites {
            answers.push(prepare(anchor).await?);
        }
        answers
    };
    let mut roots = Vec::new();
    for answer in answers {
        match answer {
            Ok(more) => roots.extend(
                into_items(more)
                    .into_iter()
                    .filter(|root| item_uri_is_authorized(root, paths)),
            ),
            Err(error) => failures.record_engine(&error)?,
        }
    }
    Ok(roots)
}

/// Importer recovery for an incoming walk: the symbol whose importers are
/// verified, the policy for the snippets the checks read, and the importer
/// window to verify.
pub(in crate::tools::lsp_search) struct ImporterRecovery<'a> {
    pub(in crate::tools::lsp_search) symbol: String,
    pub(in crate::tools::lsp_search) snippet_policy: &'a SnippetReadPolicy,
    pub(in crate::tools::lsp_search) window: WindowRequest<'a>,
    pub(in crate::tools::lsp_search) proof: SiteProof,
}

/// Importer recovery of one incoming walk (see [`hierarchy`](super::hierarchy)).
pub(super) struct IncomingRecovery<'r, 'a> {
    pub(super) anchor: Position<'a>,
    /// The anchor's own call-hierarchy items.
    pub(super) roots: &'a [Value],
    pub(super) scope: &'a Scope,
    pub(super) recovery: &'r ImporterRecovery<'a>,
    pub(super) depth: u32,
}

/// What importer recovery added to an incoming walk.
pub(super) struct Recovered {
    pub(super) importers: Importers,
    /// The anchor's file and every file its own walk answered at level 1.
    pub(super) answered: HashSet<String>,
    /// Caller rows derived from the importers' references.
    pub(super) items: Vec<Value>,
    /// Importer requests the server did not answer.
    pub(super) failures: RequestFailures,
}

impl IncomingRecovery<'_, '_> {
    /// Verify the importer window the walk's level-1 answer does not cover,
    /// root `walk` at the importers' call sites too, and derive callers from
    /// their references. `None` when the window's snapshot is stale.
    pub(super) async fn run<'p>(
        &self,
        client: &NativeLspClient,
        sources: &mut SourceCache<'p>,
        cancel: &dyn CancellationCheck,
        walk: &mut Walk<'p>,
        class_facts: &mut ClassFacts,
    ) -> Result<Option<Recovered>, LspFailure> {
        let paths = sources.policy();
        let mut answered = walk.answered_files();
        answered.insert(crate::tools::lsp_search::scope::canonical_uri(
            self.anchor.path,
        ));
        let importers = verified_anchors(
            client,
            sources,
            self.recovery.snippet_policy,
            cancel,
            ImporterSearch {
                symbol: &self.recovery.symbol,
                scope: self.scope,
                declaration: self.anchor,
                known_files: &answered,
                window: self.recovery.window,
                proof: self.recovery.proof,
                reads_imports: self.recovery.proof == SiteProof::DefinitionChain,
            },
        )
        .await?;
        if importers.stale {
            return Ok(None);
        }
        let mut foreign = importers
            .window
            .iter()
            .flat_map(|window| window.foreign().cloned())
            .collect::<HashSet<_>>();
        match self.recovery.proof {
            // A later importer page leaves out the anchor's answer (listed on
            // importer page 1): walk from this window's importer roots only.
            SiteProof::DefinitionChain if self.recovery.window.page > 1 => {
                foreign.extend(answered.iter().cloned());
                *walk = Walk::new(paths, &[], Expansion::IncomingCalls, self.depth);
            }
            SiteProof::DefinitionChain => {}
            // An index-backed answer holds every file opened so far, on any
            // page: each page keeps its own window, and the first page the
            // anchor's file and the files no window names.
            SiteProof::AnchorReferences => {
                if self.recovery.window.page > 1 {
                    let own = importers
                        .window
                        .as_ref()
                        .map(|window| window.files().into_iter().collect::<HashSet<_>>())
                        .unwrap_or_default();
                    foreign.extend(answered.iter().filter(|file| !own.contains(*file)).cloned());
                }
                *walk = Walk::new(paths, &[], Expansion::IncomingCalls, self.depth);
            }
        }
        let mut failures = RequestFailures::default();
        let extra = match self.recovery.proof {
            SiteProof::DefinitionChain => {
                importer_roots(client, paths, &importers, cancel, &mut failures).await?
            }
            // The index holds the window before the walk starts, so the anchor's
            // own items answer for it; an item prepared at a call site is another
            // root of the same symbol (clangd prepares the definition from a call
            // and the declaration from a header).
            SiteProof::AnchorReferences => self.roots.to_vec(),
        };
        walk.add_roots(client, extra, foreign, cancel).await?;
        // Reference-derived callers fill in only files the server's call
        // hierarchy did not answer for at level 1.
        let mut skip = walk.answered_files();
        skip.extend(answered.iter().cloned());
        let derived =
            callers_from_references(client, sources, cancel, &importers, &skip, &mut failures)
                .await?;
        let mut items = Vec::new();
        for (node, sites) in derived {
            let edge = HierarchyEdge {
                node,
                parent: None,
                level: 1,
                sites,
            };
            let narrowed = class_facts.narrow(&edge, sources).await;
            let mut item = public_edge(Expansion::IncomingCalls, &edge, narrowed.as_ref());
            item["source"] = json!(RECOVERED_FROM_REFERENCES);
            items.push(item);
        }
        Ok(Some(Recovered {
            importers,
            answered,
            items,
            failures,
        }))
    }
}
