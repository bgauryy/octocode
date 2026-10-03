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
//! Public output coordinates are one-based lines and one-based UTF-16
//! columns (see [`locations`]); only the `position` input is zero-based.
use crate::policy::path::PathPolicy;
use crate::tools::cancel::CancellationCheck;
use octocode_engine::lsp::config::{
    LspDiscoveryOptions, default_server_for_file_with_options,
    default_server_for_workspace_root_with_options, workspace_root_representative_source,
};
use octocode_engine::lsp::pool::LspClientPool;
use octocode_engine::lsp::uri::path_to_uri as engine_path_to_uri;
use octocode_engine::lsp::workspace::resolve_workspace_root_for_file;
use serde_json::Value;
use std::future::Future;
use std::path::Path;
use std::time::Duration;

mod anchor;
mod failure;
mod importers;
mod inferred_project;
mod locations;
mod ops;
pub mod prewarm;
mod receipt;
mod recovery;
mod render;
mod source;
mod walk;

pub use failure::LspFailure;
use failure::{failure, mark_partial, with_next};
use render::decode_uri_path;
use source::{SourceCache, SourceReadError, read_bounded_source_async, snippet_policy};

/// After the first `didOpen` of a document, how long to wait for a project
/// load the open triggers to START (tsserver begins ~100 ms after didOpen),
/// and the bound on the whole post-open wait.
const DIDOPEN_SETTLE_MS: u32 = 400;
const DIDOPEN_READY_TIMEOUT_MS: u32 = 15_000;
/// How often a long language-server await re-checks cancellation.
const CANCEL_POLL_MS: u64 = 50;

use crate::contracts::tool_types as wire;
pub use crate::contracts::tool_types::LspSearchQuery;

/// Binds `$field` from whichever anchor shape `$query` is. The shapes type
/// shared fields differently, so each gets its own arm.
macro_rules! each_shape {
    ($query:expr, $field:ident => $value:expr) => {
        match $query {
            LspSearchQuery::Anchored(wire::Anchored { $field, .. }) => $value,
            LspSearchQuery::Position(wire::Position { $field, .. }) => $value,
            LspSearchQuery::Document(wire::Document { $field, .. }) => $value,
            LspSearchQuery::WorkspaceUri(wire::WorkspaceUri { $field, .. }) => $value,
            LspSearchQuery::WorkspaceRoot(wire::WorkspaceRoot { $field, .. }) => $value,
        }
    };
}

fn u32_of(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn u32_of_signed(value: i64) -> u32 {
    u32::try_from(value.max(0)).unwrap_or(u32::MAX)
}

/// Shape-independent views over the generated wire query, in LSP `u32`
/// coordinates.
impl LspSearchQuery {
    pub fn operation(&self) -> String {
        each_shape!(self, operation => operation.to_string())
    }
    pub fn uri(&self) -> Option<&str> {
        match self {
            Self::Anchored(query) => Some(query.uri.as_str()),
            Self::Position(query) => Some(query.uri.as_str()),
            Self::Document(query) => Some(query.uri.as_str()),
            Self::WorkspaceUri(query) => Some(query.uri.as_str()),
            Self::WorkspaceRoot(query) => query.uri.as_deref(),
        }
    }
    pub fn workspace_root(&self) -> Option<&str> {
        match self {
            Self::WorkspaceRoot(query) => Some(query.workspace_root.as_str()),
            Self::Anchored(query) => query.workspace_root.as_deref(),
            Self::Position(query) => query.workspace_root.as_deref(),
            Self::Document(query) => query.workspace_root.as_deref(),
            Self::WorkspaceUri(query) => query.workspace_root.as_deref(),
        }
    }
    pub fn symbol_name(&self) -> Option<&str> {
        match self {
            Self::Anchored(query) => Some(query.symbol_name.as_str()),
            Self::WorkspaceUri(query) => Some(query.symbol_name.as_str()),
            Self::WorkspaceRoot(query) => Some(query.symbol_name.as_str()),
            Self::Position(_) | Self::Document(_) => None,
        }
    }
    /// The explicit zero-based anchor as `(line, character)`.
    pub fn position(&self) -> Option<(u32, u32)> {
        match self {
            Self::Position(query) => Some((
                u32_of_signed(query.position.line),
                u32_of_signed(query.position.character),
            )),
            _ => None,
        }
    }
    pub fn line_hint(&self) -> Option<u32> {
        match self {
            Self::Anchored(query) => Some(u32_of(query.line_hint.get())),
            _ => None,
        }
    }
    /// The occurrence index; the default (first occurrence) reads as unset.
    pub fn order_hint(&self) -> Option<u32> {
        match self {
            Self::Anchored(query) => {
                (query.order_hint != 0).then(|| u32_of_signed(query.order_hint))
            }
            _ => None,
        }
    }
    pub fn include_declaration(&self) -> Option<bool> {
        match self {
            Self::Anchored(query) => query.include_declaration,
            Self::Position(query) => query.include_declaration,
            _ => None,
        }
    }
    pub fn group_by_file(&self) -> Option<bool> {
        match self {
            Self::Anchored(query) => query.group_by_file,
            Self::Position(query) => query.group_by_file,
            _ => None,
        }
    }
    pub fn depth(&self) -> Option<u32> {
        match self {
            Self::Anchored(query) => query.depth.map(u32_of_signed),
            Self::Position(query) => query.depth.map(u32_of_signed),
            _ => None,
        }
    }
    pub fn page(&self) -> Option<u32> {
        each_shape!(self, page => Some(u32_of(page.get())))
    }
    pub fn page_size(&self) -> u32 {
        each_shape!(self, page_size => u32_of(page_size.get()))
    }
    pub fn snapshot(&self) -> Option<&str> {
        each_shape!(self, snapshot => snapshot.as_ref().map(|snapshot| snapshot.as_str()))
    }
    pub fn context_lines(&self) -> Option<u32> {
        match self {
            Self::Anchored(query) => query.context_lines.map(u32_of_signed),
            Self::Position(query) => query.context_lines.map(u32_of_signed),
            _ => None,
        }
    }
    /// The Rust build context as JSON, decoded by [`receipt`].
    pub fn rust_context(&self) -> Option<Value> {
        each_shape!(self, rust_context => rust_context
            .as_ref()
            .and_then(|context| serde_json::to_value(context).ok()))
    }
    /// The query as a replayable JSON row.
    pub fn to_row(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| serde_json::json!({}))
    }
}

#[derive(Clone, Debug, Default)]
pub struct LspExecutionConfig {
    pub config_path: Option<String>,
    pub trust_project_config: bool,
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
            output = &mut future => return Ok(output),
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

/// Name the anchor file and workspace root by their canonical paths, so a
/// continuation that spells them relative to the workspace (or absolute)
/// replays the same snapshot. Invalid paths stay for the checks below.
fn canonicalize_query_paths(query: &mut Value, paths: &PathPolicy) {
    if let Some(uri) = query.get("uri").and_then(Value::as_str)
        && let Ok(decoded) = decode_uri_path(uri)
        && let Ok(valid) = paths.validate_read(&decoded)
    {
        query["uri"] = Value::String(valid.canonical.to_string_lossy().into_owned());
    }
    if let Some(root) = query.get("workspaceRoot").and_then(Value::as_str)
        && let Ok(valid) = paths.validate(root)
    {
        query["workspaceRoot"] = Value::String(valid.canonical.to_string_lossy().into_owned());
    }
}

pub async fn execute(
    query: Value,
    cancel: &dyn CancellationCheck,
    pool: &LspClientPool,
    paths: &PathPolicy,
    execution_config: &LspExecutionConfig,
) -> Result<Value, LspFailure> {
    // A continuation page (page > 1 with its walk's snapshot) may reuse the
    // server responses computed for page 1; the snapshot check still proves
    // the page belongs to the same result set. First pages always re-query.
    let reuse = query
        .get("page")
        .and_then(Value::as_u64)
        .is_some_and(|page| page > 1)
        && query.get("snapshot").is_some_and(Value::is_string);
    let scope = octocode_engine::lsp::client::ResponseScope {
        reuse,
        generation: String::new(),
    };
    octocode_engine::lsp::client::RESPONSE_SCOPE
        .scope(
            std::cell::RefCell::new(scope),
            execute_page(query, cancel, pool, paths, execution_config),
        )
        .await
}

async fn execute_page(
    query: Value,
    cancel: &dyn CancellationCheck,
    pool: &LspClientPool,
    paths: &PathPolicy,
    execution_config: &LspExecutionConfig,
) -> Result<Value, LspFailure> {
    cancel.check().map_err(LspFailure::cancelled)?;
    // `debug` asks for the provider receipt (server identity, fingerprints,
    // capabilities); ordinary rows carry only the answer.
    let debug = query.get("debug").and_then(Value::as_bool) == Some(true);
    let mut query = query;
    canonicalize_query_paths(&mut query, paths);
    let query: LspSearchQuery = serde_json::from_value(query)
        .map_err(|error| LspFailure::invalid_query(error.to_string()))?;
    let path = if let Some(uri) = query.uri() {
        let decoded = decode_uri_path(uri).map_err(LspFailure::invalid_query)?;
        paths
            .validate_read(&decoded)
            .map_err(LspFailure::path_denied)?
            .canonical
            .to_string_lossy()
            .into_owned()
    } else {
        paths
            .validate(query.workspace_root().ok_or_else(|| {
                LspFailure::invalid_query("lspSearch requires uri or workspaceRoot")
            })?)
            .map_err(LspFailure::path_denied)?
            .canonical
            .to_string_lossy()
            .into_owned()
    };
    let canonical_uri =
        engine_path_to_uri(&path).map_err(|error| LspFailure::invalid_query(error.to_string()))?;
    let fail = |code: &str, message: &str, server_available: bool| {
        Ok(failure(
            &query,
            &canonical_uri,
            code,
            message,
            server_available,
        ))
    };
    let workspace_candidate = query
        .workspace_root()
        .map(str::to_owned)
        .or_else(|| resolve_workspace_root_for_file(path.clone()).ok())
        .unwrap_or_else(|| {
            Path::new(&path)
                .parent()
                .map(|parent| parent.to_string_lossy().into_owned())
                .unwrap_or_else(|| ".".into())
        });
    let workspace = match paths.validate(&workspace_candidate) {
        Ok(validated) if validated.canonical.is_dir() => {
            validated.canonical.to_string_lossy().into_owned()
        }
        _ => {
            return fail(
                "lsp.workspaceRootInvalid",
                "workspaceRoot is not an authorized directory.",
                false,
            );
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
    let discovery = LspDiscoveryOptions {
        config_path,
        trust_project_config: execution_config.trust_project_config,
    };
    // A `workspaceRoot`-only query has a directory, not a file: infer the
    // server from project markers instead of the (absent) file extension.
    let root_only = Path::new(&path).is_dir();
    let discovered = if root_only {
        default_server_for_workspace_root_with_options(workspace, &discovery)
    } else {
        default_server_for_file_with_options(path.clone(), workspace, &discovery)
    };
    let Some(mut config) = discovered else {
        return fail(
            "lsp.serverUnavailable",
            if root_only {
                "No language server could be inferred for this workspace root (no tsconfig.json, Cargo.toml, go.mod, pyproject.toml, setup.py, jsconfig.json, or package.json)."
            } else {
                "No language server is configured for this file."
            },
            false,
        );
    };
    receipt::apply_rust_context(&mut config, &query).map_err(LspFailure::invalid_query)?;

    // Read the anchor document once (bounded, regular files only). The same
    // text is sent in didOpen, resolves the anchor, and serves same-file
    // context windows; a bad document or anchor costs no server start.
    let mut sources = SourceCache::new(paths);
    let document = if root_only {
        None
    } else {
        match read_bounded_source_async(std::path::PathBuf::from(&path)).await {
            Ok(content) => {
                // Responses are cached per anchor content: an edited anchor
                // never reuses an earlier page's server answers.
                let generation = {
                    use sha2::{Digest, Sha256};
                    hex::encode(Sha256::digest(content.as_bytes()))
                };
                let _ = octocode_engine::lsp::client::RESPONSE_SCOPE
                    .try_with(|scope| scope.borrow_mut().generation = generation);
                Some(sources.insert(&path, content))
            }
            Err(SourceReadError::TooLarge(len)) => {
                return fail(
                    "lsp.documentTooLarge",
                    &format!(
                        "The source document is too large to synchronize with the language server ({len} bytes > {} bytes).",
                        source::MAX_LSP_DIDOPEN_BYTES
                    ),
                    true,
                );
            }
            Err(SourceReadError::Unreadable(error)) => {
                return fail(
                    "lsp.documentReadFailed",
                    &format!("The source document could not be read: {error}"),
                    false,
                );
            }
        }
    };
    let anchor = match anchor::resolve_anchor(
        &query,
        &path,
        &canonical_uri,
        document.as_deref().map(|source| source.content.as_str()),
    ) {
        Ok(anchor) => anchor,
        Err(error) => {
            let mut row = failure(&query, &canonical_uri, "lsp.anchorUnresolved", &error, true);
            failure::anchor_recovery(
                &mut row,
                &query,
                document.as_deref().map(|source| source.content.as_str()),
            );
            return Ok(row);
        }
    };

    let receipt_config = config.clone();
    // One lease for the whole operation, taken by the pool under its lock at
    // acquire: syncs, readiness waits, and the gaps between this request's
    // many LSP calls all count as busy, so idle eviction never stops the
    // server mid-operation (nor between acquire and the first request).
    let (client, _lease) = match cancellable(cancel, pool.acquire_leased(config)).await? {
        Ok(Some(leased)) => leased,
        Ok(None) => {
            return fail(
                "lsp.serverUnavailable",
                "Language server failed to start.",
                false,
            );
        }
        Err(error) => {
            let code = match LspFailure::from_engine(&error).code {
                "lsp.timeout" => "lsp.timeout",
                _ => "lsp.serverUnavailable",
            };
            return fail(code, &error.to_string(), false);
        }
    };
    if client.readiness().as_deref() == Some("timeout") {
        return fail(
            "lsp.timeout",
            "Timed out waiting for the language server to become ready.",
            false,
        );
    }
    if let Some(capability) = receipt::required_capability(&query.operation())
        && !client.has_capability(capability.to_owned())
    {
        return fail(
            "lsp.capabilityUnavailable",
            &format!("The language server does not advertise {capability}."),
            true,
        );
    }
    // The first didOpen of a document can trigger a project load (tsserver
    // starts one only then); wait for it so queries do not race it.
    let mut open_readiness = None;
    if let Some(document) = &document {
        match cancellable(
            cancel,
            client.open_document_and_wait(
                path.clone(),
                document.content.clone(),
                Some(DIDOPEN_SETTLE_MS),
                Some(DIDOPEN_READY_TIMEOUT_MS),
            ),
        )
        .await?
        {
            Ok(readiness) => open_readiness = readiness,
            Err(error) => {
                return fail(
                    "lsp.documentSyncFailed",
                    &format!("The source document could not be synchronized: {error}"),
                    true,
                );
            }
        }
    } else if root_only
        && let Some(representative) = workspace_root_representative_source(&path)
        && let Some(source) = sources.get(&representative).await
    {
        // Some servers (tsserver: "No Project") cannot answer workspace-wide
        // queries until a document of the project is open. Best-effort: a
        // failed sync just leaves the server to answer (or error) as before.
        if let Ok(readiness) = cancellable(
            cancel,
            client.open_document_and_wait(
                representative,
                source.content.clone(),
                Some(DIDOPEN_SETTLE_MS),
                Some(DIDOPEN_READY_TIMEOUT_MS),
            ),
        )
        .await?
        {
            open_readiness = readiness;
        }
    }
    let snippet_policy = snippet_policy(paths);
    let mut result = ops::Operation {
        client: &client,
        query: &query,
        sources: &mut sources,
        snippet_policy: &snippet_policy,
        cancel,
        path: &path,
        workspace_root: receipt_config.workspace_root.as_str(),
        root_only,
        line: anchor.line,
        character: anchor.character,
        language_id: receipt_config.language_id.as_deref(),
    }
    .run()
    .await?;
    if open_readiness.as_deref() == Some("timeout") && result.get("status") != Some(&"error".into())
    {
        mark_partial(
            &mut result,
            &query,
            "languageServerIndexing",
            &[format!(
                "The language server was still loading the project after {DIDOPEN_READY_TIMEOUT_MS} ms; results may be incomplete."
            )],
        );
    }
    inferred_project::annotate(
        &mut result,
        &query,
        receipt_config.language_id.as_deref(),
        &path,
        &receipt_config.workspace_root,
    );
    inferred_project::annotate_compile_database(
        &mut result,
        &query,
        receipt_config.language_id.as_deref(),
        &path,
        &receipt_config.workspace_root,
    );
    receipt::attach_provider_context(
        &mut result,
        &query,
        &canonical_uri,
        anchor.resolved_symbol,
        &receipt_config,
        &client,
        debug,
    );
    Ok(with_next(&query, result))
}

#[cfg(all(test, unix))]
mod process_tests;
#[cfg(test)]
mod tests;
