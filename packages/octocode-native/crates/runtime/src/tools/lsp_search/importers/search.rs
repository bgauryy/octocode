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
    } = search;
    if symbol.trim().is_empty() || scope.root.is_empty() {
        // No candidate list: a later window cannot be placed.
        return Ok(Importers {
            stale: request.page > 1,
            ..Importers::default()
        });
    }
    let mut skip = known_files.clone();
    skip.insert(canonical(anchor_path));
    let Some(candidates) = candidate_files(scope, symbol, &skip, sources.policy(), cancel).await?
    else {
        return Ok(Importers {
            failed: true,
            stale: request.page > 1,
            ..Importers::default()
        });
    };
    let window = match Window::cut(candidates, request) {
        Cut::Window(window) => window,
        Cut::Stale => {
            return Ok(Importers {
                stale: true,
                ..Importers::default()
            });
        }
    };
    let mut ignored = scope
        .ignored_text_files(symbol, &skip, sources.policy(), cancel)
        .await?;
    check_imports(&mut ignored, sources.policy(), cancel, symbol).await?;
    let declaration = identities(
        client,
        sources,
        snippet_policy,
        cancel,
        anchor_path,
        line,
        character,
    )
    .await?;
    let Some(declaration) = declaration.filter(|declaration| !declaration.is_empty()) else {
        return Ok(Importers {
            window: Some(window),
            failed: true,
            ignored,
            ..Importers::default()
        });
    };
    let mut window = window;
    let started = std::time::Instant::now();
    let mut anchors = Vec::new();
    let mut rejected = Vec::new();
    let mut unverified = Vec::new();
    loop {
        let (opened, open_failed) =
            open_candidates(client, sources, cancel, anchor_path, window.files(), symbol).await?;
        let (found, other, unresolved) = verify_occurrences(
            client,
            sources,
            snippet_policy,
            cancel,
            opened,
            &declaration,
        )
        .await?;
        anchors.extend(found);
        rejected.extend(other);
        unverified.extend(unresolved.into_iter().chain(open_failed));
        // The first window lists the anchor's own answer; a later one with
        // no verified importer would list nothing, so the request goes on.
        let silent = window.first > 1 && anchors.is_empty();
        if !silent
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
        ignored,
    })
}

/// Sort `ignored.files` into those that import `symbol` (by name:
/// `import { symbol }`, `const { symbol } = require(…)`, renamed or not; or
/// through a namespace or default binding whose member `.symbol` the file
/// spells) and those the check cannot decide: refused by the policy,
/// unreadable, larger than [`CODE_CHECK_MAX_BYTES`], unparsable, or past
/// [`CODE_CHECK_MAX_FILES`]. One blocking worker reads the files in turn and
/// keeps none: the server never opens them, so no request cache holds them.
pub(super) async fn check_imports(
    ignored: &mut IgnoredMentions,
    policy: &PathPolicy,
    cancel: &dyn CancellationCheck,
    symbol: &str,
) -> Result<(), LspFailure> {
    let files = ignored.files.clone();
    let (policy, symbol) = (policy.clone(), symbol.to_owned());
    let checked = super::super::blocking_cancellable(cancel, move |stopped| {
        files
            .into_iter()
            .enumerate()
            .map(|(index, file)| {
                let imports = (index < CODE_CHECK_MAX_FILES && !stopped())
                    .then(|| import_of(&policy, &file, &symbol))
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

/// Whether the policy-readable, bounded `file` imports `symbol`; `None`
/// when it cannot be read or parsed.
pub(super) fn import_of(policy: &PathPolicy, file: &str, symbol: &str) -> Option<bool> {
    let valid = policy.validate_read(file).ok()?;
    let bytes = crate::tools::source::read_bounded(&valid.canonical, CODE_CHECK_MAX_BYTES).ok()?;
    imports_name(
        &super::super::source::admitted_text(&bytes).ok()?,
        file,
        symbol,
    )
}

/// Whether `content` imports `symbol` (see [`check_imports`]); `None` when
/// the file cannot be parsed.
pub(in super::super) fn imports_name(content: &str, file: &str, symbol: &str) -> Option<bool> {
    let parsed = octocode_engine::signatures::extract_graph_facts(content, file)
        .and_then(|facts| serde_json::from_str::<Value>(&facts).ok())?;
    let member = format!(".{symbol}");
    let spells_member = || {
        content.match_indices(&member).any(|(at, _)| {
            !content[at + member.len()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$')
        })
    };
    Some(
        parsed
            .get("imports")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(
                |import| match import.get("importedName").and_then(Value::as_str) {
                    Some(name) if name == symbol => true,
                    Some("*" | "default") => spells_member(),
                    _ => false,
                },
            ),
    )
}
