//! Keeping a finished index honest: open what it cannot speak for before asking.
use super::index_proof::open_and_index;
use super::*;
use crate::lsp_search::recovery::get_locations;
use crate::lsp_search::scope::canonical_uri;
use octocode_engine::lsp::client::LocationRequest;

/// What [`freshen_index`] did.
pub(in super::super) enum Freshened {
    /// Every file the index cannot speak for is open. These are the ones it did
    /// not list: a text-only one among them is unproven.
    Opened(HashSet<String>),
    /// More such files than one request opens: answer as if there were no index.
    TooMany,
}

/// Before asking a server with a finished index: open what that index cannot
/// speak for, so the answer holds it. Those are the files that spell `symbol` and
/// that it did not index or that changed since the server started (a new file, a
/// monorepo's other project, a stale database, flags that failed, an edit), and
/// every listed file that changed, whatever it spells (a call an edit removed, or
/// added through a macro). A file the index holds as it is on disk needs nothing:
/// if it spells the name and is unlisted, the name there is another symbol or
/// disabled code.
pub(in super::super) async fn freshen_index(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    search: IndexWindow<'_>,
    anchor: Position<'_>,
) -> Result<Freshened, LspFailure> {
    let IndexWindow { symbol, scope, .. } = search;
    let answered = get_locations(
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
    .await
    .unwrap_or_default()
    .iter()
    .map(|location| canonical_uri(&location.uri))
    .collect::<HashSet<_>>();
    let spelled = candidate_files(
        scope,
        symbol,
        &HashSet::from([canonical(anchor.path)]),
        sources.policy(),
        cancel,
    )
    .await?
    .unwrap_or_default();
    let indexed = client.indexed_names();
    let unknown = spelled
        .into_iter()
        .filter(|file| !answered.contains(file) && !client.index_covers(file, &indexed))
        .collect::<HashSet<_>>();
    let mut stale = client
        .changed_listed_files()
        .into_iter()
        .chain(
            answered
                .into_iter()
                .filter(|file| client.changed_since_start(file)),
        )
        .filter(|file| !unknown.contains(file) && file_exists(file))
        .collect::<Vec<_>>();
    stale.sort();
    stale.dedup();
    if unknown.len() + stale.len() > MAX_CANDIDATE_FILES {
        return Ok(Freshened::TooMany);
    }
    stale.extend(unknown.iter().cloned());
    if !stale.is_empty() {
        open_and_index(client, sources, cancel, anchor.path, &stale).await?;
        client.forget_recorded_answers();
    }
    Ok(Freshened::Opened(unknown))
}

fn file_exists(file: &str) -> bool {
    Path::new(file).is_file()
}
