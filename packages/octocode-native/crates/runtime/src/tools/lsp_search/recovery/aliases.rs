//! Alias-aware reference recovery: renamed imports found by a bounded scan,
//! and disclosing when that scan was capped.
use super::*;
use octocode_contract_types::LspSearchQuery;

/// The references request [`recover_aliases`] extends: the anchor's
/// `symbol`, whether declarations count, and the references the server
/// (`provider`) already reported.
pub(in super::super) struct AliasSearch<'a> {
    pub(in super::super) symbol: Option<&'a str>,
    pub(in super::super) include_declaration: bool,
    pub(in super::super) anchor: Position<'a>,
    pub(in super::super) provider: &'a [LspCodeSnippet],
}

/// References through aliasing imports (`import { x as y }`) the server did
/// not report: for each file that already references the symbol, find
/// imports that rename it, confirm by definition identity that the alias is
/// the same symbol, and add the alias's references. Bounded by
/// [`MAX_ALIAS_FILES_READ`] files read and [`MAX_ALIAS_IMPORTS`] verified
/// imports per request; a cap that leaves files or imports unchecked sets
/// [`AliasRecovery::capped`].
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
        anchor: Position {
            path,
            line,
            character,
        },
        provider,
    } = search;
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
        Err(failure) => {
            // Without the anchor's identity no alias can be verified: every
            // candidate file goes unchecked.
            let mut failures = RequestFailures::default();
            failures.record(failure)?;
            return Ok(AliasRecovery {
                failures,
                ..AliasRecovery::default()
            });
        }
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
    let mut failures = RequestFailures::default();
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
            let targets = match get_locations(
                client,
                snippet_policy,
                cancel,
                LocationRequest::Definition,
                &file,
                local_line,
                local_character,
            )
            .await
            {
                Ok(targets) => targets,
                Err(failure) => {
                    failures.record(failure)?;
                    continue;
                }
            };
            if targets.iter().map(snippet_identity).collect::<HashSet<_>>() != definition_ids {
                continue;
            }
            match get_locations(
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
                Ok(extra) => recovered.extend(
                    extra
                        .into_iter()
                        .filter(|snippet| !provider_ids.contains(&snippet_identity(snippet))),
                ),
                Err(failure) => failures.record(failure)?,
            }
        }
    }
    Ok(AliasRecovery {
        snippets: recovered,
        capped,
        failures,
    })
}

/// What alias recovery found, whether a cap left candidates unchecked, and
/// the server requests that failed (each one an alias left unchecked).
#[derive(Debug, Default)]
pub(in super::super) struct AliasRecovery {
    pub(in super::super) snippets: Vec<LspCodeSnippet>,
    pub(in super::super) capped: bool,
    pub(in super::super) failures: RequestFailures,
}

/// Partial reason of a references row whose alias scan stopped at a cap.
pub(in super::super) const ALIAS_SCAN_CAPPED_REASON: &str = "aliasScanCapped";

/// Disclose a capped alias scan: `coverage.aliasScan:"capped"`, a partial
/// reason with a warning, and the lexical `findText` lead that reaches the
/// aliases left unchecked.
pub(in super::super) fn disclose_alias_cap(
    row: &mut Value,
    query: &LspSearchQuery,
    scope: &super::super::scope::Scope,
) {
    if let Some(coverage) = row.pointer_mut("/coverage") {
        coverage["aliasScan"] = serde_json::json!("capped");
    }
    super::super::failure::flag_partial(
        row,
        query,
        ALIAS_SCAN_CAPPED_REASON,
        Some(&format!(
            "Alias recovery checked at most {MAX_ALIAS_FILES_READ} files and {MAX_ALIAS_IMPORTS} renaming imports; references through the unchecked aliases may be missing. This cap is terminal: no page or retry re-checks them. Confirm with {}.",
            super::super::failure::TEXT_CHECK
        )),
        scope,
    );
}

/// Whether `source` spells a rename of `symbol`: the word followed by `as`
/// (`import { f as g }`, `use a::f as g`) or by one `:` (`const { f: g }`).
/// Files without one cannot hold an aliasing import, so they are not parsed.
pub(in super::super) fn may_rename(source: &str, symbol: &str) -> bool {
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
pub(super) fn aliasing_imports(source: &str, file: &str, symbol: &str) -> Vec<(u32, u32)> {
    if !may_rename(source, symbol) {
        return Vec::new();
    }
    let Some(facts) = octocode_engine::signatures::extract_graph_facts(source, file) else {
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
