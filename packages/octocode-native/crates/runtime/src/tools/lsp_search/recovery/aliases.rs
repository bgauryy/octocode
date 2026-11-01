//! Alias-aware reference recovery, driven by the server's answer. A
//! reference that is the imported name of a renaming import (`import { x as
//! y }`, `use a::x as y`, `from a import x as y`) names an alias whose uses
//! the server may not have listed. The answer is the only input: its own
//! reference proves the import binds the symbol, so the work is bounded by
//! the renaming imports it reaches.
//!
//! - The answer lists the alias binding itself: the server followed the
//!   rename (typescript-language-server), and nothing is recovered.
//! - Otherwise the server is asked for references at the binding, and the
//!   sites beyond the answer are recovered (pyright).
//! - A server that answers the binding with the imported symbol's own
//!   references (rust-analyzer) adds nothing; then each occurrence of the
//!   local name in that file is verified by its definition chain, as
//!   importer recovery verifies candidates.
use super::*;
use crate::tools::lsp_search::importers::{self, Binding};
use octocode_engine::lsp::types::{LspExactPosition, LspRange};
use std::collections::BTreeMap;

/// The references request [`recover_aliases`] extends: the anchor's
/// `symbol`, whether declarations count, and the references the server
/// (`provider`) already reported.
pub(in super::super) struct AliasSearch<'a> {
    pub(in super::super) symbol: Option<&'a str>,
    pub(in super::super) include_declaration: bool,
    pub(in super::super) anchor: Position<'a>,
    pub(in super::super) provider: &'a [LspCodeSnippet],
}

/// One renaming import the answer reached: its file, the local name, and
/// the zero-based start of the local binding.
struct Alias {
    file: String,
    local: String,
    line: u32,
    character: u32,
}

/// References through the renaming imports the answer reaches that the
/// server did not list (see the module notes). Failed requests are
/// recorded, each one an alias left unchecked; cancellation ends the
/// request.
pub(in super::super) async fn recover_aliases(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    search: AliasSearch<'_>,
) -> Result<AliasRecovery, LspFailure> {
    let AliasSearch {
        symbol,
        include_declaration,
        anchor,
        provider,
    } = search;
    let Some(symbol) = symbol else {
        return Ok(AliasRecovery::default());
    };
    let aliases = renaming_imports(sources, cancel, provider, symbol).await?;
    let listed = provider.iter().map(site).collect::<HashSet<_>>();
    let mut seen = provider
        .iter()
        .map(snippet_identity)
        .collect::<HashSet<_>>();
    let mut recovery = AliasRecovery::default();
    // The anchor's declaration, asked once and only when a server leaves an
    // alias's uses out of both answers.
    let mut declaration = None;
    for alias in aliases {
        if listed.contains(&(alias.file.clone(), alias.line, alias.character)) {
            continue;
        }
        let answer = match get_locations(
            client,
            snippet_policy,
            cancel,
            LocationRequest::References {
                include_declaration,
            },
            &alias.file,
            alias.line,
            alias.character,
        )
        .await
        {
            Ok(answer) => answer,
            Err(failure) => {
                recovery.failures.record(failure)?;
                continue;
            }
        };
        let extra = answer
            .into_iter()
            .filter(|snippet| seen.insert(snippet_identity(snippet)))
            .collect::<Vec<_>>();
        if !extra.is_empty() {
            recovery.snippets.extend(extra);
            continue;
        }
        if declaration.is_none() {
            declaration = Some(
                importers::identities(
                    client,
                    sources,
                    snippet_policy,
                    cancel,
                    anchor.path,
                    anchor.line,
                    anchor.character,
                )
                .await?
                .filter(|found| !found.is_empty()),
            );
        }
        let Some(Some(declared)) = &declaration else {
            recovery.failures.record(LspFailure::unestablished(format!(
                "the anchor's declaration did not resolve, so the uses of alias {} in {} are unverified",
                alias.local, alias.file
            )))?;
            continue;
        };
        let uses = verified_uses(client, sources, snippet_policy, cancel, &alias, declared).await?;
        recovery.snippets.extend(
            uses.into_iter()
                .filter(|snippet| seen.insert(snippet_identity(snippet))),
        );
    }
    Ok(recovery)
}

/// What alias recovery found, and the server requests that failed (each
/// one an alias left unchecked).
#[derive(Debug, Default)]
pub(in super::super) struct AliasRecovery {
    pub(in super::super) snippets: Vec<LspCodeSnippet>,
    pub(in super::super) failures: RequestFailures,
}

/// A location's file (as the answer names it) and zero-based start.
fn site(snippet: &LspCodeSnippet) -> (String, u32, u32) {
    (
        uri_to_path(&snippet.uri),
        snippet.range.start.line,
        snippet.range.start.character,
    )
}

/// The renaming imports whose imported name is one of the answer's sites,
/// from each answered file's digest-cached declarations parse; only a site
/// whose text spells a rename ([`may_rename`]) has its file parsed.
async fn renaming_imports(
    sources: &mut SourceCache<'_>,
    cancel: &dyn CancellationCheck,
    provider: &[LspCodeSnippet],
    symbol: &str,
) -> Result<Vec<Alias>, LspFailure> {
    let mut by_file = BTreeMap::<String, Vec<(u32, u32)>>::new();
    for snippet in provider
        .iter()
        .filter(|snippet| may_rename(&snippet.content, symbol))
    {
        let (file, line, character) = site(snippet);
        by_file.entry(file).or_default().push((line, character));
    }
    let mut aliases = Vec::new();
    for (file, sites) in by_file {
        cancel.check().map_err(LspFailure::cancelled)?;
        let Some(source) = sources.get(&file).await else {
            continue;
        };
        let (path, symbol) = (file.clone(), symbol.to_owned());
        let found = super::super::blocking_cancellable(cancel, move |_| {
            crate::tools::declarations_cache::file_document(&source.content, &path)
                .map(|document| aliasing_imports(&document, &symbol, &sites))
                .unwrap_or_default()
        })
        .await?
        .unwrap_or_default();
        aliases.extend(found.into_iter().map(|(local, line, character)| Alias {
            file: file.clone(),
            local,
            line,
            character,
        }));
    }
    Ok(aliases)
}

/// The occurrences of the alias's local name in its file whose definition
/// chain reaches the anchor's `declaration` ([`importers::binding`]), as
/// locations reading their own line.
async fn verified_uses(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    alias: &Alias,
    declaration: &HashSet<String>,
) -> Result<Vec<LspCodeSnippet>, LspFailure> {
    let Some(source) = sources.get(&alias.file).await else {
        return Ok(Vec::new());
    };
    let Ok(uri) = octocode_engine::lsp::uri::path_to_uri(&alias.file) else {
        return Ok(Vec::new());
    };
    let missing = super::super::import_stop::proven_missing(sources.policy());
    let width = u32::try_from(alias.local.encode_utf16().count()).unwrap_or(u32::MAX);
    let mut uses = Vec::new();
    for (line, character, _) in importers::occurrences(&source.content, &alias.local) {
        let Some(resolved) = importers::identities(
            client,
            sources,
            snippet_policy,
            cancel,
            &alias.file,
            line,
            character,
        )
        .await?
        else {
            continue;
        };
        if importers::binding(
            &resolved,
            &alias.file,
            declaration,
            &source.content,
            &missing,
        ) != Binding::Anchor
        {
            continue;
        }
        let at = |character| LspExactPosition { line, character };
        uses.push(LspCodeSnippet {
            uri: uri.clone(),
            range: LspRange {
                start: at(character),
                end: at(character.saturating_add(width)),
            },
            content: source
                .content
                .lines()
                .nth(line as usize)
                .unwrap_or_default()
                .to_owned(),
            symbol_kind: None,
            display_range: None,
        });
    }
    Ok(uses)
}

/// Whether `source` spells a rename of `symbol`: the word followed by `as`
/// (`import { f as g }`, `use a::f as g`) or by one `:` (`const { f: g }`).
/// Files without one cannot hold an aliasing import, so they are not parsed.
pub(in super::super) fn may_rename(source: &str, symbol: &str) -> bool {
    if symbol.is_empty() {
        return false;
    }
    let mut from = 0;
    while let Some(offset) = source[from..].find(symbol) {
        let start = from + offset;
        let end = start + symbol.len();
        from = end;
        if !octocode_engine::text::is_whole_word(source.as_bytes(), start, end) {
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

/// The renaming imports of `symbol` in `document` (a file's digest-cached
/// declarations parse) whose imported name starts at one of `sites` (the
/// answer's zero-based starts in that file): each local name with the
/// zero-based start of its binding.
pub(super) fn aliasing_imports(
    document: &octocode_engine::graph::GraphFactsDocument,
    symbol: &str,
    sites: &[(u32, u32)],
) -> Vec<(String, u32, u32)> {
    document
        .imports
        .iter()
        .filter_map(|import| {
            let local = import.local_name.as_deref()?;
            if import.imported_name.as_deref() != Some(symbol) || local == symbol {
                return None;
            }
            // A parse without the imported name's range names its line.
            let reported = match &import.imported_range {
                Some(range) => sites.contains(&(range.start.line, range.start.character)),
                None => sites
                    .iter()
                    .any(|(line, _)| line.saturating_add(1) == import.line),
            };
            let start = &import.local_range.as_ref()?.start;
            reported.then(|| (local.to_owned(), start.line, start.character))
        })
        .collect()
}

#[cfg(test)]
mod tests;
