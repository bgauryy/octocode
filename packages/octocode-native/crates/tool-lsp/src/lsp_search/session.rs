//! The language-server session: discovering the server, reading the anchor
//! document, leasing a pooled client and syncing the document.
use super::*;

/// Resolve the workspace, then discover the language server for the file
/// (or, for a `workspaceRoot`-only query, from the root's project markers).
pub(super) fn discover_server(
    query: &LspSearchQuery,
    path: String,
    paths: &PathPolicy,
    execution_config: &LspExecutionConfig,
) -> Result<Target, Exit> {
    let uri = octocode_engine::lsp::uri::path_to_uri(&path)
        .map_err(|error| LspFailure::invalid_query(error.to_string()))?;
    let fail =
        |code: ErrorCode, message: &str| Exit::Row(failure(query, &uri, code, message, false));
    let root_only = Path::new(&path).is_dir();
    let discovery = execution_config.discovery().for_one_request();
    let (workspace, root_note) = resolve_workspace(query, &path, paths, &discovery)
        .map_err(|message| fail(ErrorCode::WorkspaceRootInvalid, &message))?;
    if discovery.opens_fetched_code(&path, &workspace) {
        return Err(fail(
            ErrorCode::Policy,
            octocode_engine::lsp::config::FETCHED_CODE_REFUSAL,
        ));
    }
    let discovered = if root_only {
        default_server_for_directory(&path, &workspace, &discovery)
    } else {
        default_server_for_file(&path, &workspace, &discovery)
    };
    let Some(config) = discovered else {
        let message = if root_only {
            format!(
                "No language server could be inferred for this directory: it holds no source file of a supported language, and neither it nor the workspace root has a project marker (none of {}).",
                octocode_engine::lsp::config::workspace_root_marker_names()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            let extension = std::path::Path::new(&path)
                .extension()
                .map(|extension| format!(".{}", extension.to_string_lossy()))
                .unwrap_or_else(|| "this".to_owned());
            format!(
                "No language server configured for {extension} files. Add a server in ~/.octocode/lsp-servers.json; use localSearch or localFetch meanwhile."
            )
        };
        return Err(fail(ErrorCode::ServerUnavailable, &message));
    };
    if let Some(refusal) = octocode_engine::lsp::config::repository_toolchain_refusal(&config) {
        return Err(fail(ErrorCode::ServerUnavailable, &refusal));
    }
    receipt::validate_rust_options(&config).map_err(LspFailure::invalid_query)?;
    let config_notes =
        octocode_engine::lsp::config::route_problems(&path, Some(&config), &workspace, &discovery);
    let fallback = super::page::Fallback {
        path: path.clone(),
        workspace,
        root_only,
        discovery,
        routed: std::sync::OnceLock::new(),
    };
    Ok(Target {
        path,
        uri,
        root_only,
        config,
        fallback,
        root_note,
        config_notes,
    })
}

/// The authorized directory a request on `path` runs at: the query's
/// `workspaceRoot`, else the project root inferred for the file, else its
/// directory; with the note a row carries when the inferred root lies
/// outside the allowed roots and the widest allowed directory stands in.
/// `Err` is the message of a root that cannot be authorized.
pub(super) fn resolve_workspace(
    query: &LspSearchQuery,
    path: &str,
    paths: &PathPolicy,
    discovery: &octocode_engine::lsp::config::LspDiscoveryOptions,
) -> Result<(String, Option<String>), String> {
    let root_only = Path::new(path).is_dir();
    let workspace_candidate = lsp_search_query::workspace_root(query)
        .map(str::to_owned)
        .or_else(|| octocode_engine::lsp::config::workspace_root_for_file(path, discovery).ok())
        .unwrap_or_else(|| {
            Path::new(path)
                .parent()
                .map(|parent| parent.to_string_lossy().into_owned())
                .unwrap_or_else(|| ".".into())
        });
    match paths.validate(&workspace_candidate) {
        Ok(validated) if validated.canonical.is_dir() => {
            Ok((validated.canonical.to_string_lossy().into_owned(), None))
        }
        _ if lsp_search_query::workspace_root(query).is_some() => {
            Err("workspaceRoot is not an authorized directory.".to_owned())
        }
        // An inferred root above the allowed roots (a member crate opened
        // from inside its Cargo workspace): the caller named no root, so the
        // server runs at the widest allowed directory on the file's path.
        _ => {
            let inferred = paths.redact(&workspace_candidate);
            let start = if root_only {
                Path::new(path)
            } else {
                Path::new(path).parent().unwrap_or(Path::new(path))
            };
            let Some(allowed) = scope::widest_authorized_dir(start, paths) else {
                return Err(format!(
                    "The project root inferred for this file ({inferred}) is not an authorized directory, and no directory on the file's path is."
                ));
            };
            let note = format!(
                "The project root inferred for this file ({inferred}) is outside the allowed roots, so the language server runs at {} instead; pass an authorized workspaceRoot to choose another.",
                paths.redact(&allowed)
            );
            Ok((allowed, Some(note)))
        }
    }
}

/// Read the anchor once; its digest verifies the source of a saved continuation.
pub(super) async fn read_anchor_document(
    query: &LspSearchQuery,
    target: &Target,
    sources: &mut SourceCache<'_>,
) -> Result<Option<std::sync::Arc<source::Source>>, Exit> {
    if target.root_only {
        return Ok(None);
    }
    match read_bounded_source_async(std::path::PathBuf::from(&target.path)).await {
        Ok(read) => Ok(Some(sources.insert(&target.path, read))),
        Err(SourceReadError::Binary) => Err(target.fail(
            query,
            ErrorCode::BinaryFileUnsupported,
            "The source document is binary content, not source text.",
            false,
        )),
        Err(SourceReadError::TooLarge(len)) => Err(target.fail(
            query,
            ErrorCode::FileTooLarge,
            &format!(
                "The source document is too large to synchronize with the language server ({len} bytes > {} bytes).",
                source::MAX_LSP_DIDOPEN_BYTES
            ),
            true,
        )),
        Err(SourceReadError::Unreadable(error)) => Err(target.fail(
            query,
            ErrorCode::FileReadFailed,
            &format!("The source document could not be read: {error}"),
            false,
        )),
    }
}

/// A leased, ready client with the anchor document synchronized.
pub(super) struct Session {
    pub(super) client: octocode_engine::lsp::client::NativeLspClient,
    pub(super) _lease: octocode_engine::lsp::client::LspLease,
    /// The server that answers: the target's, or its fallback.
    pub(super) config: octocode_engine::lsp::types::LspLanguageServerConfig,
    /// Why the target's own server did not start, when `config` replaced it.
    pub(super) fell_back_from: Option<String>,
    /// The post-didOpen readiness (timeout when the project load was
    /// still running).
    pub(super) open_readiness: Option<Readiness>,
    /// The pool's resident bytes and budget when it held more than the budget
    /// as this call acquired its client.
    pub(super) pool_over_budget: Option<(u64, u64)>,
}

/// Lease a pooled client for the whole operation and synchronize the anchor
/// document. The lease is taken by the pool under its lock at acquire:
/// syncs, readiness waits, and the gaps between this request's many LSP
/// calls all count as busy, so idle eviction never stops the server
/// mid-operation (nor between acquire and the first request).
pub(super) async fn open_session(
    query: &LspSearchQuery,
    target: &Target,
    document: Option<&source::Source>,
    sources: &mut SourceCache<'_>,
    cancel: &dyn CancellationCheck,
    pool: &LspClientPool,
) -> Result<Session, Exit> {
    let began = tokio::time::Instant::now();
    let leased = acquire_leased_client(query, target, pool, cancel).await?;
    let pool_over_budget = pool.over_budget().await;
    open_leased(
        query,
        target,
        document,
        sources,
        cancel,
        leased,
        began,
        pool_over_budget,
    )
    .await
}

/// [`open_session`] once the client is leased: the readiness and capability
/// checks, then the anchor document's sync.
async fn open_leased(
    query: &LspSearchQuery,
    target: &Target,
    document: Option<&source::Source>,
    sources: &mut SourceCache<'_>,
    cancel: &dyn CancellationCheck,
    leased: Leased,
    began: tokio::time::Instant,
    pool_over_budget: Option<(u64, u64)>,
) -> Result<Session, Exit> {
    let Leased {
        client,
        lease,
        config,
        fell_back_from,
    } = leased;
    let advertises = |capability: &str| client.has_capability(capability.to_owned());
    // A readiness timeout means the live server announced progress and is
    // still indexing: the call waits it out while the server works, within
    // the caller's deadline. A load still under way then is not queried: the
    // row says how long the call waited and offers the same query again, on
    // the server the pool keeps loading.
    let operation = lsp_search_query::operation(query);
    // Dynamic registration may arrive during project loading. Only an already
    // advertised document outline can skip the initial readiness probe.
    if lsp_search_query::op(query) != Op::DocumentSymbols
        || !receipt::admits(operation, &advertises)
    {
        reporting(
            LOADING,
            &config,
            cancellable(cancel, client.wait_for_project_readiness()),
        )
        .await?;
    }
    let load = reporting(
        LOADING,
        &config,
        cancellable(cancel, client.finish_load(client.load_deadline())),
    )
    .await?;
    if load == Some(Readiness::Timeout) {
        return Err(receipt::naming(
            Exit::Row(failure::still_indexing(query, began.elapsed())),
            receipt::server_receipt(&config, false),
        ));
    }
    if let Some(capability) = receipt::required_capability(operation)
        && !receipt::admits(operation, &advertises)
    {
        // The row lists what the server can run instead.
        let mut named = receipt::server_receipt(&config, false);
        named["operations"] = json!(receipt::advertised_operations(&advertises));
        return Err(receipt::naming(
            target.fail(
                query,
                ErrorCode::CapabilityUnavailable,
                &format!("The language server does not advertise {capability}."),
                true,
            ),
            named,
        ));
    }
    let open_readiness = sync_document(query, target, document, sources, &client, cancel).await?;
    Ok(Session {
        client,
        _lease: lease,
        config,
        fell_back_from,
        open_readiness,
        pool_over_budget,
    })
}

/// What a call reports while its server starts.
pub(in crate::lsp_search) const STARTING: &str = "Starting";
/// What a call reports while it waits for a load its server has under way.
pub(in crate::lsp_search) const LOADING: &str = "Waiting for the project load of";
/// What a call reports while its server works on the request's questions.
pub(in crate::lsp_search) const ASKING: &str = "Waiting for an answer from";

/// A pooled client leased for one operation.
pub(super) struct Leased {
    pub(super) client: octocode_engine::lsp::client::NativeLspClient,
    pub(super) lease: octocode_engine::lsp::client::LspLease,
    /// The configuration that actually started: `target.config`, or its
    /// fallback.
    pub(super) config: octocode_engine::lsp::types::LspLanguageServerConfig,
    /// Why `target.config` did not start, when `config` is the fallback.
    pub(super) fell_back_from: Option<String>,
}

/// `work` of the server `config` runs (its start, its project load, or the
/// request's own questions), reporting progress to the call while it takes
/// long: a cold server can take tens of seconds, and a host with a progress
/// token should hear it. Every wait of a request on its server runs here.
pub(in crate::lsp_search) async fn reporting<F: Future>(
    what: &str,
    config: &octocode_engine::lsp::types::LspLanguageServerConfig,
    work: F,
) -> F::Output {
    let server = config.display_name();
    // A start and a project load are the server's cost, not the request's.
    let kept = if what == ASKING { "" } else { KEPT };
    octocode_io::budget::report_while(Box::pin(work), |elapsed| {
        format!(
            "{what} the {server} language server ({}s){kept}",
            elapsed.as_secs()
        )
    })
    .await
}

/// What a start or load wait adds: the wait belongs to the server, which
/// answers later calls for as long as it runs.
pub(in crate::lsp_search) const KEPT: &str = "; later calls reuse this server while it runs";

/// Lease a client for `target.config`. When it fails to start (not when the
/// request is cancelled) and the target has a fallback, lease that instead
/// and keep the reason, so the answer can say which server replied.
pub(super) async fn acquire_leased_client(
    query: &LspSearchQuery,
    target: &Target,
    pool: &LspClientPool,
    cancel: &dyn CancellationCheck,
) -> Result<Leased, Exit> {
    let acquired = reporting(
        STARTING,
        &target.config,
        cancellable(cancel, pool.acquire_initialized(target.config.clone())),
    )
    .await?;
    let (code, message) = match start_failure(acquired) {
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
    // The row names the server that did not start: what the user configures.
    let failed = || {
        receipt::naming(
            target.fail(query, code, &message, false),
            receipt::server_receipt(&target.config, true),
        )
    };
    let Some(fallback) = target.fallback.config().cloned() else {
        return Err(failed());
    };
    let acquired = reporting(
        STARTING,
        &fallback,
        cancellable(cancel, pool.acquire_initialized(fallback.clone())),
    )
    .await?;
    match start_failure(acquired) {
        Ok((client, lease)) => Ok(Leased {
            client,
            lease,
            config: fallback,
            fell_back_from: Some(message),
        }),
        Err(_) => Err(failed()),
    }
}

/// The failure code and message of a server that did not start.
pub(super) fn start_failure(
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
    (ErrorCode, String),
> {
    match acquired {
        Ok(Some(leased)) => Ok(leased),
        Ok(None) => Err((
            ErrorCode::ServerUnavailable,
            "Language server failed to start.".to_owned(),
        )),
        Err(error) => {
            let code = match LspFailure::from_engine(&error).code {
                ErrorCode::Timeout => ErrorCode::Timeout,
                _ => ErrorCode::ServerUnavailable,
            };
            Err((code, error.to_string()))
        }
    }
}

mod sync;
mod waits;
pub(in crate::lsp_search) use sync::sync_document;
pub(in crate::lsp_search) use waits::{disclose_pool_overflow, session_failed};
