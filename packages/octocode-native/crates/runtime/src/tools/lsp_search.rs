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
use octocode_contract_types::{ErrorCode, LspSearchQuery};
use octocode_engine::lsp::config::{
    LspDiscoveryOptions, default_server_for_file, default_server_for_workspace_root,
    representative_source_for, typescript_server_fallback,
    typescript_server_fallback_for_workspace_root, workspace_root_languages,
    workspace_root_representative_source,
};
use octocode_engine::lsp::pool::LspClientPool;
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
mod page;
mod paths;
pub mod prewarm;
mod receipt;
mod recovery;
mod render;
mod responses;
mod scope;
mod server_coverage;
mod session;
mod source;
mod walk;
mod words;

pub use failure::LspFailure;
use failure::{empty, failure, partial_with_retry, with_next};
pub(crate) use output::Output;
use page::*;
use paths::*;
use render::decode_uri_path;
use responses::*;
use session::*;
use source::{SourceCache, SourceReadError, read_bounded_source_async, snippet_policy};

/// How often a long language-server await re-checks cancellation.
const CANCEL_POLL_MS: u64 = 50;

/// Binds `$field` from whichever shape `$query` is.
macro_rules! each_shape {
    ($query:expr,$field:ident => $value:expr) => {
        match $query {
            LspSearchQuery::Variant0(octocode_contract_types::AnchoredLocations {
                $field, ..
            }) => $value,
            LspSearchQuery::Variant1(octocode_contract_types::AnchoredLocationsFiles {
                $field,
                ..
            }) => $value,
            LspSearchQuery::Variant2(octocode_contract_types::AnchoredReferences {
                $field,
                ..
            }) => $value,
            LspSearchQuery::Variant3(octocode_contract_types::AnchoredReferencesFiles {
                $field,
                ..
            }) => $value,
            LspSearchQuery::Variant4(octocode_contract_types::AnchoredCallers {
                $field, ..
            }) => $value,
            LspSearchQuery::Variant5(octocode_contract_types::AnchoredHierarchy {
                $field, ..
            }) => $value,
            LspSearchQuery::Variant6(octocode_contract_types::AnchoredSubtypes {
                $field, ..
            }) => $value,
            LspSearchQuery::Variant7(octocode_contract_types::AnchoredHover { $field, .. }) => {
                $value
            }
            LspSearchQuery::Variant8 { $field, .. } => $value,
            LspSearchQuery::Variant9 { $field, .. } => $value,
            LspSearchQuery::Variant10 { $field, .. } => $value,
        }
    };
}
macro_rules! anchored_shape {
    ($query:expr,$field:ident => $value:expr,else => $other:expr) => {
        match $query {
            LspSearchQuery::Variant0(octocode_contract_types::AnchoredLocations {
                $field, ..
            }) => $value,
            LspSearchQuery::Variant1(octocode_contract_types::AnchoredLocationsFiles {
                $field,
                ..
            }) => $value,
            LspSearchQuery::Variant2(octocode_contract_types::AnchoredReferences {
                $field,
                ..
            }) => $value,
            LspSearchQuery::Variant3(octocode_contract_types::AnchoredReferencesFiles {
                $field,
                ..
            }) => $value,
            LspSearchQuery::Variant4(octocode_contract_types::AnchoredCallers {
                $field, ..
            }) => $value,
            LspSearchQuery::Variant5(octocode_contract_types::AnchoredHierarchy {
                $field, ..
            }) => $value,
            LspSearchQuery::Variant6(octocode_contract_types::AnchoredSubtypes {
                $field, ..
            }) => $value,
            LspSearchQuery::Variant7(octocode_contract_types::AnchoredHover { $field, .. }) => {
                $value
            }
            _ => $other,
        }
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
pub(crate) mod lsp_search_query;

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

#[cfg(all(test, unix))]
#[expect(
    clippy::expect_used,
    reason = "test module: clippy's allow-expect-in-tests does not see `cfg(all(test, unix))`"
)]
mod process_tests;
#[cfg(test)]
mod tests;
