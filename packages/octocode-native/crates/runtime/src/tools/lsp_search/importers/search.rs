//! The importer search: verified anchors from candidate files whose imports
//! name the symbol.
use super::*;

pub(in super::super) const SCAN_COMPLETE: &str = "complete";

pub(in super::super) const SCAN_CAPPED: &str = "capped";

pub(in super::super) const SCAN_FAILED: &str = "failed";

/// Every candidate was verified, and files the scan's rules keep out of it
/// (gitignored or hidden) import the name or could not be checked for an
/// import, or the scan for them did not finish: the row counts them and
/// leads to them.
pub(in super::super) const SCAN_SKIPPED: &str = "skippedFiles";

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
        failure: ScanFailure::CandidateList,
        stale,
        ..Importers::default()
    }
}

/// The candidates of one search, before they are cut into windows.
struct CandidateList {
    /// Sorted files that may import the symbol, under its own name or an
    /// alias a relay exports it under.
    candidates: Vec<String>,
    /// Files that spell the name and whose imports cannot reach its module.
    unreached: Vec<String>,
    /// The anchor's module, when the language's imports name their symbols.
    module: Option<Arc<AnchorModule>>,
    renamed: Renamed,
}

/// The list cut into windows and digested: the text scan less the anchor
/// and the files whose imports cannot reach its module, with the importers
/// of each alias a relay exports the symbol under. All of it follows from
/// the sources alone; the server's own answer (which varies with what it
/// has loaded) only excludes files from verification, so a later window
/// names the same files from any process. `None` when a text scan failed.
async fn candidate_list(
    sources: &mut SourceCache<'_>,
    cancel: &dyn CancellationCheck,
    scope: &Scope,
    (anchor_path, anchor_file, symbol): (&str, &String, &str),
    reads_imports: bool,
) -> Result<Option<CandidateList>, LspFailure> {
    let Some(candidates) = candidate_files(
        scope,
        symbol,
        &HashSet::from([anchor_file.clone()]),
        sources.policy(),
        cancel,
    )
    .await?
    else {
        return Ok(None);
    };
    // Module reach reads ES and CommonJS imports; an `#include` names no symbol.
    let module = if reads_imports {
        anchor_module(sources, scope, anchor_path, symbol).await
    } else {
        None
    };
    let (candidates, unreached) =
        split_reachable(candidates, module.clone(), sources.policy(), cancel).await?;
    // A relay may export the symbol under another name: its importers spell
    // only that name, so they join the list as candidates of their own.
    let Some(renamed) = Renamed::among(
        scope,
        module.as_ref(),
        &candidates,
        anchor_file,
        sources.policy(),
        cancel,
    )
    .await?
    else {
        return Ok(None);
    };
    let (candidates, unreached) = renamed.joined(candidates, unreached);
    Ok(Some(CandidateList {
        candidates,
        unreached,
        module,
        renamed,
    }))
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
    let anchor_file = canonical(anchor_path);
    let Some(listed) = candidate_list(
        sources,
        cancel,
        scope,
        (anchor_path, &anchor_file, symbol),
        reads_imports,
    )
    .await?
    else {
        return Ok(no_candidates(request.page > 1, true));
    };
    let CandidateList {
        candidates,
        unreached,
        module,
        renamed,
    } = listed;
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
    let ignored = ignored_mentions(
        scope,
        symbol,
        (known_files, &anchor_file),
        (sources.policy(), cancel),
        reads_imports.then_some(&module),
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
            failure: ScanFailure::AnchorDeclaration,
            rejected: unreached,
            ignored,
            ..Importers::default()
        });
    };
    let started = std::time::Instant::now();
    let mut anchors = Vec::new();
    let mut rejected = unreached;
    let mut unverified = Vec::new();
    let mut unopened = Vec::<String>::new();
    let mut elsewhere = HashSet::new();
    loop {
        let (opened, open_failed) = open_candidates(
            client,
            sources,
            cancel,
            anchor_path,
            &window.files(),
            &|file| renamed.names_in(symbol, file),
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
        unopened.extend(open_failed.iter().cloned());
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
    // What the server left undecided, the file's own module references may
    // decide: a name bound only from other modules is not the anchor's.
    let (unverified, elsewhere_bound) = split_bound(
        unverified,
        module,
        Arc::clone(&renamed.relays),
        sources.policy(),
        cancel,
    )
    .await?;
    rejected.extend(elsewhere_bound);
    unopened.retain(|file| unverified.contains(file));
    Ok(Importers {
        anchors,
        window: Some(window),
        stale: false,
        failed: !unverified.is_empty(),
        failure: ScanFailure::Candidates,
        rejected,
        unverified,
        unopened,
        elsewhere,
        ignored,
    })
}

/// The other names relays export the anchor's symbol under, by the files
/// that may import them.
#[derive(Default)]
struct Renamed {
    /// For each candidate (canonical) of an alias: the relay modules it may
    /// import the symbol from under another name, in the order found.
    relays: Arc<HashMap<String, Vec<Arc<AnchorModule>>>>,
}

impl Renamed {
    /// The aliases of `module`'s symbol among the `reachable` candidates
    /// ([`renamed_exports`]) and their importers: the files under the
    /// `scope` that spell an alias and may reach its relay
    /// ([`split_reachable`], as for the symbol itself). Each of those may
    /// rename it again, so the walk goes on from them; a relay's export is
    /// followed once, which ends a cycle of renames. None without a
    /// `module`: only a module's own export is imported by name. `Ok(None)`
    /// when a text scan failed.
    async fn among(
        scope: &Scope,
        module: Option<&Arc<AnchorModule>>,
        reachable: &[String],
        anchor_file: &str,
        policy: &PathPolicy,
        cancel: &dyn CancellationCheck,
    ) -> Result<Option<Self>, LspFailure> {
        let Some(module) = module else {
            return Ok(Some(Self::default()));
        };
        if std::env::var_os("OCTO_D7_BEFORE").is_some() {
            return Ok(Some(Self::default())); // TEMPORARY failing-first toggle
        }
        let own = |module: &AnchorModule| {
            let (path, name) = module.export();
            (path.to_path_buf(), name.to_owned())
        };
        let mut followed = HashSet::from([own(module)]);
        let mut relays = HashMap::<String, Vec<Arc<AnchorModule>>>::new();
        let mut pending = vec![(Arc::clone(module), reachable.to_vec())];
        while let Some((exporter, files)) = pending.pop() {
            for relay in renamed_exports(files, exporter, policy, cancel).await? {
                if !followed.insert(own(&relay)) {
                    continue;
                }
                let (relay_file, alias) = relay.export();
                let Some(spelling) = scope.files_spelling(alias, policy, cancel).await? else {
                    return Ok(None);
                };
                let importers = spelling
                    .into_iter()
                    .filter(|file| file != anchor_file && Path::new(file) != relay_file)
                    .collect();
                let (reached, _) =
                    split_reachable(importers, Some(Arc::clone(&relay)), policy, cancel).await?;
                for file in &reached {
                    relays
                        .entry(file.clone())
                        .or_default()
                        .push(Arc::clone(&relay));
                }
                pending.push((relay, reached));
            }
        }
        Ok(Some(Self {
            relays: Arc::new(relays),
        }))
    }

    /// The one sorted candidate list: the symbol's `candidates` and every
    /// alias's, and the `unreached` files less those an alias reaches.
    fn joined(
        &self,
        candidates: Vec<String>,
        unreached: Vec<String>,
    ) -> (Vec<String>, Vec<String>) {
        let candidates = candidates
            .into_iter()
            .chain(self.relays.keys().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let unreached = unreached
            .into_iter()
            .filter(|file| !self.relays.contains_key(file))
            .collect();
        (candidates, unreached)
    }

    /// The names whose occurrences in `file` (canonical) are checked: the
    /// `symbol`, and each alias the file is a candidate for.
    fn names_in(&self, symbol: &str, file: &str) -> Vec<String> {
        let mut names = vec![symbol.to_owned()];
        for relay in self.relays.get(file).into_iter().flatten() {
            let alias = relay.export().1;
            if !names.iter().any(|name| name == alias) {
                names.push(alias.to_owned());
            }
        }
        names
    }
}

/// The scope's ignored files that spell `symbol`, less the `known` files and
/// the `anchor` file, sorted by what their imports say: [`check_imports`]
/// against the anchor's module where the language's imports name their
/// symbols (`reads`); elsewhere no import does, so a file that spells the name may
/// use it.
async fn ignored_mentions(
    scope: &Scope,
    symbol: &str,
    (known, anchor): (&HashSet<String>, &String),
    (policy, cancel): (&PathPolicy, &dyn CancellationCheck),
    reads: Option<&Option<Arc<AnchorModule>>>,
) -> Result<IgnoredMentions, LspFailure> {
    let skip = known
        .iter()
        .chain(std::iter::once(anchor))
        .cloned()
        .collect::<HashSet<_>>();
    let mut ignored = scope
        .ignored_text_files(symbol, &skip, policy, cancel)
        .await?;
    // Nested repositories are other repositories: only one holding the
    // anchor is a cut of this answer's scope.
    ignored
        .nested_repositories
        .retain(|repo| Path::new(anchor).starts_with(Path::new(&scope.root).join(repo)));
    match reads {
        Some(module) => {
            check_imports(&mut ignored, policy, cancel, symbol, module.clone()).await?;
        }
        None => ignored.importing = ignored.files.clone(),
    }
    Ok(ignored)
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
/// ([`crate::security::SourceLimits::whole_source`]), unparsable, or not
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
/// [`AnchorModule::of`]), in the project of the `scope`'s listed files;
/// `None` when the anchor cannot be read.
async fn anchor_module(
    sources: &mut SourceCache<'_>,
    scope: &Scope,
    anchor_path: &str,
    symbol: &str,
) -> Option<Arc<AnchorModule>> {
    let source = sources.get(anchor_path).await?;
    let module = AnchorModule::of(anchor_path, &source.content, symbol)?;
    Some(Arc::new(module.in_scope(scope, symbol, sources.policy())))
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
