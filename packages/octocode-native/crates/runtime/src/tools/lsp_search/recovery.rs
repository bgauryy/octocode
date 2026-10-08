//! Multi-hop location recovery: the definition chain (follow re-exports and
//! aliases to the declaration) and alias-aware reference recovery. Every hop
//! target is server-supplied, so it is read only through the request's
//! policy-gated [`SourceCache`], and location reads go through the engine
//! snippet policy.

use super::cancellable;
use super::failure::LspFailure;
use super::render::uri_to_path;
use super::source::SourceCache;
use crate::tools::cancel::CancellationCheck;
use octocode_engine::lsp::client::{
    LocationRequest, NativeLspClient, SNIPPET_CONTENT_WITHHELD, SnippetReadPolicy,
};
use octocode_engine::lsp::types::JsCodeSnippet;
use serde_json::Value;
use std::collections::{BTreeSet, HashSet};
use std::time::Duration;

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
) -> Result<Vec<JsCodeSnippet>, LspFailure> {
    settled_locations(
        client,
        snippet_policy,
        cancel,
        request,
        (path, line, character),
        None,
    )
    .await
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
) -> Result<Vec<JsCodeSnippet>, LspFailure> {
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
    snippets.retain(|snippet| snippet.content != SNIPPET_CONTENT_WITHHELD);
    Ok(snippets)
}

/// Definition identity: `(canonical path, range)`, so a symlink and its
/// target (or two spellings of one path) name the same definition, as the
/// hierarchy walk's node keys do. A path that cannot be canonicalized (gone,
/// or not local) falls back to its decoded form.
pub(super) fn snippet_identity(snippet: &JsCodeSnippet) -> String {
    let path = super::scope::canonical(&uri_to_path(&snippet.uri));
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
/// unauthorized target is not followed.
pub(super) async fn resolve_definition_chain(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    path: &str,
    line: u32,
    character: u32,
) -> Result<(Vec<JsCodeSnippet>, Vec<String>), LspFailure> {
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
    let mut current = definition(path.to_owned(), line, character).await?;
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
                if failure.code == "cancelled" {
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
                if should_retry_definition_hop(depth, path, &target, has_distinct_target) {
                    // Ask again after the settle; never the cached early answer.
                    nested = settled_locations(
                        client,
                        snippet_policy,
                        cancel,
                        LocationRequest::Definition,
                        (&target, hop_line, hop_character),
                        Some(Duration::from_millis(DEFINITION_ALIAS_SETTLE_MS)),
                    )
                    .await;
                }
            }
            let nested = match nested {
                Ok(nested) => nested,
                Err(failure) if failure.code == "cancelled" => return Err(failure),
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
    Ok((current, warnings))
}

fn hop_failure_warning(snippet: &JsCodeSnippet, depth: usize, failure: &LspFailure) -> String {
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

/// References through aliasing imports (`import { x as y }`) the server did
/// not report: for each file that already references the symbol, find
/// imports that rename it, confirm by definition identity that the alias is
/// the same symbol, and add the alias's references. Bounded by
/// [`MAX_ALIAS_FILES_READ`] files read and [`MAX_ALIAS_IMPORTS`] verified
/// imports per request; a cap that leaves files or imports unchecked sets
/// [`AliasRecovery::capped`].
#[allow(clippy::too_many_arguments)]
pub(super) async fn recover_aliases(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    symbol: Option<&str>,
    include_declaration: bool,
    path: &str,
    line: u32,
    character: u32,
    provider: &[JsCodeSnippet],
) -> Result<AliasRecovery, LspFailure> {
    let Some(symbol) = symbol else {
        return Ok(AliasRecovery::default());
    };
    let definition_ids = match get_locations(
        client,
        snippet_policy,
        cancel,
        LocationRequest::Definition,
        path,
        line,
        character,
    )
    .await
    {
        Ok(definitions) => definitions
            .iter()
            .map(snippet_identity)
            .collect::<HashSet<_>>(),
        Err(failure) if failure.code == "cancelled" => return Err(failure),
        Err(_) => return Ok(AliasRecovery::default()),
    };
    if definition_ids.is_empty() {
        return Ok(AliasRecovery::default());
    }
    let provider_ids = provider
        .iter()
        .map(snippet_identity)
        .collect::<HashSet<_>>();
    // BTreeSet: deduplicated and deterministic.
    let files = provider
        .iter()
        .map(|snippet| uri_to_path(&snippet.uri))
        .collect::<BTreeSet<_>>();
    let mut recovered = Vec::new();
    let mut capped = false;
    let mut files_read = 0usize;
    let mut imports_checked = 0usize;
    for file in files {
        if files_read >= MAX_ALIAS_FILES_READ || imports_checked >= MAX_ALIAS_IMPORTS {
            capped = true;
            break;
        }
        cancel.check().map_err(LspFailure::cancelled)?;
        let reads_before = sources.reads();
        let source = sources.get(&file).await;
        files_read += sources.reads() - reads_before;
        let Some(source) = source else {
            continue;
        };
        for (local_line, local_character) in aliasing_imports(&source.content, &file, symbol) {
            if imports_checked >= MAX_ALIAS_IMPORTS {
                capped = true;
                break;
            }
            imports_checked += 1;
            let Ok(targets) = get_locations(
                client,
                snippet_policy,
                cancel,
                LocationRequest::Definition,
                &file,
                local_line,
                local_character,
            )
            .await
            else {
                cancel.check().map_err(LspFailure::cancelled)?;
                continue;
            };
            if targets.iter().map(snippet_identity).collect::<HashSet<_>>() != definition_ids {
                continue;
            }
            if let Ok(extra) = get_locations(
                client,
                snippet_policy,
                cancel,
                LocationRequest::References {
                    include_declaration,
                },
                &file,
                local_line,
                local_character,
            )
            .await
            {
                recovered.extend(
                    extra
                        .into_iter()
                        .filter(|snippet| !provider_ids.contains(&snippet_identity(snippet))),
                );
            }
        }
    }
    Ok(AliasRecovery {
        snippets: recovered,
        capped,
    })
}

/// What alias recovery found, and whether a cap left candidates unchecked.
#[derive(Debug, Default)]
pub(super) struct AliasRecovery {
    pub(super) snippets: Vec<JsCodeSnippet>,
    pub(super) capped: bool,
}

/// Partial reason of a references row whose alias scan stopped at a cap.
pub(super) const ALIAS_SCAN_CAPPED_REASON: &str = "aliasScanCapped";

/// Disclose a capped alias scan: `coverage.aliasScan:"capped"`, a partial
/// reason with a warning, and the lexical `hints.textSearch` that reaches the
/// aliases left unchecked.
pub(super) fn disclose_alias_cap(
    row: &mut Value,
    query: &super::LspSearchQuery,
    scope: &super::scope::Scope,
) {
    if let Some(coverage) = row.pointer_mut("/payload/coverage") {
        coverage["aliasScan"] = serde_json::json!("capped");
    }
    super::failure::flag_partial(
        row,
        query,
        ALIAS_SCAN_CAPPED_REASON,
        &format!(
            "Alias recovery checked at most {MAX_ALIAS_FILES_READ} files and {MAX_ALIAS_IMPORTS} renaming imports; references through the unchecked aliases may be missing. Confirm with hints.textSearch."
        ),
        scope,
    );
}

/// Whether `source` spells a rename of `symbol`: the word followed by `as`
/// (`import { f as g }`, `use a::f as g`) or by one `:` (`const { f: g }`).
/// Files without one cannot hold an aliasing import, so they are not parsed.
pub(super) fn may_rename(source: &str, symbol: &str) -> bool {
    if symbol.is_empty() {
        return false;
    }
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let mut from = 0;
    while let Some(offset) = source[from..].find(symbol) {
        let start = from + offset;
        let end = start + symbol.len();
        from = end;
        if source[..start].chars().next_back().is_some_and(is_word)
            || source[end..].chars().next().is_some_and(is_word)
        {
            continue;
        }
        let rest = source[end..].trim_start();
        let renamed = rest
            .strip_prefix("as")
            .is_some_and(|after| after.starts_with(char::is_whitespace))
            || (rest.starts_with(':') && !rest.starts_with("::"));
        if renamed {
            return true;
        }
    }
    false
}

/// Zero-based positions of the local names of imports in `source` that
/// rename `symbol` (`imported == symbol`, `local != imported`).
fn aliasing_imports(source: &str, file: &str, symbol: &str) -> Vec<(u32, u32)> {
    if !may_rename(source, symbol) {
        return Vec::new();
    }
    let Some(facts) = octocode_engine::portable::extract_graph_facts(source, file) else {
        return Vec::new();
    };
    let Ok(parsed) = serde_json::from_str::<Value>(&facts) else {
        return Vec::new();
    };
    parsed
        .get("imports")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|import| {
            let imported = import.get("importedName").and_then(Value::as_str)?;
            let local = import.get("localName").and_then(Value::as_str)?;
            if imported != symbol || local == imported {
                return None;
            }
            let start = import.get("localRange")?.get("start")?;
            let line = u32::try_from(start.get("line")?.as_u64()?).ok()?;
            let character = u32::try_from(start.get("character")?.as_u64()?).ok()?;
            Some((line, character))
        })
        .collect()
}
