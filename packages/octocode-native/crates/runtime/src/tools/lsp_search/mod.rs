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
use crate::tools::local_fetch::CancellationCheck;
use octocode_engine::lsp::config::{
    LspDiscoveryOptions, default_server_for_file_with_options,
    default_server_for_workspace_root_with_options, workspace_root_representative_source,
};
use octocode_engine::lsp::pool::LspClientPool;
use octocode_engine::lsp::uri::path_to_uri as engine_path_to_uri;
use octocode_engine::lsp::workspace::resolve_workspace_root_for_file;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::path::Path;
use std::time::Duration;

mod anchor;
mod failure;
mod locations;
mod ops;
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LspSearchQuery {
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_name: Option<String>,
    /// Explicit source anchor, consumed as **zero-based** LSP coordinates
    /// (UTF-16 columns) and passed straight through to the language server.
    /// This is the one zero-based coordinate in the contract: every emitted
    /// coordinate is one-based (lines and UTF-16 columns), so an emitted
    /// `displayRange`/`resolvedSymbol` point becomes a `position` by
    /// subtracting 1 from both. `symbolName`+`lineHint` is the one-based
    /// entry point.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<LspPosition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_hint: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_hint: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_declaration: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_by_file: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_lines: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_context: Option<Value>,
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

pub async fn execute(
    query: Value,
    cancel: &dyn CancellationCheck,
    pool: &LspClientPool,
    paths: &PathPolicy,
    execution_config: &LspExecutionConfig,
) -> Result<Value, LspFailure> {
    cancel.check().map_err(LspFailure::cancelled)?;
    let mut query = query;
    // `debug` asks for the provider receipt (server identity, fingerprints,
    // capabilities); ordinary rows carry only the answer.
    let debug = query.get("debug").and_then(Value::as_bool) == Some(true);
    if let Some(object) = query.as_object_mut() {
        object.remove("goal");
        object.remove("reasoning");
        object.remove("debug");
    }
    let query: LspSearchQuery = serde_json::from_value(query)
        .map_err(|error| LspFailure::invalid_query(error.to_string()))?;
    let path = if let Some(uri) = query.uri.as_deref() {
        let decoded = decode_uri_path(uri).map_err(LspFailure::invalid_query)?;
        paths
            .validate_read(&decoded)
            .map_err(LspFailure::path_denied)?
            .canonical
            .to_string_lossy()
            .into_owned()
    } else {
        paths
            .validate(query.workspace_root.as_deref().ok_or_else(|| {
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
        .workspace_root
        .clone()
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
            Ok(content) => Some(sources.insert(&path, content)),
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
        Err(error) => return fail("lsp.anchorUnresolved", &error, true),
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
    if let Some(capability) = receipt::required_capability(&query.operation)
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
