//! Proving candidate occurrences on a server that answers from an index of
//! what it has opened: clangd before its background index has finished, or with
//! none (no compile database, or the user turned it off). Text decides which files
//! to open, so a use that does not spell the name (a macro, an alias, an operator)
//! can stay unseen: every row from this path says it is partial (`Operation::run`).
//!
//! clangd's `references` come from its AST and index, so they already apply
//! the language's scoping, overloads and macros. Its `definition` cannot prove
//! a site: on a definition it toggles to the declaration, and where the AST has
//! no answer (a comment, code the preprocessor disabled) it guesses from the
//! word (`locateSymbolTextually`). So once a window is open and parsed, a site
//! is a use exactly when the anchor's own references list it.
use super::*;
use crate::lsp_search::cancellable;
use crate::lsp_search::recovery::get_locations;
use crate::lsp_search::scope::canonical_uri;
use octocode_engine::lsp::client::LocationRequest;
use octocode_engine::lsp::types::LspCodeSnippet;

/// How a candidate's occurrence is proven a use of the anchor's symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum SiteProof {
    /// The site's definition chain reaches the anchor's declaration.
    DefinitionChain,
    /// The anchor's own references list the site, once the file is parsed.
    AnchorReferences,
}

impl SiteProof {
    pub(in super::super) fn of(language_id: Option<&str>) -> Self {
        if octocode_engine::lsp::family::is_c_family(language_id) {
            Self::AnchorReferences
        } else {
            Self::DefinitionChain
        }
    }
}

/// What an unlisted code site is. Hover reads the AST alone, with no textual
/// guess: nothing there means the preprocessor disabled it or it did not parse.
/// With a compile command the anchor's references are exact, so a name the AST
/// knows is another symbol. Without one clangd guesses each file's language (a
/// `.h` as Objective-C++), so one function can read as two; only a definition in
/// the site's own file then proves another symbol.
async fn unlisted_site(
    client: &NativeLspClient,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    file: &str,
    spot: Spot,
    has_database: bool,
) -> Result<SiteState, LspFailure> {
    let hover = cancellable(
        cancel,
        client.get_hover(file.to_owned(), spot.line, spot.character),
    )
    .await?;
    if !matches!(&hover, Ok(hover) if !hover.is_null()) {
        return Ok(SiteState::Unknown);
    }
    if has_database {
        return Ok(SiteState::OtherSymbol);
    }
    let targets = get_locations(
        client,
        snippet_policy,
        cancel,
        LocationRequest::Definition,
        file,
        spot.line,
        spot.character,
    )
    .await;
    Ok(match targets {
        Ok(targets)
            if !targets.is_empty()
                && targets
                    .iter()
                    .all(|target| canonical_uri(&target.uri) == file) =>
        {
            SiteState::OtherSymbol
        }
        Err(failure) if failure.is_cancelled() => return Err(failure),
        _ => SiteState::Unknown,
    })
}

/// What one candidate's occurrences are, given whether the anchor's references
/// list each one and what the AST knows about the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SiteState {
    /// The anchor's references list it.
    Listed,
    /// The AST names something else there: another symbol of the same name.
    OtherSymbol,
    /// The AST knows nothing there, and the line is code: disabled by the
    /// preprocessor under this compile command, or not parsed at all.
    Unknown,
    /// Only a comment or string spells the name.
    Mention,
}

/// The verdict of one candidate from its sites' states: any listed site is a
/// use; an unknown site with none listed leaves the file undecided.
pub(super) fn file_verdict(file: String, sites: Vec<(Spot, SiteState)>) -> FileVerdict {
    let mut anchors = Vec::new();
    let mut elsewhere = Vec::new();
    let mut unresolved = false;
    for (spot, state) in sites {
        match state {
            SiteState::Listed => anchors.push(Anchor {
                path: file.clone(),
                line: spot.line,
                character: spot.character,
                is_call: spot.is_call,
            }),
            SiteState::OtherSymbol => elsewhere.push((spot.line, spot.character)),
            SiteState::Unknown => unresolved = true,
            SiteState::Mention => {}
        }
    }
    FileVerdict {
        file,
        anchors,
        unresolved,
        elsewhere,
    }
}

/// Whether clangd reads a compile command for `anchor`'s directory.
fn compile_database_found(client: &NativeLspClient, anchor: &str) -> bool {
    let directory = Path::new(anchor).parent().unwrap_or(Path::new(anchor));
    octocode_engine::lsp::clangd_database::reads_compile_command(client.config(), directory)
}

/// Whether `location` starts at the zero-based `line`/`character` of `file`
/// (canonical).
fn lists(location: &LspCodeSnippet, file: &str, line: u32, character: u32) -> bool {
    location.range.start.line == line
        && location.range.start.character == character
        && canonical_uri(&location.uri) == file
}

/// Open the candidates of the request's window and wait until the server has
/// indexed them, before any question it answers from that index: clangd then
/// answers `references`, `callers`, `implementation` and `subtypes` for them
/// with its own scoping, callers' names and hierarchy, and no answer this
/// response records is older than the window. A later window also
/// drops what its continuation recorded for earlier ones. A candidate that
/// cannot be opened or indexed is judged later, by [`verify_by_references`].
pub(in super::super) async fn index_window(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    cancel: &dyn CancellationCheck,
    search: IndexWindow<'_>,
) -> Result<(), LspFailure> {
    let IndexWindow {
        symbol,
        scope,
        anchor,
        window: request,
    } = search;
    if symbol.trim().is_empty() || scope.root.is_empty() {
        return Ok(());
    }
    let Some(candidates) = candidate_files(
        scope,
        symbol,
        &HashSet::from([canonical(anchor)]),
        sources.policy(),
        cancel,
    )
    .await?
    else {
        return Ok(());
    };
    let Cut::Window(window) = Window::cut(candidates, request) else {
        return Ok(());
    };
    open_and_index(client, sources, cancel, anchor, &window.files()).await?;
    if request.page > 1 {
        client.forget_recorded_answers();
    }
    Ok(())
}

/// Open `files` beside `anchor` and wait until the server has indexed them. A
/// file that misses the wait stays unverified at the proof.
pub(super) async fn open_and_index(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    cancel: &dyn CancellationCheck,
    anchor: &str,
    files: &[String],
) -> Result<(), LspFailure> {
    if let Some(source) = sources.get(anchor).await {
        client
            .open_document(anchor.to_owned(), &source.content)
            .await
            .map_err(|error| LspFailure::from_engine(&error))?;
    }
    let mut opened = Vec::with_capacity(files.len());
    for file in files {
        cancel.check().map_err(LspFailure::cancelled)?;
        if let Some(source) = sources.get(file).await
            && client
                .open_document(file.clone(), &source.content)
                .await
                .is_ok()
        {
            opened.push(file.clone());
        }
    }
    cancellable(cancel, client.wait_for_parses(opened, PARSE_QUIET_MS)).await?;
    Ok(())
}

/// What [`index_window`] opens: the candidates of `symbol` in `scope`, less the
/// `anchor`, in the importer `window` the request names.
pub(in super::super) struct IndexWindow<'a> {
    pub(in super::super) symbol: &'a str,
    pub(in super::super) scope: &'a Scope,
    pub(in super::super) anchor: &'a str,
    pub(in super::super) window: WindowRequest<'a>,
}

/// How long the server may show no sign of work (CPU time, progress, any
/// message) before the wait for the candidates' parses ends. The wait itself
/// lasts as long as the server works, within the caller's deadline and its
/// cancel; a file not parsed by then is reported, never assumed.
const PARSE_QUIET_MS: u32 = 10_000;

/// [`verify_occurrences`] by [`SiteProof::AnchorReferences`]: wait until every
/// opened candidate is in the index, ask the anchor's references once, then
/// judge each site. clangd adds a file to its index just before it publishes
/// that file's diagnostics (`onMainAST`), on no other path: a request on the
/// file can be answered from a separate AST first, so only the publication
/// proves the index holds it. A wait or request that fails decides nothing:
/// the file stays unverified, so the row is partial, never a false absence.
pub(super) async fn verify_by_references(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    opened: Vec<OpenedFile>,
    anchor: Position<'_>,
    symbol: &str,
) -> Result<Verified, LspFailure> {
    let mut verified = Verified::default();
    let files = opened.iter().map(|(file, _)| file.clone()).collect();
    let answered = cancellable(cancel, client.wait_for_parses(files, PARSE_QUIET_MS)).await?;
    let mut parsed = Vec::with_capacity(opened.len());
    for (file, spots) in opened {
        if answered.contains(&file) {
            parsed.push((file, spots));
        } else {
            verified.unverified.push(file);
        }
    }
    // Recorded after [`index_window`], so this answer already holds the window.
    let listed = get_locations(
        client,
        snippet_policy,
        cancel,
        LocationRequest::References {
            include_declaration: true,
        },
        anchor.path,
        anchor.line,
        anchor.character,
    )
    .await;
    if let Err(failure) = &listed
        && failure.is_cancelled()
    {
        return listed.map(|_| verified);
    }
    let Ok(mut listed) = listed else {
        verified
            .unverified
            .extend(parsed.into_iter().map(|(file, _)| file));
        return Ok(verified);
    };
    // The declaration the anchor's own translation unit resolves names the
    // symbol too: without an index that unit lists uses in its own file only,
    // and a header opened alone (parsed as another language with no compile
    // command) spells the declaration under another identity, so no
    // references answer lists it.
    match get_locations(
        client,
        snippet_policy,
        cancel,
        LocationRequest::Declaration,
        anchor.path,
        anchor.line,
        anchor.character,
    )
    .await
    {
        Ok(declared) => listed.extend(declared),
        Err(failure) if failure.is_cancelled() => return Err(failure),
        Err(_) => {}
    }
    let has_database = compile_database_found(client, anchor.path);
    for (file, spots) in parsed {
        cancel.check().map_err(LspFailure::cancelled)?;
        let code_lines = match sources.get(&file).await {
            Some(source) => octocode_engine::lsp::resolver::code_occurrence_lines(
                &file,
                &source.content,
                symbol,
            ),
            None => None,
        };
        let mut sites = Vec::with_capacity(spots.len());
        for spot in spots {
            let state = if listed
                .iter()
                .any(|location| lists(location, &file, spot.line, spot.character))
            {
                SiteState::Listed
            } else if code_lines
                .as_ref()
                .is_some_and(|lines| !lines.contains(&(spot.line as usize)))
            {
                SiteState::Mention
            } else {
                unlisted_site(client, snippet_policy, cancel, &file, spot, has_database).await?
            };
            sites.push((spot, state));
        }
        let verdict = file_verdict(file, sites);
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

#[cfg(test)]
mod tests;
