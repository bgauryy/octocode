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
use start_guards::run_cancellation_safe_start;
use state::{SharedResult, shared_error};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LspPoolOptions {
    pub idle_timeout_ms: u64,
    pub max_entries: usize,
    /// Memory all pooled servers may hold together, in bytes: the
    /// pool stops idle least-recently-used servers while the sampled total is
    /// over it, never the one in use or a busy one. `0` means no budget.
    /// Pools that share the budget each keep an equal part
    /// ([`LspClientPool::share_memory_budget`]).
    pub max_resident_bytes: u64,
    /// Options every client the pool starts takes.
    pub client: super::client::LspClientOptions,
}

/// Agent think time between calls often exceeds a minute, and a cold
/// language server costs seconds; an idle server holds only memory.
impl Default for LspPoolOptions {
    fn default() -> Self {
        let max_entries = 4;
        Self {
            idle_timeout_ms: 300_000,
            max_entries,
            max_resident_bytes: super::spawn_limits::default_pool_memory_budget_bytes(max_entries),
            client: super::client::LspClientOptions::default(),
        }
    }
}

/// Start one pooled client; the pool's factory for both acquire flavors.
/// The readiness wait (the server's project load, for languages that need
/// it) happens after the client is installed, through
/// [`pool_client::PoolClient::resume`]: a caller cancelled during that wait
/// leaves a started, indexing server in the pool for the next request
/// instead of throwing its index away, and a wait that ends in `timeout`
/// is resumed by the next acquire instead of failing it.
async fn start_client(
    config: LspLanguageServerConfig,
    options: super::client::LspClientOptions,
    clock: super::clock::LspClock,
) -> SharedResult<NativeLspClient> {
    let client = NativeLspClient::with_clock(config, options, clock);
    let starting_client = client.clone();
    let start = run_cancellation_safe_start(client.clone(), |abandoned| async move {
        starting_client.start_until(abandoned.wait()).await
    })
    .await
    .map_err(|error| shared_error(format!("LSP client startup task failed: {error}")))?;
    start.map_err(Arc::new)?;
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
        let (key, family) = keys_of(&config).await?;
        let options = self.inner.options.client;
        let clock = self.inner.clock.clone();
        self.inner
            .acquire(key, family, || start_client(config, options, clock))
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
        let (key, family) = keys_of(&config).await?;
        let options = self.inner.options.client;
        let clock = &self.inner.clock;
        self.inner
            .acquire_leased(
                key,
                family,
                || start_client(config.clone(), options, clock.clone()),
                NativeLspClient::lease,
            )
            .await
            .map_err(Arc::unwrap_or_clone)
    }

    pub async fn clear(&self, config: &LspLanguageServerConfig) -> Result<bool> {
        let (key, _) = keys_of(config).await?;
        Ok(self.inner.clear(&key).await)
    }

    pub async fn clear_all(&self) {
        self.inner.clear_all().await;
    }

    /// State how many pools share the memory budget, this one included: one
    /// per live server process of the user. Each pool then keeps an equal
    /// part of `max_resident_bytes`, so all of them together hold no more
    /// than one budget; a pool alone keeps the whole. A busy server is stopped
    /// only once its request ends, so a part smaller than one server costs
    /// that server's next cold start, never a request.
    pub fn share_memory_budget(&self, sharers: usize) {
        self.inner.share_memory_budget(sharers);
    }

    /// The servers' sampled bytes and this pool's part of the budget
    /// (`max_resident_bytes` over its sharers) while a busy server other than the
    /// newest keeps them over it. The pool stops that server when its request
    /// ends; a newest server alone over the budget is kept and not reported.
    pub async fn over_budget(&self) -> Option<(u64, u64)> {
        self.inner.over_budget().await
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
pub(super) fn readiness_timeout(language_id: Option<&str>) -> Option<u32> {
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

/// The language a pool key names. clangd serves C and C++ headers and sources
/// as one project and takes no meaning from a document's language id, so they
/// share one server (CUDA differs by its `initializationOptions`).
fn pool_language(language_id: Option<&str>) -> Option<&str> {
    if super::family::is_c_family(language_id) {
        Some("c")
    } else {
        language_id
    }
}

/// The pool key and family of `config`. The key asks what the launch
/// resolves to (a rustup answer costs a process until it is kept), so it is
/// read off the thread that serves the other callers.
async fn keys_of(config: &LspLanguageServerConfig) -> Result<(String, String)> {
    let config = config.clone();
    tokio::task::spawn_blocking(move || {
        Ok((canonical_lsp_key(&config)?, canonical_lsp_family(&config)?))
    })
    .await
    .map_err(|error| Error::new(format!("LSP pool key task failed: {error}")))?
}

/// Sibling-family key of a pool entry: the workspace root, server binary and
/// pooled language, without the initialization options, arguments or
/// environment that make up the full [`canonical_lsp_key`]. Starting a server
/// retires the idle entries that share its family, so an option change leaves
/// one server per workspace instead of two until the idle timeout.
///
/// The environment is outside the family on purpose, so a call that selects
/// another toolchain for the same workspace (another `RUSTUP_TOOLCHAIN`,
/// `VIRTUAL_ENV`, `GOROOT`) has another key but the same family: its start
/// retires the idle server of the first toolchain, and a client that
/// alternates two toolchains on one workspace restarts a server on every
/// switch. Keeping both would hold two resident servers per workspace for a
/// rare case; the pool's entry and memory budgets (`evict_overflow`) are
/// the bound that keying the family by toolchain would spend.
pub fn canonical_lsp_family(config: &LspLanguageServerConfig) -> Result<String> {
    serde_json::to_string(&serde_json::json!({
        "workspaceRoot": normalize_workspace_root(&config.workspace_root)?,
        "command": config.command,
        "languageId": pool_language(config.language_id.as_deref()),
    }))
    .map_err(|error| Error::new(format!("Failed to serialize LSP pool family: {error}")))
}

pub fn canonical_lsp_key(config: &LspLanguageServerConfig) -> Result<String> {
    let workspace_root = normalize_workspace_root(&config.workspace_root)?;
    // What the launch resolves to, not the strings of the caller's
    // environment (`config::launch_identity`).
    let launch = super::config::launch_identity(config);
    let initialization_options = crate::digest::canonical_json(
        config
            .initialization_options
            .clone()
            // `null` options are no options: one server, not two.
            .filter(|options| !options.is_null())
            .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        crate::digest::Nulls::Keep,
    );
    serde_json::to_string(&serde_json::json!({
        "workspaceRoot": workspace_root,
        "args": config.args.clone().unwrap_or_default(),
        "launch": launch,
        "languageId": pool_language(config.language_id.as_deref()),
        "initializationOptions": initialization_options,
        "documentLanguageIds": config.document_language_ids,
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
