//! Opening candidate files and verifying each occurrence's identity with
//! the server, concurrently and in order.
use super::output::through_output;
use super::*;
use crate::lsp_search::build_output::BuildOutput;
use crate::lsp_search::call_sites::classify_sites;
use crate::lsp_search::source::Source;
use octocode_engine::graph::GraphPosition;

/// One occurrence of the name in a candidate: zero-based line and UTF-16
/// character, whether it is a call, and the file's own binding it names
/// (the oxc parse's id, see `ReferenceSite::binding`).
#[derive(Clone, Copy, Debug)]
pub(super) struct Spot {
    pub(super) line: u32,
    pub(super) character: u32,
    pub(super) is_call: bool,
    pub(super) binding: Option<u32>,
}

/// A synchronized candidate and its occurrences.
pub(super) type OpenedFile = (String, Vec<Spot>);

/// What [`open_candidates`] made of a window's candidates.
#[derive(Default)]
pub(super) struct Opened {
    /// Each synchronized candidate and its occurrences.
    pub(super) files: Vec<OpenedFile>,
    /// Candidates that could not be read or opened in the server.
    pub(super) failed: Vec<String>,
    /// Candidates larger than any source a language server is sent (a
    /// vendored bundle): never opened, so never verified, and no failure.
    pub(super) too_large: Vec<String>,
}

/// Open every candidate that mentions one of its names (`names_in`: the
/// symbol, and an alias a relay exports it under); wait once, on the last one
/// opened, for the project load the opens trigger, so identity checks do
/// not race it. The anchor document is synchronized first: the client
/// keeps a bounded set of open documents, and a request that verifies
/// several windows must not evict the anchor whose program the checks use.
pub(super) async fn open_candidates(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    cancel: &dyn CancellationCheck,
    anchor: &str,
    files: &[String],
    names_in: &dyn Fn(&str) -> Vec<String>,
) -> Result<Opened, LspFailure> {
    if let Some(source) = sources.get(anchor).await {
        client
            .open_document(anchor.to_owned(), &source.content)
            .await
            .map_err(|error| LspFailure::from_engine(&error))?;
    }
    let mut eligible = Vec::new();
    let mut opened = Opened::default();
    for file in files {
        cancel.check().map_err(LspFailure::cancelled)?;
        let Some(source) = sources.get(file).await else {
            if sources.is_too_large(file) {
                opened.too_large.push(file.clone());
            } else {
                opened.failed.push(file.clone());
            }
            continue;
        };
        let mut found = names_in(file)
            .iter()
            .flat_map(|name| occurrences(&source.content, name))
            .collect::<Vec<_>>();
        found.sort_unstable();
        found.dedup();
        let mut spots = found
            .into_iter()
            .map(|(line, character, is_call)| Spot {
                line,
                character,
                is_call,
                binding: None,
            })
            .collect::<Vec<_>>();
        if spots.is_empty() {
            continue;
        }
        classify_occurrences(cancel, &source, file, &mut spots).await?;
        eligible.push((file.clone(), spots));
    }
    let last = eligible.len().saturating_sub(1);
    for (index, (file, spots)) in eligible.into_iter().enumerate() {
        cancel.check().map_err(LspFailure::cancelled)?;
        let Some(source) = sources.get(&file).await else {
            opened.failed.push(file);
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
            opened.files.push((file, spots));
        } else {
            opened.failed.push(file);
        }
    }
    Ok(opened)
}

/// Settle each occurrence's is-call by TypeScript's call-site rule, from one
/// oxc parse of the file (see [`classify_sites`]): an implementing method's
/// own `name(` is no call, and `name<T>(…)`, `` name`…` ``, `@name`,
/// `<Name />` and `x.name` are. An occurrence the parse leaves undecided (a
/// comment), or a file oxc cannot parse, keeps the text rule. The same
/// parse names the binding of each occurrence.
async fn classify_occurrences(
    cancel: &dyn CancellationCheck,
    source: &Arc<Source>,
    path: &str,
    spots: &mut [Spot],
) -> Result<(), LspFailure> {
    let starts = spots
        .iter()
        .map(|spot| GraphPosition {
            line: spot.line,
            character: spot.character,
        })
        .collect();
    if let Some(sites) = classify_sites(cancel, source, path, starts).await? {
        for (spot, site) in spots.iter_mut().zip(sites) {
            if let Some(is_call) = site.is_call {
                spot.is_call = is_call;
            }
            spot.binding = site.binding;
        }
    }
    Ok(())
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
    /// Files without an anchor whose sites bind the anchor package's build
    /// output where no source declaration mirrors it ([`through_output`]).
    pub(super) package: Vec<String>,
}

/// Classify each occurrence by what its definition chain reaches (see
/// [`binding`](fn@binding)).
pub(super) async fn verify_occurrences(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    opened: Vec<OpenedFile>,
    (declaration, output): (&HashSet<String>, Option<&BuildOutput>),
    empty_is_unknown: bool,
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
                (declaration, output),
                &missing,
                empty_is_unknown,
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
                    (declaration, output),
                    &missing,
                    empty_is_unknown,
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
        } else if verdict.output {
            verified.package.push(verdict.file);
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
pub(in super::super) async fn ordered_concurrently<I, T, Fut>(
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
pub(in super::super) const CONCURRENT_FILE_VERIFICATIONS: usize = 8;

/// What the definition chains of one candidate's occurrences proved.
pub(super) struct FileVerdict {
    pub(super) file: String,
    pub(super) anchors: Vec<Anchor>,
    /// A site was undecided ([`Binding::Undecided`]), so without an anchor
    /// the file stays unverified rather than rejected.
    pub(super) unresolved: bool,
    /// A site bound the anchor package's build output that no source
    /// declaration mirrors: without an anchor the file is a package
    /// importer, neither rejected nor verified.
    pub(super) output: bool,
    /// Zero-based `(line, character)` of sites that bind another declaration.
    pub(super) elsewhere: Vec<(u32, u32)>,
}

pub(super) async fn verify_file(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    (file, spots): OpenedFile,
    (declaration, build): (&HashSet<String>, Option<&BuildOutput>),
    missing: &(impl Fn(&Path) -> bool + Sync),
    empty_is_unknown: bool,
) -> Result<FileVerdict, LspFailure> {
    let content = match sources.get(&file).await {
        Some(source) => source.content.clone(),
        None => String::new(),
    };
    let mut anchors = Vec::new();
    let mut unresolved = false;
    let mut output = false;
    let mut elsewhere = Vec::new();
    // One verified anchor answers for the whole file's program; keep
    // checking only until a verified call site is found as well.
    let mut verified_any = false;
    // What each of the file's bindings was proven to name: every other
    // site of a binding names the same declaration, so it is decided
    // without a request. An undecided binding is asked again at its next
    // site.
    let mut settled = HashMap::<u32, Binding>::new();
    for spot in spots {
        let Spot {
            line: spot_line,
            character: spot_character,
            is_call,
            binding: named,
        } = spot;
        if verified_any && !is_call {
            continue;
        }
        cancel.check().map_err(LspFailure::cancelled)?;
        let known = named.and_then(|id| settled.get(&id).copied());
        let verdict = match known {
            Some(verdict) => verdict,
            None => {
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
                // A package imported by name binds its build output: the
                // anchor's source stands for the declaration compiled from it.
                let translated = through_output(resolved, build, declaration, sources).await;
                let resolved = translated.resolved;
                if translated.unmapped && resolved.is_empty() {
                    output = true;
                    continue;
                }
                // A server with no project behind a file answers nothing: that
                // is no other declaration. A comment says nothing either way.
                if empty_is_unknown && resolved.is_empty() {
                    let line = content.lines().nth(spot_line as usize).unwrap_or_default();
                    let comment = octocode_engine::signatures::is_comment_line(&file, line);
                    if comment != Some(true) {
                        unresolved = true;
                    }
                    continue;
                }
                let verdict = binding(&resolved, &file, declaration, &content, missing);
                if let Some(id) = named
                    && verdict != Binding::Undecided
                {
                    settled.insert(id, verdict);
                }
                verdict
            }
        };
        match verdict {
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
        output,
        elsewhere,
    })
}
