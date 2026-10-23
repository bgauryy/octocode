//! Opening candidate files and verifying each occurrence's identity with
//! the server, concurrently and in order.
use super::*;

/// A synchronized candidate and its `(line, character, is-call)` occurrences.
pub(super) type OpenedFile = (String, Vec<(u32, u32, bool)>);

/// Open every candidate that mentions `symbol`; wait once, on the last one
/// opened, for the project load the opens trigger, so identity checks do
/// not race it. The anchor document is synchronized first: the client
/// keeps a bounded set of open documents, and a request that verifies
/// several windows must not evict the anchor whose program the checks use.
/// Returns each opened file's occurrences and the candidates that failed.
pub(super) async fn open_candidates(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    cancel: &dyn CancellationCheck,
    anchor: &str,
    files: &[String],
    symbol: &str,
) -> Result<(Vec<OpenedFile>, Vec<String>), LspFailure> {
    if let Some(source) = sources.get(anchor).await {
        client
            .open_document(anchor.to_owned(), &source.content)
            .await
            .map_err(|error| LspFailure::from_engine(&error))?;
    }
    let mut eligible = Vec::new();
    let mut failed = Vec::new();
    for file in files {
        cancel.check().map_err(LspFailure::cancelled)?;
        let Some(source) = sources.get(file).await else {
            failed.push(file.clone());
            continue;
        };
        let spots = occurrences(&source.content, symbol);
        if !spots.is_empty() {
            eligible.push((file.clone(), spots));
        }
    }
    let last = eligible.len().saturating_sub(1);
    let mut opened = Vec::new();
    for (index, (file, spots)) in eligible.into_iter().enumerate() {
        cancel.check().map_err(LspFailure::cancelled)?;
        let Some(source) = sources.get(&file).await else {
            failed.push(file);
            continue;
        };
        let synced = if index == last {
            client
                .open_document_and_wait(
                    file.clone(),
                    &source.content,
                    Some(LAST_OPEN_SETTLE_MS),
                    Some(LAST_OPEN_READY_TIMEOUT_MS),
                )
                .await
                .map(|_| ())
        } else {
            client.open_document(file.clone(), &source.content).await
        };
        if synced.is_ok() {
            opened.push((file, spots));
        } else {
            failed.push(file);
        }
    }
    Ok((opened, failed))
}

/// Keep each occurrence whose definition chain reaches `declaration`.
/// Returns the verified anchors, the files whose every occurrence resolved
/// to another declaration (rejected), and the files left unverified: an
/// identity check failed, or every occurrence's chain stopped inside the
/// file itself (an import binding the server did not follow) while the
/// declaration lives elsewhere. Unresolved is not proof of another
/// declaration, so such a file is never rejected.
pub(super) async fn verify_occurrences(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    opened: Vec<OpenedFile>,
    declaration: &HashSet<String>,
) -> Result<(Vec<Anchor>, Vec<String>, Vec<String>), LspFailure> {
    let policy = sources.policy();
    let missing = proven_missing(policy);
    // A server that pipelines requests (tsgo) gets several files at once,
    // each with its own source cache (a cache needs `&mut`); the answers
    // keep candidate order. tsserver works through requests one at a time,
    // so there each file waits its turn on the shared cache instead of
    // queueing behind the others against its request timeout.
    let verdicts = if client.serves_requests_concurrently() {
        ordered_concurrently(opened, CONCURRENT_FILE_VERIFICATIONS, |file| async {
            let mut own = SourceCache::new(policy);
            verify_file(
                client,
                &mut own,
                snippet_policy,
                cancel,
                file,
                declaration,
                &missing,
            )
            .await
        })
        .await?
    } else {
        let mut verdicts = Vec::with_capacity(opened.len());
        for file in opened {
            verdicts.push(
                verify_file(
                    client,
                    sources,
                    snippet_policy,
                    cancel,
                    file,
                    declaration,
                    &missing,
                )
                .await?,
            );
        }
        verdicts
    };
    let mut anchors = Vec::new();
    let mut rejected = Vec::new();
    let mut unverified = Vec::new();
    for verdict in verdicts {
        if !verdict.anchors.is_empty() {
            anchors.extend(verdict.anchors);
        } else if verdict.unresolved {
            unverified.push(verdict.file);
        } else {
            rejected.push(verdict.file);
        }
    }
    Ok((anchors, rejected, unverified))
}

/// Run `task` over `items` with at most `limit` in flight; results keep
/// the order of `items`, and the first failure drops the unfinished tasks.
pub(super) async fn ordered_concurrently<I, T, Fut>(
    items: Vec<I>,
    limit: usize,
    task: impl FnMut(I) -> Fut,
) -> Result<Vec<T>, LspFailure>
where
    Fut: std::future::Future<Output = Result<T, LspFailure>>,
{
    stream::iter(items)
        .map(task)
        .buffered(limit)
        .try_collect()
        .await
}

/// Candidate files verified at once on a server that pipelines requests.
pub(super) const CONCURRENT_FILE_VERIFICATIONS: usize = 8;

/// What the definition chains of one candidate's occurrences proved.
pub(super) struct FileVerdict {
    pub(super) file: String,
    pub(super) anchors: Vec<Anchor>,
    /// A chain failed or stopped at an import the server did not follow, so
    /// without an anchor the file stays unverified rather than rejected.
    pub(super) unresolved: bool,
}

pub(super) async fn verify_file(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    (file, spots): OpenedFile,
    declaration: &HashSet<String>,
    missing: &(impl Fn(&Path) -> bool + Sync),
) -> Result<FileVerdict, LspFailure> {
    let content = match sources.get(&file).await {
        Some(source) => source.content.clone(),
        None => String::new(),
    };
    let mut anchors = Vec::new();
    let mut unresolved = false;
    // One verified anchor answers for the whole file's program; keep
    // checking only until a verified call site is found as well.
    let mut verified_any = false;
    for (spot_line, spot_character, is_call) in spots {
        if verified_any && !is_call {
            continue;
        }
        cancel.check().map_err(LspFailure::cancelled)?;
        let resolved = identities(
            client,
            sources,
            snippet_policy,
            cancel,
            &file,
            spot_line,
            spot_character,
        )
        .await?;
        let Some(resolved) = resolved else {
            unresolved = true;
            continue;
        };
        if !resolved.is_disjoint(declaration) {
            anchors.push(Anchor {
                path: file.clone(),
                line: spot_line,
                character: spot_character,
                is_call,
            });
            if is_call {
                break;
            }
            verified_any = true;
        } else if stays_on_import(&resolved, &file, declaration, &content, missing) {
            unresolved = true;
        }
    }
    Ok(FileVerdict {
        file,
        anchors,
        unresolved,
    })
}

/// Declaration identities reached by the definition chain at a position.
pub(super) async fn identities(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    path: &str,
    line: u32,
    character: u32,
) -> Result<Option<HashSet<String>>, LspFailure> {
    match resolve_definition_chain(
        client,
        sources,
        snippet_policy,
        cancel,
        path,
        line,
        character,
    )
    .await
    {
        Ok((found, warnings, _)) if warnings.is_empty() => {
            Ok(Some(found.iter().map(snippet_identity).collect()))
        }
        // A retained pre-failure alias is not terminal declaration proof.
        Ok(_) => Ok(None),
        Err(failure) if failure.is_cancelled() => Err(failure),
        Err(_) => Ok(None),
    }
}
