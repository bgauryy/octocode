mod acquire;
mod lifecycle;
mod pool_client;
mod start_guards;
mod state;

use crate::error::{Error, Result};
use crate::lsp::client::{LspLease, NativeLspClient};
use crate::lsp::types::LspLanguageServerConfig;
use acquire::GenericPool;
use serde_json::Value;
use start_guards::{StopOnDrop, run_cancellation_safe_start};
use state::{SharedResult, shared_error};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

/// Largest readiness wait in [`readiness_timeout`]; the cold-start budget
/// (`lsp::MAX_COLD_LSP_EXECUTION_BUDGET_MS`) is built on it.
pub(crate) const MAX_READINESS_TIMEOUT_MS: u64 = 120_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LspPoolOptions {
    pub idle_timeout_ms: u64,
    pub max_entries: usize,
    /// Resident memory all pooled servers may hold together, in bytes: the
    /// pool stops idle least-recently-used servers while the sampled total is
    /// over it, never the one in use or a busy one. `0` means no budget.
    pub max_resident_bytes: u64,
    /// Options every client the pool starts takes.
    pub client: super::client::LspClientOptions,
}

/// Agent think time between calls often exceeds a minute, and a cold
/// language server costs seconds; an idle server holds only memory.
impl Default for LspPoolOptions {
    fn default() -> Self {
        Self {
            idle_timeout_ms: 300_000,
            max_entries: 4,
            max_resident_bytes: super::spawn_limits::default_memory_cap_bytes(),
            client: super::client::LspClientOptions::default(),
        }
    }
}

/// Start and (for languages that need it) wait for readiness of one pooled
/// client; the pool's factory for both acquire flavors. A readiness wait
/// that ends in `timeout` still installs the started server (it keeps
/// indexing); the next acquire waits again through [`pool_client::PoolClient::resume`]
/// instead of failing on the stored `timeout`.
async fn start_client(
    config: LspLanguageServerConfig,
    options: super::client::LspClientOptions,
) -> SharedResult<NativeLspClient> {
    let client = NativeLspClient::with_options(config.clone(), options);
    let starting_client = client.clone();
    let start =
        run_cancellation_safe_start(client.clone(), async move { starting_client.start().await })
            .await
            .map_err(|error| shared_error(format!("LSP client startup task failed: {error}")))?;
    start.map_err(Arc::new)?;
    let mut cleanup = StopOnDrop::new(client.clone());
    if let Some(timeout_ms) = readiness_timeout(config.language_id.as_deref())
        && let Err(error) = client.wait_for_ready(Some(timeout_ms)).await
    {
        let _ = client.stop().await;
        cleanup.disarm();
        return Err(Arc::new(error));
    }
    cleanup.disarm();
    Ok(Some(client))
}

pub struct LspClientPool {
    inner: GenericPool<NativeLspClient>,
}

impl Default for LspClientPool {
    fn default() -> Self {
        Self::new(LspPoolOptions::default())
    }
}

impl LspClientPool {
    pub fn new(options: LspPoolOptions) -> Self {
        Self {
            inner: GenericPool::new(options),
        }
    }

    pub async fn acquire(
        &self,
        config: LspLanguageServerConfig,
    ) -> Result<Option<NativeLspClient>> {
        let key = canonical_lsp_key(&config)?;
        let options = self.inner.options.client;
        self.inner
            .acquire(key, || start_client(config, options))
            .await
            .map_err(Arc::unwrap_or_clone)
    }

    /// Acquire a client already leased: the lease is taken under the pool
    /// lock, so the idle timer cannot stop the server between acquire and the
    /// caller's first request. Hold the lease for the whole operation.
    pub async fn acquire_leased(
        &self,
        config: LspLanguageServerConfig,
    ) -> Result<Option<(NativeLspClient, LspLease)>> {
        let key = canonical_lsp_key(&config)?;
        let options = self.inner.options.client;
        self.inner
            .acquire_leased(
                key,
                || start_client(config.clone(), options),
                NativeLspClient::lease,
            )
            .await
            .map_err(Arc::unwrap_or_clone)
    }

    pub async fn clear(&self, config: &LspLanguageServerConfig) -> Result<bool> {
        let key = canonical_lsp_key(config)?;
        Ok(self.inner.clear(&key).await)
    }

    pub async fn clear_all(&self) {
        self.inner.clear_all().await;
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// How long a first request waits for the server's project load, per
/// language. Built-in servers and the languages a user's `lsp-servers.json`
/// commonly adds (sourcekit-lsp for Swift, bash-language-server) are listed.
fn readiness_timeout(language_id: Option<&str>) -> Option<u32> {
    use super::family::{is_c_family, is_typescript_family};
    // tsserver/typescript-language-server and pyright/jedi build a project
    // model before they can answer navigation queries; clangd parses the
    // compilation database + preamble; Metals and jdtls import the build.
    // Without a readiness wait a query can race that indexing and return a
    // partial/empty answer stripped of the partiality signal the lifecycle
    // contract promises.
    if is_typescript_family(language_id) {
        return Some(30_000);
    }
    if is_c_family(language_id) {
        return Some(20_000);
    }
    match language_id {
        Some("go" | "python") => Some(15_000),
        Some("rust") => Some(60_000),
        Some("java" | "scala") => Some(120_000),
        Some("csharp" | "swift") => Some(30_000),
        Some("shellscript") => Some(2_000),
        _ => None,
    }
}

pub fn canonical_lsp_key(config: &LspLanguageServerConfig) -> Result<String> {
    let workspace_root = normalize_workspace_root(&config.workspace_root)?;
    let env = config
        .env
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let initialization_options = crate::digest::canonical_json(
        config
            .initialization_options
            .clone()
            .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        crate::digest::Nulls::Keep,
    );
    serde_json::to_string(&serde_json::json!({
        "workspaceRoot": workspace_root,
        "command": config.command,
        "args": config.args.clone().unwrap_or_default(),
        "env": env,
        "languageId": config.language_id,
        "initializationOptions": initialization_options,
        "maxMemoryMb": config.max_memory_mb,
    }))
    .map_err(|error| Error::new(format!("Failed to serialize LSP pool key: {error}")))
}

/// Pool-key form of a workspace root: the real path (symlinks resolved) when
/// it exists, so a symlinked and a direct path share one server; otherwise the
/// lexical normalization of the absolute path.
fn normalize_workspace_root(root: &str) -> Result<String> {
    let path = Path::new(root);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| Error::new(format!("Failed to resolve current directory: {error}")))?
            .join(path)
    };
    if let Ok(real) = std::fs::canonicalize(&absolute) {
        return Ok(real.to_string_lossy().into_owned());
    }
    Ok(crate::text::lexical(&absolute)
        .to_string_lossy()
        .into_owned())
}

#[cfg(test)]
mod tests;
