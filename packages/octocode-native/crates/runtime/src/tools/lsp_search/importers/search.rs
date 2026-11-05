//! The importer search: verified anchors from candidate files whose imports
//! name the symbol.
use super::*;

pub(in super::super) const SCAN_COMPLETE: &str = "complete";

pub(in super::super) const SCAN_CAPPED: &str = "capped";

pub(in super::super) const SCAN_FAILED: &str = "failed";

/// What [`verified_anchors`] verifies: importers of `symbol`, declared at
/// `declaration`, among `scope`'s candidates in the importer `window` the
/// request names. `known_files` are files the server already reported; they
/// are not candidates.
pub(in super::super) struct ImporterSearch<'a> {
    pub(in super::super) symbol: &'a str,
    pub(in super::super) scope: &'a Scope,
    pub(in super::super) declaration: Position<'a>,
    pub(in super::super) known_files: &'a HashSet<String>,
    pub(in super::super) window: WindowRequest<'a>,
    pub(in super::super) proof: SiteProof,
    /// Whether the language's imports can set candidates aside: ES and CommonJS
    /// name their symbols, so a file importing only other modules is no use.
    pub(in super::super) reads_imports: bool,
}

/// The answer with no candidate list: a later window cannot be placed.
fn no_candidates(stale: bool, failed: bool) -> Importers {
    Importers {
        failed,
        stale,
        ..Importers::default()
    }
}

/// Verified importer anchors of one [`ImporterSearch`].
pub(in super::super) async fn verified_anchors(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    search: ImporterSearch<'_>,
) -> Result<Importers, LspFailure> {
    let ImporterSearch {
        symbol,
        scope,
        declaration:
            Position {
                path: anchor_path,
                line,
                character,
            },
        known_files,
        window: request,
        proof,
        reads_imports,
    } = search;
    if symbol.trim().is_empty() || scope.root.is_empty() {
        return Ok(no_candidates(request.page > 1, false));
    }
    // The list cut into windows and digested is the text scan less the
    // anchor and the files whose imports cannot reach its module: both
    // follow from the sources alone. The server's own answer (which varies
    // with what it has loaded) only excludes files from verification, so a
    // later window names the same files from any process.
    let anchor_file = canonical(anchor_path);
    let Some(candidates) = candidate_files(
        scope,
        symbol,
        &HashSet::from([anchor_file]),
        sources.policy(),
        cancel,
    )
    .await?
    else {
        return Ok(no_candidates(request.page > 1, true));
    };
    // Module reach reads ES and CommonJS imports; an `#include` names no symbol.
    let module = if reads_imports {
        anchor_module(sources, anchor_path, symbol).await
    } else {
        None
    };
    let (candidates, unreached) =
        split_reachable(candidates, module.clone(), sources.policy(), cancel).await?;
    let mut window = match Window::cut(candidates, request) {
        Cut::Window(window) => window,
        Cut::Stale => {
            return Ok(Importers {
                stale: true,
                ..Importers::default()
            });
        }
    };
    // On an index-backed server a later window's files are in the anchor's own
    // answer only because this request opened them; they are this page's rows.
    if proof == SiteProof::DefinitionChain || request.page <= 1 {
        window.exclude(known_files.clone());
    }
    let skip = known_files
        .iter()
        .cloned()
        .chain(std::iter::once(canonical(anchor_path)))
        .collect::<HashSet<_>>();
    let mut ignored = scope
        .ignored_text_files(symbol, &skip, sources.policy(), cancel)
        .await?;
    // Nested repositories are other repositories: only one holding the
    // anchor is a cut of this answer's scope.
    let anchor_file = canonical(anchor_path);
    ignored
        .nested_repositories
        .retain(|repo| Path::new(&anchor_file).starts_with(Path::new(&scope.root).join(repo)));
    sort_ignored(
        &mut ignored,
        reads_imports,
        sources.policy(),
        cancel,
        symbol,
        module,
    )
    .await?;
    let Some(declaration) = anchor_declaration(
        proof,
        (client, sources, snippet_policy, cancel),
        Position {
            path: anchor_path,
            line,
            character,
        },
    )
    .await?
    else {
        return Ok(Importers {
            window: Some(window),
            failed: true,
            rejected: unreached,
            ignored,
            ..Importers::default()
        });
    };
    let started = std::time::Instant::now();
    let mut anchors = Vec::new();
    let mut rejected = unreached;
    let mut unverified = Vec::new();
    let mut elsewhere = HashSet::new();
    loop {
        let (opened, open_failed) = open_candidates(
            client,
            sources,
            cancel,
            anchor_path,
            &window.files(),
            symbol,
        )
        .await?;
        let verified = verify_window(
            (client, sources, snippet_policy, cancel),
            opened,
            WindowProof {
                proof,
                declaration: &declaration,
                reads_imports,
                anchor: Position {
                    path: anchor_path,
                    line,
                    character,
                },
                symbol,
            },
        )
        .await?;
        anchors.extend(verified.anchors);
        rejected.extend(verified.rejected);
        unverified.extend(verified.unverified.into_iter().chain(open_failed));
        elsewhere.extend(verified.elsewhere);
        // A window with no verified importer adds no site, so the request
        // goes on to the next one rather than ending a call on nothing new.
        // An index-backed server answers only what `index_window` opened, so
        // this request judges that one window; the rest are `next.nextImporterWindow`.
        if proof == SiteProof::AnchorReferences
            || !anchors.is_empty()
            || !window.more()
            || window.verified_windows() >= MAX_WINDOWS_PER_REQUEST
            || started.elapsed() >= SKIP_BUDGET
        {
            break;
        }
        window.advance();
    }
    Ok(Importers {
        anchors,
        window: Some(window),
        stale: false,
        failed: !unverified.is_empty(),
        rejected,
        unverified,
        elsewhere,
        ignored,
    })
}

/// [`check_imports`] where a language's imports name their symbols; elsewhere no
/// import does, so a file that spells the name may use it.
async fn sort_ignored(
    ignored: &mut IgnoredMentions,
    reads_imports: bool,
    policy: &PathPolicy,
    cancel: &dyn CancellationCheck,
    symbol: &str,
    module: Option<Arc<AnchorModule>>,
) -> Result<(), LspFailure> {
    if reads_imports {
        return check_imports(ignored, policy, cancel, symbol, module).await;
    }
    ignored.importing = ignored.files.clone();
    Ok(())
}

/// What a site's definition chain must reach; an index-backed proof compares with
/// the anchor's references instead. `None` when the chain could not be resolved.
async fn anchor_declaration(
    proof: SiteProof,
    (client, sources, snippet_policy, cancel): (
        &NativeLspClient,
        &mut SourceCache<'_>,
        &SnippetReadPolicy,
        &dyn CancellationCheck,
    ),
    anchor: Position<'_>,
) -> Result<Option<HashSet<String>>, LspFailure> {
    if proof == SiteProof::AnchorReferences {
        return Ok(Some(HashSet::new()));
    }
    let found = identities(
        client,
        sources,
        snippet_policy,
        cancel,
        anchor.path,
        anchor.line,
        anchor.character,
    )
    .await?;
    Ok(found.filter(|found| !found.is_empty()))
}

/// How one window's opened candidates are judged.
struct WindowProof<'a> {
    proof: SiteProof,
    declaration: &'a HashSet<String>,
    reads_imports: bool,
    anchor: Position<'a>,
    symbol: &'a str,
}

async fn verify_window(
    (client, sources, snippet_policy, cancel): (
        &NativeLspClient,
        &mut SourceCache<'_>,
        &SnippetReadPolicy,
        &dyn CancellationCheck,
    ),
    opened: Vec<OpenedFile>,
    how: WindowProof<'_>,
) -> Result<Verified, LspFailure> {
    match how.proof {
        SiteProof::DefinitionChain => {
            verify_occurrences(
                client,
                sources,
                snippet_policy,
                cancel,
                opened,
                how.declaration,
                !how.reads_imports,
            )
            .await
        }
        SiteProof::AnchorReferences => {
            verify_by_references(
                client,
                sources,
                snippet_policy,
                cancel,
                opened,
                how.anchor,
                how.symbol,
            )
            .await
        }
    }
}

/// Sort `ignored.files` into those that import `symbol` (by name:
/// `import { symbol }`, `const { symbol } = require(…)`, renamed or not; or
/// through a namespace or default binding whose member `.symbol` the file
/// spells) from a module that may be the anchor's (see [`AnchorModule`];
/// any module when `module` is unknown), and those the check cannot decide:
/// refused by the policy, unreadable, larger than the parser's bound
/// ([`octocode_engine::signatures::MAX_PARSE_SIZE`]), unparsable, or not
/// reached before the request stopped. Every file is checked: one
/// cancellable blocking worker reads them in turn and keeps none (the server
/// never opens them, so no request cache holds them); its parses are
/// digest-cached for later windows.
pub(super) async fn check_imports(
    ignored: &mut IgnoredMentions,
    policy: &PathPolicy,
    cancel: &dyn CancellationCheck,
    symbol: &str,
    module: Option<Arc<AnchorModule>>,
) -> Result<(), LspFailure> {
    let files = ignored.files.clone();
    let (policy, symbol) = (policy.clone(), symbol.to_owned());
    let checked = super::super::blocking_cancellable(cancel, move |stopped| {
        let mut relays = HashMap::new();
        files
            .into_iter()
            .map(|file| {
                let imports = (!stopped())
                    .then(|| {
                        let content = read_checked(&policy, &file)?;
                        let facts = crate::tools::declarations::graph_facts(&content, &file)?;
                        Some(
                            imports_name(&facts, &content, &symbol)
                                && module.as_deref().is_none_or(|module| {
                                    module.bound_by(&file, &content, &facts, &mut relays)
                                }),
                        )
                    })
                    .flatten();
                (file, imports)
            })
            .collect::<Vec<_>>()
    })
    .await?;
    let Some(checked) = checked else {
        // The worker failed: nothing was decided.
        ignored.unchecked = ignored.files.clone();
        return Ok(());
    };
    for (file, imports) in checked {
        match imports {
            Some(true) => ignored.importing.push(file),
            Some(false) => {}
            None => ignored.unchecked.push(file),
        }
    }
    Ok(())
}

/// The anchor's module when it exports `symbol` itself (see
/// [`AnchorModule::of`]); `None` when the anchor cannot be read.
async fn anchor_module(
    sources: &mut SourceCache<'_>,
    anchor_path: &str,
    symbol: &str,
) -> Option<Arc<AnchorModule>> {
    let source = sources.get(anchor_path).await?;
    AnchorModule::of(anchor_path, &source.content, symbol).map(Arc::new)
}

/// Whether the parsed `content` imports `symbol` (see [`check_imports`]).
pub(in super::super) fn imports_name(
    facts: &octocode_engine::graph::GraphFactsDocument,
    content: &str,
    symbol: &str,
) -> bool {
    let member = format!(".{symbol}");
    let spells_member = || {
        octocode_engine::text::whole_word_starts(content, &member)
            .next()
            .is_some()
    };
    facts
        .imports
        .iter()
        .any(|import| match import.imported_name.as_deref() {
            Some(name) if name == symbol => true,
            Some("*" | "default") => spells_member(),
            _ => false,
        })
}
