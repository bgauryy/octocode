//! Native `lspSearch` using the portable engine language-server client.
//!
//! `execute` admits the query (path policy, server discovery), reads the
//! anchor document once, resolves the anchor on that exact text, acquires
//! and leases a pooled client for the whole operation, synchronizes the
//! document, and hands off to the per-operation code:
//!
//! - [`anchor`]: anchor resolution and the `resolvedSymbol` receipt
//! - [`source`]: per-request policy-gated, bounded, cached source reads
//! - [`ops`]: per-operation requests and row shaping
//! - [`recovery`]: definition chains and alias-aware references
//! - [`walk`]: breadth-first call/type-hierarchy walks
//! - [`locations`]: public coordinates, pagination, snapshots
//! - [`failure`]: typed failures, empty/partial rows, `next.*`
//! - [`receipt`]: provider capabilities, Rust context, server receipt
//!
//! Public coordinates, input and output, are one-based lines and one-based
//! UTF-16 columns (see [`locations`]); requests anchor by `symbolName` and
//! `line`.
use crate::execution::cancel::CancellationCheck;
use crate::num::{u32_of, u32_of_signed};
use crate::policy::path::PathPolicy;
use octocode_engine::lsp::config::{
    LspDiscoveryOptions, default_server_for_file, default_server_for_workspace_root,
    representative_source_for, typescript_server_fallback,
    typescript_server_fallback_for_workspace_root, workspace_root_languages,
    workspace_root_representative_source,
};
use octocode_engine::lsp::pool::LspClientPool;
use octocode_engine::lsp::uri::path_to_uri as engine_path_to_uri;
use octocode_engine::lsp::workspace::resolve_workspace_root_for_file;
use serde_json::{Value, json};
use std::future::Future;
use std::path::Path;
use std::time::Duration;

mod anchor;
mod call_sites;
mod failure;
mod fallback_hierarchy;
mod heritage;
mod import_stop;
mod importers;
mod inferred_project;
mod locations;
mod ops;
mod output;
pub mod prewarm;
mod receipt;
mod recovery;
mod render;
mod scope;
mod server_coverage;
mod source;
mod walk;

pub use failure::LspFailure;
use failure::{empty, failure, partial_with_retry, with_next};
pub(crate) use output::Output;
use render::decode_uri_path;
use source::{SourceCache, SourceReadError, read_bounded_source_async, snippet_policy};

/// How often a long language-server await re-checks cancellation.
const CANCEL_POLL_MS: u64 = 50;

use octocode_contract_types as wire;
pub use octocode_contract_types::LspSearchQuery;

/// Binds `$field` from whichever shape `$query` is.
macro_rules! each_shape {
    ($query:expr, $field:ident => $value:expr) => {
        crate::tools::variant_field::variant_field!($query, newtype [
            LspSearchQuery::AnchoredLocations => wire::AnchoredLocations,
            LspSearchQuery::AnchoredReferences => wire::AnchoredReferences,
            LspSearchQuery::AnchoredCallers => wire::AnchoredCallers,
            LspSearchQuery::AnchoredHierarchy => wire::AnchoredHierarchy,
            LspSearchQuery::AnchoredSubtypes => wire::AnchoredSubtypes,
            LspSearchQuery::AnchoredHover => wire::AnchoredHover,
            LspSearchQuery::Document => wire::Document,
            LspSearchQuery::WorkspacePath => wire::WorkspacePath,
            LspSearchQuery::WorkspaceRoot => wire::WorkspaceRoot,
        ] $field => $value)
    };
}

/// Binds `$field` from an anchored shape (a symbol at a line); `$other` for
/// the document and workspace shapes.
macro_rules! anchored_shape {
    ($query:expr, $field:ident => $value:expr, else => $other:expr) => {
        crate::tools::variant_field::variant_field!($query, newtype [
            LspSearchQuery::AnchoredLocations => wire::AnchoredLocations,
            LspSearchQuery::AnchoredReferences => wire::AnchoredReferences,
            LspSearchQuery::AnchoredCallers => wire::AnchoredCallers,
            LspSearchQuery::AnchoredHierarchy => wire::AnchoredHierarchy,
            LspSearchQuery::AnchoredSubtypes => wire::AnchoredSubtypes,
            LspSearchQuery::AnchoredHover => wire::AnchoredHover,
        ] $field => $value, else => $other)
    };
}

/// The requested operation, decoded once from whichever shape the query
/// is. The match is exhaustive over the generated operation enums, so an
/// operation the contract adds fails to compile until it is handled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Op {
    Definition,
    References,
    Callers,
    Callees,
    Hover,
    TypeDefinition,
    Implementation,
    Supertypes,
    Subtypes,
    DocumentSymbols,
    Diagnostic,
    WorkspaceSymbol,
}

impl Op {
    /// The wire spelling.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Definition => "definition",
            Self::References => "references",
            Self::Callers => "callers",
            Self::Callees => "callees",
            Self::Hover => "hover",
            Self::TypeDefinition => "typeDefinition",
            Self::Implementation => "implementation",
            Self::Supertypes => "supertypes",
            Self::Subtypes => "subtypes",
            Self::DocumentSymbols => "documentSymbols",
            Self::Diagnostic => "diagnostic",
            Self::WorkspaceSymbol => "workspaceSymbol",
        }
    }
}

/// Shape-independent views over the generated wire query, in LSP `u32`
/// coordinates.
pub(crate) mod lsp_search_query {
    use super::*;
    pub(crate) fn op(query: &LspSearchQuery) -> Op {
        use wire::{
            AnchoredHierarchyOperation as H, AnchoredLocationsOperation as L,
            DocumentOperation as D,
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
            LspSearchQuery::WorkspacePath(_) | LspSearchQuery::WorkspaceRoot(_) => {
                Op::WorkspaceSymbol
            }
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
}

#[derive(Clone, Debug, Default)]
pub struct LspExecutionConfig {
    pub config_path: Option<String>,
    pub trust_project_config: bool,
    /// Resolved `OCTOCODE_*` settings (process env and trusted `.env` layers).
    pub env: std::collections::BTreeMap<String, String>,
    pub octocode_home: Option<std::path::PathBuf>,
    /// The runtime's working directory: the invocation directory language
    /// server discovery trusts and searches, whatever the process's is.
    pub invocation_dir: Option<std::path::PathBuf>,
    pub(crate) lead_cache: std::sync::Arc<crate::tools::leads::lsp::LeadProbeCache>,
    pub(crate) prewarm_state: std::sync::Arc<std::sync::Mutex<prewarm::WarmState>>,
}

impl LspExecutionConfig {
    /// Engine discovery options for these settings; `config_path` is the
    /// caller-authorized form of [`Self::config_path`].
    #[must_use]
    pub fn discovery(&self, config_path: Option<std::path::PathBuf>) -> LspDiscoveryOptions {
        LspDiscoveryOptions {
            config_path,
            trust_project_config: self.trust_project_config,
            env: self.env.clone(),
            octocode_home: self.octocode_home.clone(),
            invocation_dir: self.invocation_dir.clone(),
        }
    }

    /// Discovery for the `lspSearch` leads other tools offer: the configured
    /// server file counts only when the path policy authorizes reading it.
    #[must_use]
    pub fn lead_discovery(&self, paths: &PathPolicy) -> LspDiscoveryOptions {
        let config_path = self
            .config_path
            .as_deref()
            .and_then(|path| paths.validate_read(path).ok())
            .map(|validated| validated.canonical);
        self.discovery(config_path)
    }
}

/// Await `future`, re-checking `cancel` every [`CANCEL_POLL_MS`]; a
/// cancelled request drops the future (the engine then abandons the
/// language-server request) and returns [`LspFailure::cancelled`].
pub(super) async fn cancellable<F: Future>(
    cancel: &dyn CancellationCheck,
    future: F,
) -> Result<F::Output, LspFailure> {
    cancel.check().map_err(LspFailure::cancelled)?;
    let mut future = std::pin::pin!(future);
    let mut poll = tokio::time::interval(Duration::from_millis(CANCEL_POLL_MS));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            output = &mut future => {
                cancel.check().map_err(LspFailure::cancelled)?;
                return Ok(output);
            },
            _ = poll.tick() => cancel.check().map_err(LspFailure::cancelled)?,
        }
    }
}

/// Run blocking `work` on a worker whose stop callback follows `cancel`
/// live: the request keeps polling `cancel` (as [`cancellable`] does) while
/// the worker runs, and once it fails, or the request future is dropped, the
/// callback returns true so the worker stops at its next check. `Ok(None)`
/// means the worker itself failed rather than produced a result.
pub(super) async fn blocking_cancellable<T: Send + 'static>(
    cancel: &dyn CancellationCheck,
    work: impl FnOnce(&(dyn Fn() -> bool + Sync)) -> T + Send + 'static,
) -> Result<Option<T>, LspFailure> {
    struct StopOnDrop(std::sync::Arc<std::sync::atomic::AtomicBool>);
    impl Drop for StopOnDrop {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let _stop_worker = StopOnDrop(std::sync::Arc::clone(&stop));
    let worker = tokio::task::spawn_blocking(move || {
        work(&|| stop.load(std::sync::atomic::Ordering::Relaxed))
    });
    Ok(cancellable(cancel, worker).await?.ok())
}

/// Why a page stopped before its operation ran: a typed failure, or an
/// error row (`status: "error"`) the caller returns as the answer.
enum Exit {
    Failed(LspFailure),
    Row(Value),
}

impl From<LspFailure> for Exit {
    fn from(failure: LspFailure) -> Self {
        Self::Failed(failure)
    }
}

fn wants(query: &LspSearchQuery, kind: octocode_contract_types::ExtraKind) -> bool {
    each_shape!(query, extras => extras.contains(&kind))
}

fn set_path(query: &mut LspSearchQuery, path: String) {
    match query {
        LspSearchQuery::AnchoredLocations(query) => {
            if let Ok(path) = path.try_into() {
                query.path = path;
            }
        }
        LspSearchQuery::AnchoredReferences(query) => {
            if let Ok(path) = path.try_into() {
                query.path = path;
            }
        }
        LspSearchQuery::AnchoredCallers(query) => {
            if let Ok(path) = path.try_into() {
                query.path = path;
            }
        }
        LspSearchQuery::AnchoredHierarchy(query) => {
            if let Ok(path) = path.try_into() {
                query.path = path;
            }
        }
        LspSearchQuery::AnchoredSubtypes(query) => {
            if let Ok(path) = path.try_into() {
                query.path = path;
            }
        }
        LspSearchQuery::AnchoredHover(query) => {
            if let Ok(path) = path.try_into() {
                query.path = path;
            }
        }
        LspSearchQuery::Document(query) => {
            if let Ok(path) = path.try_into() {
                query.path = path;
            }
        }
        LspSearchQuery::WorkspacePath(query) => {
            if let Ok(path) = path.try_into() {
                query.path = path;
            }
        }
        LspSearchQuery::WorkspaceRoot(query) => query.path = Some(path),
    }
}

fn set_workspace_root(query: &mut LspSearchQuery, root: String) {
    match query {
        LspSearchQuery::WorkspaceRoot(query) => {
            if let Ok(root) = root.try_into() {
                query.workspace_root = root;
            }
        }
        LspSearchQuery::AnchoredLocations(query) => query.workspace_root = Some(root),
        LspSearchQuery::AnchoredReferences(query) => query.workspace_root = Some(root),
        LspSearchQuery::AnchoredCallers(query) => query.workspace_root = Some(root),
        LspSearchQuery::AnchoredHierarchy(query) => query.workspace_root = Some(root),
        LspSearchQuery::AnchoredSubtypes(query) => query.workspace_root = Some(root),
        LspSearchQuery::AnchoredHover(query) => query.workspace_root = Some(root),
        LspSearchQuery::Document(query) => query.workspace_root = Some(root),
        LspSearchQuery::WorkspacePath(query) => query.workspace_root = Some(root),
    }
}

/// Validate the anchor file (or, with no file, the workspace root) once
/// and name it and the workspace root by their canonical paths, so a
/// continuation that spells them relative to the workspace (or absolute)
/// replays the same snapshot. Returns the canonical request path.
fn resolve_paths(query: &mut LspSearchQuery, paths: &PathPolicy) -> Result<String, LspFailure> {
    if let Some(root) = lsp_search_query::workspace_root(query)
        && let Ok(valid) = paths.validate(root)
    {
        set_workspace_root(query, valid.canonical.to_string_lossy().into_owned());
    }
    let path = match lsp_search_query::path(query) {
        Some(uri) => {
            let decoded = decode_uri_path(uri).map_err(LspFailure::invalid_query)?;
            if let Some(root) = directory_as_workspace_root(query, &decoded, paths) {
                return Ok(root);
            }
            let path = paths
                .validate_read(&decoded)
                .map_err(LspFailure::path_denied)?
                .canonical
                .to_string_lossy()
                .into_owned();
            set_path(query, path.clone());
            path
        }
        None => paths
            .validate(lsp_search_query::workspace_root(query).ok_or_else(|| {
                LspFailure::invalid_query("lspSearch requires path or workspaceRoot")
            })?)
            .map_err(LspFailure::path_denied)?
            .canonical
            .to_string_lossy()
            .into_owned(),
    };
    Ok(path)
}

/// A `workspaceSymbol` `path` naming an authorized directory is read as
/// its `workspaceRoot` (a workspace-wide search has no anchor file):
/// the query keeps the directory and searches from it.
fn directory_as_workspace_root(
    query: &mut LspSearchQuery,
    decoded: &str,
    paths: &PathPolicy,
) -> Option<String> {
    if lsp_search_query::op(query) != Op::WorkspaceSymbol {
        return None;
    }
    let directory = paths
        .validate(decoded)
        .ok()
        .filter(|valid| valid.canonical.is_dir())?
        .canonical
        .to_string_lossy()
        .into_owned();
    if lsp_search_query::workspace_root(query).is_none() {
        // A source directory (`src`) is searched from its project root,
        // where the server finds tsconfig.json, Cargo.toml, and the like.
        let root = octocode_engine::lsp::workspace::project_root_of_dir(Path::new(&directory))
            .and_then(|root| paths.validate(&root).ok())
            .filter(|valid| valid.canonical.is_dir())
            .map_or_else(
                || directory.clone(),
                |valid| valid.canonical.to_string_lossy().into_owned(),
            );
        set_workspace_root(query, root);
    }
    set_path(query, directory.clone());
    Some(directory)
}

/// Continuations repeat the caller's spelling of `workspaceRoot` (a
/// workspace-relative root stays relative), not the canonical path the
/// request resolved it to; both name the same root and snapshot.
fn restore_root_spelling(row: &mut Value, canonical: Option<&str>, original: Option<&str>) {
    let (Some(canonical), Some(original)) = (canonical, original) else {
        return;
    };
    if canonical == original {
        return;
    }
    let Some(next) = row.get_mut("next").and_then(Value::as_object_mut) else {
        return;
    };
    for call in next.values_mut() {
        if call.get("tool").and_then(Value::as_str) != Some(crate::id::ToolId::LspSearch.as_str()) {
            continue;
        }
        let rows = call
            .pointer_mut("/query/queries")
            .and_then(Value::as_array_mut);
        for query in rows.into_iter().flatten() {
            if query.get("workspaceRoot").and_then(Value::as_str) == Some(canonical) {
                query["workspaceRoot"] = Value::from(original);
            }
        }
    }
}

/// Operations whose first page may reuse server responses from an earlier
/// request: their answers depend only on the files the scope fingerprint
/// covers. Diagnostics and hover always ask the server.
const REUSED_OPERATIONS: [Op; 3] = [Op::References, Op::Callers, Op::Callees];

/// The page's response scope. Every cached key carries the anchor content
/// digest; for [`REUSED_OPERATIONS`] it also carries the scope fingerprint
/// (any edit, addition, or removal of a family or project file misses), so
/// a first page may reuse an earlier request's answers. Without a
/// fingerprint (walk past its bounds) only continuation pages reuse, as the
/// snapshot check proves they belong to the same result set. A later
/// importer window is a continuation too: reusing the anchor's answer keeps
/// its candidate list (and so the window) the one its digest names, even
/// after earlier windows opened files the server now also searches.
fn response_scope(
    query: &LspSearchQuery,
    content_digest: String,
    fingerprint: Option<&str>,
) -> octocode_engine::lsp::client::ResponseScope {
    let continuation = (lsp_search_query::page(query).is_some_and(|page| page > 1)
        || lsp_search_query::importer_page(query) > 1)
        && lsp_search_query::snapshot(query).is_some();
    match fingerprint {
        Some(fingerprint) if REUSED_OPERATIONS.contains(&lsp_search_query::op(query)) => {
            octocode_engine::lsp::client::ResponseScope {
                reuse: true,
                generation: octocode_engine::digest::sha256(
                    format!("{content_digest}\u{0}{fingerprint}").as_bytes(),
                ),
            }
        }
        _ => octocode_engine::lsp::client::ResponseScope {
            reuse: continuation,
            generation: content_digest,
        },
    }
}

/// Set this page's response scope from the anchor text and, for a
/// [`REUSED_OPERATIONS`] query, the scope fingerprint (one bounded walk).
async fn scope_responses(
    query: &LspSearchQuery,
    scope: &scope::Scope,
    document: Option<&source::Source>,
    cancel: &dyn CancellationCheck,
) -> Result<(), LspFailure> {
    let Some(document) = document else {
        return Ok(());
    };
    let digest = octocode_engine::digest::sha256(document.content.as_bytes());
    let fingerprint = if REUSED_OPERATIONS.contains(&lsp_search_query::op(query)) {
        let walked = scope.walk_inputs();
        blocking_cancellable(cancel, move |stopped| walked.fingerprint(stopped))
            .await?
            .flatten()
    } else {
        None
    };
    if let Some(fingerprint) = &fingerprint {
        scope.set_fingerprint(fingerprint.clone());
    }
    let next = response_scope(query, digest, fingerprint.as_deref());
    let _ =
        octocode_engine::lsp::client::RESPONSE_SCOPE.try_with(|scope| *scope.borrow_mut() = next);
    Ok(())
}

/// Responses read while the project was still loading may be incomplete:
/// key them apart, so no later request reuses them.
fn quarantine_responses() {
    let _ = octocode_engine::lsp::client::RESPONSE_SCOPE.try_with(|scope| {
        let mut scope = scope.borrow_mut();
        scope.reuse = false;
        scope.generation.push_str("\u{0}loading");
    });
}

pub async fn execute(
    query: LspSearchQuery,
    cancel: &dyn CancellationCheck,
    pool: &LspClientPool,
    paths: &PathPolicy,
    execution_config: &LspExecutionConfig,
    response_window: Option<usize>,
) -> Result<Value, LspFailure> {
    // No reuse until the anchor text (and, for incoming walks, the scope
    // fingerprint) keys the page's responses: see `scope_responses`.
    let scope = octocode_engine::lsp::client::ResponseScope::default();
    octocode_engine::lsp::client::RESPONSE_SCOPE
        .scope(
            std::cell::RefCell::new(scope),
            scope::with_canonical_memo(execute_page(
                query,
                cancel,
                pool,
                paths,
                execution_config,
                response_window,
            )),
        )
        .await
}

/// The request as the server sees it: the canonical file (or root), its
/// URI, and the discovered server configuration.
struct Target {
    path: String,
    uri: String,
    root_only: bool,
    config: octocode_engine::lsp::types::LspLanguageServerConfig,
    /// The server to use when `config` (octocode's native tsgo) fails to
    /// start; `None` where there is no substitute.
    fallback: Option<octocode_engine::lsp::types::LspLanguageServerConfig>,
    /// Why the server runs at another root than the one inferred for the
    /// file: a warning every row of the request carries.
    root_note: Option<String>,
}

impl Target {
    fn fail(&self, query: &LspSearchQuery, code: &str, message: &str, available: bool) -> Exit {
        Exit::Row(failure(query, &self.uri, code, message, available))
    }

    /// Whether the server (or its fallback) would launch, by resolution
    /// only: a row that fails before any server starts still says whether
    /// one could.
    fn server_resolves(&self) -> bool {
        use octocode_engine::lsp::config::server_command_resolves;
        server_command_resolves(&self.config)
            || self.fallback.as_ref().is_some_and(server_command_resolves)
    }
}

async fn execute_page(
    mut query: LspSearchQuery,
    cancel: &dyn CancellationCheck,
    pool: &LspClientPool,
    paths: &PathPolicy,
    execution_config: &LspExecutionConfig,
    response_window: Option<usize>,
) -> Result<Value, LspFailure> {
    cancel.check().map_err(LspFailure::cancelled)?;
    if unpinned_later_page(&query) {
        return Ok(unpinned_page_row(&query));
    }
    let original_root = lsp_search_query::workspace_root(&query).map(str::to_owned);
    let path = match resolve_paths(&mut query, paths) {
        Ok(path) => path,
        Err(failure) => return failure::path_failure(&query, paths, failure),
    };
    match run_page(
        &query,
        path,
        cancel,
        pool,
        paths,
        execution_config,
        response_window,
    )
    .await
    {
        Ok(mut row) | Err(Exit::Row(mut row)) => {
            restore_root_spelling(
                &mut row,
                lsp_search_query::workspace_root(&query),
                original_root.as_deref(),
            );
            failure::stamp_coverage(&mut row);
            Ok(row)
        }
        Err(Exit::Failed(failure)) if failure.lexical_fallback => {
            let mut row = failure.row(&query, lsp_search_query::path(&query).unwrap_or_default());
            failure::stamp_coverage(&mut row);
            Ok(row)
        }
        Err(Exit::Failed(failure)) => Err(failure),
    }
}

/// A later page that names neither the snapshot of the page before it nor
/// a `pageSize`: its boundaries would follow the response window shared by
/// the batch, so the same `page` could address different rows.
/// The refusal of a later page with no snapshot and no `pageSize`, led
/// back to the walk's first page (`next.restart`), whose `next.nextPage`
/// carries the snapshot.
fn unpinned_page_row(query: &LspSearchQuery) -> Value {
    json!({
        "status": "error",
        "errorCode": "invalidInput",
        "error": "page > 1 needs the snapshot its next page carries, or an explicit pageSize: without either, pages are cut from this call's response window, which depends on the other rows of the batch.",
        "next": {
            "restart": crate::response::result::restart_query(
                crate::id::ToolId::LspSearch,
                &lsp_search_query::to_row(query),
            )
        }
    })
}

fn unpinned_later_page(query: &LspSearchQuery) -> bool {
    lsp_search_query::page(query).is_some_and(|page| page > 1)
        && lsp_search_query::snapshot(query).is_none()
        && lsp_search_query::page_size(query).is_none()
}

async fn run_page(
    query: &LspSearchQuery,
    path: String,
    cancel: &dyn CancellationCheck,
    pool: &LspClientPool,
    paths: &PathPolicy,
    execution_config: &LspExecutionConfig,
    response_window: Option<usize>,
) -> Result<Value, Exit> {
    let target = discover_server(query, path, paths, execution_config)?;
    let outcome = run_target(query, &target, cancel, pool, paths, response_window).await;
    let Some(note) = target.root_note else {
        return outcome;
    };
    match outcome {
        Ok(mut row) => {
            crate::response::paging::push_warnings(&mut row, [note]);
            Ok(row)
        }
        Err(Exit::Row(mut row)) => {
            crate::response::paging::push_warnings(&mut row, [note]);
            Err(Exit::Row(row))
        }
        failed => failed,
    }
}

/// [`run_page`] once the server is discovered.
async fn run_target(
    query: &LspSearchQuery,
    target: &Target,
    cancel: &dyn CancellationCheck,
    pool: &LspClientPool,
    paths: &PathPolicy,
    response_window: Option<usize>,
) -> Result<Value, Exit> {
    let language_id = target.config.language_id.as_deref();
    let scope = scope::Scope::resolve(
        &target.path,
        &target.config.workspace_root,
        paths,
        language_id,
    );
    // Read the anchor document once (bounded, regular files only). The same
    // text is sent in didOpen, resolves the anchor, and serves same-file
    // context windows; a bad document or anchor costs no server start.
    let mut sources = SourceCache::new(paths);
    let document = read_anchor_document(query, target, &mut sources).await?;
    scope_responses(query, &scope, document.as_deref(), cancel).await?;
    let text = document.as_deref().map(|source| source.content.as_str());
    let anchor =
        anchor::resolve_anchor(query, &target.path, &target.uri, text).map_err(|error| {
            let mut row = failure(
                query,
                &target.uri,
                "anchorUnresolved",
                &error,
                target.server_resolves(),
            );
            failure::anchor_recovery(&mut row, query, text);
            Exit::Row(row)
        })?;
    let session = open_session(
        query,
        target,
        document.as_deref(),
        &mut sources,
        cancel,
        pool,
    )
    .await?;
    if session.open_readiness.as_deref() == Some("timeout") {
        quarantine_responses();
    }
    // One request reads each snippet file once, however many responses
    // name it.
    let snippet_policy = snippet_policy(paths).shared_reads();
    let mut result = ops::Operation {
        client: &session.client,
        query,
        sources: &mut sources,
        snippet_policy: &snippet_policy,
        cancel,
        path: &target.path,
        scope: &scope,
        root_only: target.root_only,
        line: anchor.line,
        character: anchor.character,
        language_id,
        budget: crate::response::page_windows::page_budget(
            response_window,
            &lsp_search_query::to_row(query),
        ),
        response_window,
    }
    .run()
    .await?;
    if needs_text_scan(query, language_id, &result)
        && let Some(name) = lsp_search_query::symbol_name(query)
    {
        scope.text_files(name, paths, cancel).await?;
    }
    annotate(
        &mut result,
        query,
        target,
        &session,
        &scope,
        anchor.resolved_symbol,
    );
    receipt::relative_server_roots(&mut result, paths);
    if matches!(lsp_search_query::op(query), Op::References | Op::Callers) {
        locations::attach_read_lead(&mut result);
    }
    Ok(with_next(query, result))
}

/// Whether coverage annotation may flag `row` with a text lead: an
/// incoming, name-anchored, non-error row of a language with a coverage
/// signal. Its `textOnlyFiles` needs the scope's text scan first.
fn needs_text_scan(query: &LspSearchQuery, language_id: Option<&str>, row: &Value) -> bool {
    use octocode_engine::lsp::family::{is_c_family, is_typescript_family};
    inferred_project::is_incoming(lsp_search_query::operation(query))
        && (is_typescript_family(language_id)
            || is_c_family(language_id)
            || matches!(language_id, Some("python" | "rust")))
        && lsp_search_query::symbol_name(query).is_some_and(|name| !name.trim().is_empty())
        && row.get("status").and_then(Value::as_str) != Some("error")
}

/// Resolve the workspace, then discover the language server for the file
/// (or, for a `workspaceRoot`-only query, from the root's project markers).
fn discover_server(
    query: &LspSearchQuery,
    path: String,
    paths: &PathPolicy,
    execution_config: &LspExecutionConfig,
) -> Result<Target, Exit> {
    let uri =
        engine_path_to_uri(&path).map_err(|error| LspFailure::invalid_query(error.to_string()))?;
    let fail = |code: &str, message: &str| Exit::Row(failure(query, &uri, code, message, false));
    let root_only = Path::new(&path).is_dir();
    let workspace_candidate = lsp_search_query::workspace_root(query)
        .map(str::to_owned)
        .or_else(|| resolve_workspace_root_for_file(path.clone()).ok())
        .unwrap_or_else(|| {
            Path::new(&path)
                .parent()
                .map(|parent| parent.to_string_lossy().into_owned())
                .unwrap_or_else(|| ".".into())
        });
    let mut root_note = None;
    let workspace = match paths.validate(&workspace_candidate) {
        Ok(validated) if validated.canonical.is_dir() => {
            validated.canonical.to_string_lossy().into_owned()
        }
        _ if lsp_search_query::workspace_root(query).is_some() => {
            return Err(fail(
                "workspaceRootInvalid",
                "workspaceRoot is not an authorized directory.",
            ));
        }
        // An inferred root above the allowed roots (a member crate opened
        // from inside its Cargo workspace): the caller named no root, so the
        // server runs at the widest allowed directory on the file's path.
        _ => {
            let inferred = paths.redact(&workspace_candidate);
            let start = if root_only {
                Path::new(&path)
            } else {
                Path::new(&path).parent().unwrap_or(Path::new(&path))
            };
            let Some(allowed) = scope::widest_authorized_dir(start, paths) else {
                return Err(fail(
                    "workspaceRootInvalid",
                    &format!(
                        "The project root inferred for this file ({inferred}) is not an authorized directory, and no directory on the file's path is."
                    ),
                ));
            };
            root_note = Some(format!(
                "The project root inferred for this file ({inferred}) is outside the allowed roots, so the language server runs at {} instead; pass an authorized workspaceRoot to choose another.",
                paths.redact(&allowed)
            ));
            allowed
        }
    };
    let config_path = execution_config
        .config_path
        .as_deref()
        .map(|path| {
            paths
                .validate_read(path)
                .map(|validated| validated.canonical)
        })
        .transpose()
        .map_err(LspFailure::path_denied)?;
    let discovery = execution_config.discovery(config_path);
    let discovered = if root_only {
        default_server_for_workspace_root(&workspace, &discovery)
    } else {
        default_server_for_file(&path, &workspace, &discovery)
    };
    let Some(mut config) = discovered else {
        let message = if root_only {
            format!(
                "No language server could be inferred for this workspace root (none of {}).",
                octocode_engine::lsp::config::workspace_root_marker_names()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            "No language server is configured for this file.".to_owned()
        };
        return Err(fail("serverUnavailable", &message));
    };
    receipt::apply_rust_context(&mut config, query).map_err(LspFailure::invalid_query)?;
    let fallback = if root_only {
        typescript_server_fallback_for_workspace_root(&workspace, &discovery)
    } else {
        typescript_server_fallback(&path, &workspace, &discovery)
    };
    Ok(Target {
        path,
        uri,
        root_only,
        config,
        fallback,
        root_note,
    })
}

/// Read the anchor file once; responses are cached per anchor content (see
/// `scope_responses`), so an edited anchor never reuses earlier answers.
async fn read_anchor_document(
    query: &LspSearchQuery,
    target: &Target,
    sources: &mut SourceCache<'_>,
) -> Result<Option<std::sync::Arc<source::Source>>, Exit> {
    if target.root_only {
        return Ok(None);
    }
    match read_bounded_source_async(std::path::PathBuf::from(&target.path)).await {
        Ok(content) => Ok(Some(sources.insert(&target.path, content))),
        Err(SourceReadError::TooLarge(len)) => Err(target.fail(
            query,
            "fileTooLarge",
            &format!(
                "The source document is too large to synchronize with the language server ({len} bytes > {} bytes).",
                source::MAX_LSP_DIDOPEN_BYTES
            ),
            true,
        )),
        Err(SourceReadError::Unreadable(error)) => Err(target.fail(
            query,
            "fileReadFailed",
            &format!("The source document could not be read: {error}"),
            false,
        )),
    }
}

/// A leased, ready client with the anchor document synchronized.
struct Session {
    client: octocode_engine::lsp::client::NativeLspClient,
    _lease: octocode_engine::lsp::client::LspLease,
    /// The server that answers: the target's, or its fallback.
    config: octocode_engine::lsp::types::LspLanguageServerConfig,
    /// Why the target's own server did not start, when `config` replaced it.
    fell_back_from: Option<String>,
    /// The post-didOpen readiness (timeout when the project load was
    /// still running).
    open_readiness: Option<String>,
}

/// Lease a pooled client for the whole operation and synchronize the anchor
/// document. The lease is taken by the pool under its lock at acquire:
/// syncs, readiness waits, and the gaps between this request's many LSP
/// calls all count as busy, so idle eviction never stops the server
/// mid-operation (nor between acquire and the first request).
async fn open_session(
    query: &LspSearchQuery,
    target: &Target,
    document: Option<&source::Source>,
    sources: &mut SourceCache<'_>,
    cancel: &dyn CancellationCheck,
    pool: &LspClientPool,
) -> Result<Session, Exit> {
    let Leased {
        client,
        lease,
        config,
        fell_back_from,
    } = acquire_leased_client(query, target, pool, cancel).await?;
    // A readiness timeout means the live server announced progress and is
    // still indexing. Do not sync or query it yet: return the established
    // incomplete row with the same-query retry instead.
    if client.readiness().as_deref() == Some("timeout") {
        let mut row = empty(
            query,
            "languageServerIndexing",
            "The language server is still indexing the project; semantic results are not ready yet.",
            true,
        );
        partial_with_retry(&mut row, query, "languageServerIndexing", &[]);
        return Err(Exit::Row(row));
    }
    if let Some(capability) = receipt::required_capability(lsp_search_query::operation(query))
        && !client.has_capability(capability.to_owned())
        && !heritage::fallback_provider(lsp_search_query::operation(query))
            .is_some_and(|provider| client.has_capability(provider.to_owned()))
    {
        return Err(target.fail(
            query,
            "capabilityUnavailable",
            &format!("The language server does not advertise {capability}."),
            true,
        ));
    }
    let open_readiness = sync_document(query, target, document, sources, &client, cancel).await?;
    Ok(Session {
        client,
        _lease: lease,
        config,
        fell_back_from,
        open_readiness,
    })
}

/// A pooled client leased for one operation.
struct Leased {
    client: octocode_engine::lsp::client::NativeLspClient,
    lease: octocode_engine::lsp::client::LspLease,
    /// The configuration that actually started: `target.config`, or its
    /// fallback.
    config: octocode_engine::lsp::types::LspLanguageServerConfig,
    /// Why `target.config` did not start, when `config` is the fallback.
    fell_back_from: Option<String>,
}

/// Lease a client for `target.config`. When it fails to start (not when the
/// request is cancelled) and the target has a fallback, lease that instead
/// and keep the reason, so the answer can say which server replied.
async fn acquire_leased_client(
    query: &LspSearchQuery,
    target: &Target,
    pool: &LspClientPool,
    cancel: &dyn CancellationCheck,
) -> Result<Leased, Exit> {
    let (code, message) =
        match start_failure(cancellable(cancel, pool.acquire_leased(target.config.clone())).await?)
        {
            Ok((client, lease)) => {
                return Ok(Leased {
                    client,
                    lease,
                    config: target.config.clone(),
                    fell_back_from: None,
                });
            }
            Err(failure) => failure,
        };
    let Some(fallback) = target.fallback.clone() else {
        return Err(target.fail(query, code, &message, false));
    };
    match start_failure(cancellable(cancel, pool.acquire_leased(fallback.clone())).await?) {
        Ok((client, lease)) => Ok(Leased {
            client,
            lease,
            config: fallback,
            fell_back_from: Some(message),
        }),
        Err(_) => Err(target.fail(query, code, &message, false)),
    }
}

/// The failure code and message of a server that did not start.
fn start_failure(
    acquired: octocode_engine::error::Result<
        Option<(
            octocode_engine::lsp::client::NativeLspClient,
            octocode_engine::lsp::client::LspLease,
        )>,
    >,
) -> Result<
    (
        octocode_engine::lsp::client::NativeLspClient,
        octocode_engine::lsp::client::LspLease,
    ),
    (&'static str, String),
> {
    match acquired {
        Ok(Some(leased)) => Ok(leased),
        Ok(None) => Err((
            "serverUnavailable",
            "Language server failed to start.".to_owned(),
        )),
        Err(error) => {
            let code = match LspFailure::from_engine(&error).code {
                "timeout" => "timeout",
                _ => "serverUnavailable",
            };
            Err((code, error.to_string()))
        }
    }
}

/// The first didOpen of a document can trigger a project load (tsserver
/// starts one only then); wait for it so queries do not race it.
async fn sync_document(
    query: &LspSearchQuery,
    target: &Target,
    document: Option<&source::Source>,
    sources: &mut SourceCache<'_>,
    client: &octocode_engine::lsp::client::NativeLspClient,
    cancel: &dyn CancellationCheck,
) -> Result<Option<String>, Exit> {
    if let Some(document) = document {
        return match cancellable(
            cancel,
            client.open_document_and_wait(target.path.clone(), &document.content, None, None),
        )
        .await?
        {
            Ok(readiness) => Ok(readiness),
            Err(error) => Err(target.fail(
                query,
                "documentSyncFailed",
                &format!("The source document could not be synchronized: {error}"),
                true,
            )),
        };
    }
    // Some servers (tsserver: "No Project") cannot answer workspace-wide
    // queries until a document of the project is open. Best-effort: a failed
    // sync just leaves the server to answer (or error) as before.
    if target.root_only
        && let Some(representative) =
            workspace_root_representative_source(&target.path).or_else(|| {
                // A source directory (`src`) has no project marker: open a
                // file of the workspace root's language, inside the directory
                // when it has one.
                let root = &target.config.workspace_root;
                let extension = workspace_root_languages(root).into_iter().next()?;
                representative_source_for(&target.path, extension)
                    .or_else(|| representative_source_for(root, extension))
            })
        && let Some(source) = sources.get(&representative).await
        && let Ok(readiness) = cancellable(
            cancel,
            client.open_document_and_wait(representative, &source.content, None, None),
        )
        .await?
    {
        return Ok(readiness);
    }
    Ok(None)
}

/// Disclose an unfinished project load, the inferred-project and compile
/// database context, and the provider receipt.
fn annotate(
    result: &mut Value,
    query: &LspSearchQuery,
    target: &Target,
    session: &Session,
    scope: &scope::Scope,
    resolved_symbol: Option<Value>,
) {
    if session.open_readiness.as_deref() == Some("timeout")
        && result.get("status") != Some(&"error".into())
    {
        partial_with_retry(
            result,
            query,
            "languageServerIndexing",
            &[format!(
                "The language server was still loading the project after {} ms; results may be incomplete.",
                octocode_engine::lsp::client::OPEN_READY_TIMEOUT_MS
            )],
        );
    }
    if let Some(reason) = &session.fell_back_from {
        crate::response::paging::push_warnings(
            result,
            [format!(
                "The native TypeScript server (tsgo) did not start ({reason}); typescript-language-server (tsserver) answered."
            )],
        );
    }
    let language_id = target.config.language_id.as_deref();
    inferred_project::annotate(result, query, language_id, &target.path, scope);
    inferred_project::annotate_compile_database(result, query, language_id, &target.path, scope);
    server_coverage::annotate(
        result,
        query,
        language_id,
        server_coverage::Server {
            anchor: &target.path,
            root: &target.config.workspace_root,
        },
        scope,
    );
    receipt::attach_provider_context(
        result,
        query,
        &target.uri,
        resolved_symbol,
        &session.config,
        &session.client,
        wants(query, octocode_contract_types::ExtraKind::Receipts),
    );
}

#[cfg(all(test, unix))]
mod process_tests;
#[cfg(test)]
mod tests;
