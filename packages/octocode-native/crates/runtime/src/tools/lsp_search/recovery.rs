//! Multi-hop location recovery: the definition chain (follow re-exports and
//! aliases to the declaration) and alias-aware reference recovery. Every hop
//! target is server-supplied, so it is read only through the request's
//! policy-gated [`SourceCache`], and location reads go through the engine
//! snippet policy.

use super::anchor::Position;
use super::cancellable;
use super::failure::{LspFailure, RequestFailures};
use super::render::uri_to_path;
use super::source::SourceCache;
use crate::execution::cancel::CancellationCheck;
use octocode_engine::lsp::client::{
    LocationRequest, NativeLspClient, SNIPPET_CONTENT_WITHHELD, SnippetReadPolicy,
};
use octocode_engine::lsp::types::LspCodeSnippet;
use serde_json::Value;
use std::collections::{BTreeSet, HashSet};
use std::time::Duration;

mod aliases;
#[cfg(test)]
pub(super) use aliases::may_rename;
pub(super) use aliases::{
    ALIAS_SCAN_CAPPED_REASON, AliasSearch, disclose_alias_cap, recover_aliases,
};

const DEFINITION_ALIAS_SETTLE_MS: u64 = 50;
/// Definition hops followed past the first answer.
const MAX_DEFINITION_HOPS: usize = 4;
/// Candidate files alias recovery may read and parse per request.
pub(super) const MAX_ALIAS_FILES_READ: usize = 32;
/// Aliasing imports alias recovery may verify with the server per request.
const MAX_ALIAS_IMPORTS: usize = 32;

/// Location request through the snippet policy; locations whose content the
/// policy withheld name unauthorized files and are dropped here.
pub(super) async fn get_locations(
    client: &NativeLspClient,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    request: LocationRequest,
    path: &str,
    line: u32,
    character: u32,
) -> Result<Vec<LspCodeSnippet>, LspFailure> {
    get_counted_locations(
        client,
        snippet_policy,
        cancel,
        request,
        (path, line, character),
    )
    .await
    .map(|(found, _)| found)
}

/// [`get_locations`], with how many locations the policy withheld (outside
/// the allowed read roots): an answer of only those is not an absence.
pub(super) async fn get_counted_locations(
    client: &NativeLspClient,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    request: LocationRequest,
    position: (&str, u32, u32),
) -> Result<(Vec<LspCodeSnippet>, usize), LspFailure> {
    settled_locations(client, snippet_policy, cancel, request, position, None).await
}

/// [`get_locations`] asked at least `settle` after the call (see the
/// engine's `get_locations`): a cached settled answer returns at once.
async fn settled_locations(
    client: &NativeLspClient,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    request: LocationRequest,
    (path, line, character): (&str, u32, u32),
    settle: Option<Duration>,
) -> Result<(Vec<LspCodeSnippet>, usize), LspFailure> {
    let mut snippets = cancellable(
        cancel,
        client.get_locations(
            request,
            path.to_owned(),
            line,
            character,
            snippet_policy,
            settle,
        ),
    )
    .await??;
    let answered = snippets.len();
    snippets.retain(|snippet| snippet.content != SNIPPET_CONTENT_WITHHELD);
    let withheld = answered - snippets.len();
    Ok((one_spelling(snippets).0, withheld))
}

/// [`get_locations`], also saying whether the server spelled any location
/// through a symlink (see [`one_spelling`]) and how many locations the
/// read policy withheld.
pub(super) async fn get_spelled_locations(
    client: &NativeLspClient,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    request: LocationRequest,
    (path, line, character): (&str, u32, u32),
) -> Result<(Vec<LspCodeSnippet>, bool, usize), LspFailure> {
    let mut snippets = cancellable(
        cancel,
        client.get_locations(
            request,
            path.to_owned(),
            line,
            character,
            snippet_policy,
            None,
        ),
    )
    .await??;
    // Locations outside the read roots are withheld, and counted: an
    // answer missing them is partial, not smaller.
    let before = snippets.len();
    snippets.retain(|snippet| snippet.content != SNIPPET_CONTENT_WITHHELD);
    let withheld = before - snippets.len();
    let (snippets, respelled) = one_spelling(snippets);
    Ok((snippets, respelled, withheld))
}

/// Each location once, named by its real path. A server that reaches one
/// file through a symlink (a workspace package linked under
/// `node_modules`) and through its real path reports the same site twice,
/// and its `includeDeclaration: false` filter knows the declaration under
/// one spelling only. Every location is renamed to its canonical path (as
/// [`snippet_identity`] keys it), later twins are dropped, and the flag says
/// whether any location was renamed.
pub(super) fn one_spelling(snippets: Vec<LspCodeSnippet>) -> (Vec<LspCodeSnippet>, bool) {
    let mut seen = HashSet::new();
    let mut respelled = false;
    let snippets = snippets
        .into_iter()
        .filter_map(|mut snippet| {
            let spelled = uri_to_path(&snippet.uri);
            let real = super::scope::canonical(&spelled);
            if real != spelled
                && let Ok(uri) = octocode_engine::lsp::uri::path_to_uri(&real)
            {
                snippet.uri = uri;
                respelled = true;
            }
            seen.insert(snippet_identity(&snippet)).then_some(snippet)
        })
        .collect();
    (snippets, respelled)
}

/// Definition identity: `(canonical path, range)`, so a symlink and its
/// target (or two spellings of one path) name the same definition, as the
/// hierarchy walk's node keys do. A path that cannot be canonicalized (gone,
/// or not local) falls back to its decoded form.
pub(super) fn snippet_identity(snippet: &LspCodeSnippet) -> String {
    let path = super::scope::canonical_uri(&snippet.uri);
    format!(
        "{}:{}:{}:{}:{}",
        path,
        snippet.range.start.line,
        snippet.range.start.character,
        snippet.range.end.line,
        snippet.range.end.character
    )
}

pub(super) fn should_retry_definition_hop(
    depth: usize,
    source_path: &str,
    target_path: &str,
    has_distinct_target: bool,
) -> bool {
    depth == 0 && source_path == target_path && !has_distinct_target
}

/// Follow definition hops (visited set, hop cap, stop when no hop advances).
/// Only authorized, bounded hop targets are read and synchronized; an
/// unauthorized target is not followed. Also counts the first answer's
/// locations the policy withheld.
pub(super) async fn resolve_definition_chain(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    path: &str,
    line: u32,
    character: u32,
) -> Result<(Vec<LspCodeSnippet>, Vec<String>, usize), LspFailure> {
    let definition = |target: String, line: u32, character: u32| async move {
        get_locations(
            client,
            snippet_policy,
            cancel,
            LocationRequest::Definition,
            &target,
            line,
            character,
        )
        .await
    };
    // The first answer's withheld count; a hop's only keeps its origin.
    let (mut current, withheld) = get_counted_locations(
        client,
        snippet_policy,
        cancel,
        LocationRequest::Definition,
        (path, line, character),
    )
    .await?;
    let mut visited = HashSet::new();
    let mut warnings = Vec::new();
    for depth in 0..MAX_DEFINITION_HOPS {
        let mut next = Vec::new();
        let mut advanced = false;
        for snippet in current.iter().cloned() {
            cancel.check().map_err(LspFailure::cancelled)?;
            let identity = snippet_identity(&snippet);
            if !visited.insert(identity.clone()) {
                next.push(snippet);
                continue;
            }
            let target = uri_to_path(&snippet.uri);
            let Some(source) = sources.get(&target).await else {
                next.push(snippet);
                continue;
            };
            if let Err(error) = cancellable(
                cancel,
                client.open_document(target.clone(), &source.content),
            )
            .await?
            {
                let failure = LspFailure::from_engine(&error);
                if failure.is_cancelled() {
                    return Err(failure);
                }
                warnings.push(hop_failure_warning(&snippet, depth, &failure));
                next.push(snippet);
                continue;
            }
            let (hop_line, hop_character) =
                (snippet.range.start.line, snippet.range.start.character);
            let mut nested = definition(target.clone(), hop_line, hop_character).await;
            if let Ok(found) = &nested {
                let has_distinct_target = found
                    .iter()
                    .any(|candidate| snippet_identity(candidate) != identity);
                // Locations name real paths (`one_spelling`); so does the
                // anchor here.
                let anchor = super::scope::canonical(path);
                if should_retry_definition_hop(depth, &anchor, &target, has_distinct_target) {
                    // Ask again after the settle; never the cached early answer.
                    nested = settled_locations(
                        client,
                        snippet_policy,
                        cancel,
                        LocationRequest::Definition,
                        (&target, hop_line, hop_character),
                        Some(Duration::from_millis(DEFINITION_ALIAS_SETTLE_MS)),
                    )
                    .await
                    .map(|(found, _)| found);
                }
            }
            let nested = match nested {
                Ok(nested) => nested,
                Err(failure) if failure.is_cancelled() => return Err(failure),
                Err(failure) => {
                    warnings.push(hop_failure_warning(&snippet, depth, &failure));
                    next.push(snippet);
                    continue;
                }
            };
            let nested = nested
                .into_iter()
                .filter(|candidate| snippet_identity(candidate) != identity)
                .collect::<Vec<_>>();
            if nested.is_empty() {
                next.push(snippet);
            } else {
                advanced = true;
                next.extend(nested);
            }
        }
        let mut identities = HashSet::new();
        next.retain(|snippet| identities.insert(snippet_identity(snippet)));
        current = next;
        if !advanced {
            break;
        }
    }
    Ok((current, warnings, withheld))
}

fn hop_failure_warning(snippet: &LspCodeSnippet, depth: usize, failure: &LspFailure) -> String {
    format!(
        "Definition-provider follow-up hop {} at {}:{}:{} failed ({}): {}. The retained earlier location is a candidate, not verified terminal identity.",
        depth + 1,
        uri_to_path(&snippet.uri),
        snippet.range.start.line + 1,
        snippet.range.start.character + 1,
        failure.code,
        failure.message,
    )
}
