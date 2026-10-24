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

/// A zero-based site: canonical file, line, UTF-16 character.
pub(in super::super) type Site = (String, u32, u32);

/// What [`verify_occurrences`] proved of one window's opened candidates.
#[derive(Default)]
pub(super) struct Verified {
    /// Occurrences that bind the anchor's declaration.
    pub(super) anchors: Vec<Anchor>,
    /// Files whose every occurrence binds another declaration.
    pub(super) rejected: Vec<String>,
    /// Files without an anchor where an identity check failed, an
    /// occurrence's chain stopped on an import the server did not follow,
    /// or a site bound the anchor and another declaration at once.
    /// Undecided is not proof of another declaration, so such a file is
    /// never rejected.
    pub(super) unverified: Vec<String>,
    /// Occurrences that bind another declaration ([`Binding::Other`]): no
    /// answer lists them as references.
    pub(super) elsewhere: HashSet<Site>,
}

/// Classify each occurrence by what its definition chain reaches (see
/// [`binding`]).
pub(super) async fn verify_occurrences(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    opened: Vec<OpenedFile>,
    declaration: &HashSet<String>,
) -> Result<Verified, LspFailure> {
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
    let mut verified = Verified::default();
    for verdict in verdicts {
        verified.elsewhere.extend(
            verdict
                .elsewhere
                .into_iter()
                .map(|(line, character)| (verdict.file.clone(), line, character)),
        );
        if !verdict.anchors.is_empty() {
            verified.anchors.extend(verdict.anchors);
        } else if verdict.unresolved {
            verified.unverified.push(verdict.file);
        } else {
            verified.rejected.push(verdict.file);
        }
    }
    Ok(verified)
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
    /// A site was undecided ([`Binding::Undecided`]), so without an anchor
    /// the file stays unverified rather than rejected.
    pub(super) unresolved: bool,
    /// Zero-based `(line, character)` of sites that bind another declaration.
    pub(super) elsewhere: Vec<(u32, u32)>,
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
    let mut elsewhere = Vec::new();
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
        match binding(&resolved, &file, declaration, &content, missing) {
            Binding::Anchor => {
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
            }
            Binding::Other => elsewhere.push((spot_line, spot_character)),
            Binding::Undecided => unresolved = true,
        }
    }
    Ok(FileVerdict {
        file,
        anchors,
        unresolved,
        elsewhere,
    })
}

/// What the name at one candidate site binds, by its definition chain.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Binding {
    /// The anchor's declaration alone: a reference.
    Anchor,
    /// Another declaration: not a reference.
    Other,
    /// Neither proven: no reference, and the file stays unverified.
    Undecided,
}

/// What the name at a site of `file` (canonical) binds, from the `resolved`
/// identities of its definition chain. It binds the anchor only when every
/// identity is the anchor's `declaration` or lies in a file that declares
/// it (the module's own export of it). A site that also binds a
/// declaration of `file` itself (a same-named `const`, parameter or
/// destructured binding the site's scope declares, which shadows the
/// anchor; a shorthand `{ name }` names it and, through a contextual type,
/// the anchor's property too) binds that declaration. Any other extra
/// identity, an import binding of `file` the server did not follow
/// included, leaves the site undecided. A chain that misses the anchor is
/// another declaration unless it stopped on such an import.
pub(super) fn binding(
    resolved: &HashSet<String>,
    file: &str,
    declaration: &HashSet<String>,
    content: &str,
    missing: &dyn Fn(&Path) -> bool,
) -> Binding {
    if resolved.is_disjoint(declaration) {
        return if stays_on_import(resolved, file, declaration, content, missing) {
            Binding::Undecided
        } else {
            Binding::Other
        };
    }
    let declaring = declaration
        .iter()
        .map(|identity| identity_file(identity))
        .collect::<HashSet<_>>();
    let extra = resolved
        .difference(declaration)
        .filter(|identity| !declaring.contains(identity_file(identity)))
        .collect::<Vec<_>>();
    if extra.is_empty() {
        return Binding::Anchor;
    }
    let own = extra
        .into_iter()
        .filter(|identity| identity_file(identity) == file)
        .cloned()
        .collect::<HashSet<_>>();
    if own.is_empty() || stays_on_import(&own, file, declaration, content, missing) {
        Binding::Undecided
    } else {
        Binding::Other
    }
}

/// The file of a `path:line:character:endLine:endCharacter` identity.
fn identity_file(identity: &str) -> &str {
    identity.rsplitn(5, ':').last().unwrap_or(identity)
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
