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
                ErrorCode::WorkspaceRootInvalid,
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
                    ErrorCode::WorkspaceRootInvalid,
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
            let extension = std::path::Path::new(&path)
                .extension()
                .map(|extension| format!(".{}", extension.to_string_lossy()))
                .unwrap_or_else(|| "this".to_owned());
            format!(
                "No language server configured for {extension} files. Add a server in lsp.configPath or ~/.octocode/lsp-servers.json; use localSearch or localFetch meanwhile."
            )
        };
        return Err(fail(ErrorCode::ServerUnavailable, &message));
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
pub(super) async fn read_anchor_document(
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
    pub(super) open_readiness: Option<String>,
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
    let Leased {
        client,
        lease,
        config,
        fell_back_from,
    } = acquire_leased_client(query, target, pool, cancel).await?;
    let advertises = |capability: &str| client.has_capability(capability.to_owned());
    let note = |readiness| {
        octocode_engine::lsp::diagnostics::Trace::note_server(|| {
            octocode_engine::lsp::diagnostics::ServerNote {
                source: config.source.as_str(),
                readiness,
                operations: receipt::advertised_operations(&advertises),
            }
        });
    };
    note(client.readiness());
    // A readiness timeout means the live server announced progress and is
    // still indexing. Do not sync or query it yet: return the established
    // incomplete row, naming the server, with an optional same-query
    // reattempt instead.
    if client.readiness().as_deref() == Some("timeout") {
        let mut row = empty(
            query,
            "languageServerIndexing",
            "The language server is still indexing the project; semantic results are not ready yet.",
            true,
        );
        partial_with_related_rerun(&mut row, query, "languageServerIndexing", &[]);
        return Err(receipt::naming(
            Exit::Row(row),
            receipt::server_receipt(&config, false),
        ));
    }
    let operation = lsp_search_query::operation(query);
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
    if open_readiness.is_some() {
        note(open_readiness.clone());
    }
    Ok(Session {
        client,
        _lease: lease,
        config,
        fell_back_from,
        open_readiness,
    })
}

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

/// Lease a client for `target.config`. When it fails to start (not when the
/// request is cancelled) and the target has a fallback, lease that instead
/// and keep the reason, so the answer can say which server replied.
pub(super) async fn acquire_leased_client(
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
    // The row names the server that did not start: what the user configures.
    let failed = || {
        receipt::naming(
            target.fail(query, code, &message, false),
            receipt::server_receipt(&target.config, true),
        )
    };
    let Some(fallback) = target.fallback.clone() else {
        return Err(failed());
    };
    match start_failure(cancellable(cancel, pool.acquire_leased(fallback.clone())).await?) {
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

/// The first didOpen of a document can trigger a project load (tsserver
/// starts one only then); wait for it so queries do not race it.
pub(super) async fn sync_document(
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
                ErrorCode::DocumentSyncFailed,
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
            root_representative(&target.path, &target.config.workspace_root)
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

/// The source file a `workspaceRoot`-only query opens so the server loads a
/// project: the directory's own representative, else (a source directory
/// such as `src` has no project marker) one in the workspace root's
/// language, inside the directory when it has one.
pub(super) fn root_representative(directory: &str, workspace_root: &str) -> Option<String> {
    workspace_root_representative_source(directory).or_else(|| {
        let extension = workspace_root_languages(workspace_root)
            .into_iter()
            .next()?;
        representative_source_for(directory, extension)
            .or_else(|| representative_source_for(workspace_root, extension))
    })
}
