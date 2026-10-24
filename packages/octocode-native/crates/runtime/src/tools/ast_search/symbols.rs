use crate::cache::CacheClass;
use crate::id::ToolId;
use crate::response::result::ToolError;
use crate::{execution::cancel::CancellationCheck, policy::path::PathPolicy};
use octocode_contract_types::{AstSearchQuerySymbols, ErrorCode};
use serde_json::{Value, json};

mod directory;
mod empty_outline;
mod left_out;
mod memo;
mod names;
mod outline;
mod page;
mod rows;
mod select;

#[cfg(test)]
pub(super) use directory::outline_directory;
use directory::{mapped_files, outline_walked, walk_directory};
use empty_outline::note_skipped;
use memo::{SYMBOL_PAGES, directory_memo_key, with_settled};
#[cfg(test)]
pub(super) use outline::Outline;
use outline::outline_file;
pub(super) use page::render_page;
#[cfg(test)]
pub(super) use select::SymbolSet;
use select::select;

/// Engine-unit views over the generated `symbols` query.
pub(crate) mod ast_search_query_symbols {
    use super::names::{NameEntry, NameFilter};
    use crate::num::u32_of;
    use octocode_contract_types::AstSearchQuerySymbols;
    use std::collections::BTreeMap;
    pub(crate) fn language(query: &AstSearchQuerySymbols) -> Option<String> {
        query.parser.as_ref().map(ToString::to_string)
    }
    pub(crate) fn language_globs(
        query: &AstSearchQuerySymbols,
    ) -> Option<BTreeMap<String, Vec<String>>> {
        query.parser_globs.as_ref().map(super::super::parser_globs)
    }
    pub(crate) fn kinds(query: &AstSearchQuerySymbols) -> Option<Vec<String>> {
        (!query.symbol_kinds.is_empty())
            .then(|| query.symbol_kinds.iter().map(ToString::to_string).collect())
    }
    pub(crate) fn exclude(query: &AstSearchQuerySymbols) -> Option<Vec<String>> {
        (!query.exclude.is_empty()).then(|| query.exclude.clone())
    }
    /// The caller's file scan cap; unset scans the scope within the scan
    /// time budget.
    pub(crate) fn max_files(query: &AstSearchQuerySymbols) -> Option<u32> {
        query.max_files.map(|value| u32_of(value.get()))
    }
    /// Leading files (walk order) an earlier window outlined.
    pub(crate) fn scan_offset(query: &AstSearchQuerySymbols) -> u32 {
        query.scan_offset.map_or(0, |offset| u32_of(offset.get()))
    }
    pub(crate) fn page(query: &AstSearchQuerySymbols) -> u32 {
        u32_of(query.page.get())
    }
    pub(crate) fn snapshot(query: &AstSearchQuerySymbols) -> Option<&str> {
        query.snapshot.as_deref().map(String::as_str)
    }
    /// The `name` filter: a string is a one-entry list. Each entry that
    /// equals some declaration name matches only equal names; otherwise it
    /// matches as a substring. All-lowercase text ignores case.
    pub(super) fn name_filter(query: &AstSearchQuerySymbols) -> Option<NameFilter> {
        let filter = query.symbol_filter.as_ref()?;
        Some(NameFilter {
            entries: filter
                .terms
                .iter()
                .map(|term| NameEntry::new(term.to_string(), filter.case_mode, filter.mode))
                .collect(),
            mode: filter.mode,
        })
    }
}

pub fn execute_symbols(
    q: &AstSearchQuerySymbols,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    response_window: Option<usize>,
) -> Result<Value, ToolError> {
    run_symbols(q, paths, cancel, response_window).map(super::output::finish)
}

fn run_symbols(
    q: &AstSearchQuerySymbols,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    response_window: Option<usize>,
) -> Result<Value, ToolError> {
    cancel.check().map_err(ToolError::cancelled)?;
    if let Some(unknown) = q
        .symbol_kinds
        .iter()
        .find(|kind| !octocode_engine::signatures::DECLARATION_KINDS.contains(&kind.as_str()))
    {
        return Err(ToolError::new(
            ErrorCode::InvalidInput,
            format!(
                "Unknown declaration kind \"{unknown}\". Use one of: {}.",
                octocode_engine::signatures::DECLARATION_KINDS.join(", ")
            ),
        ));
    }
    let p = paths
        .validate(q.path.as_str())
        .map_err(super::errors::policy_error)?;
    let meta = std::fs::metadata(&p.canonical)
        .map_err(|error| super::errors::io_error(q.path.as_str(), error))?;
    check_language_scope(q, &p.canonical, meta.is_file())?;
    if meta.is_file() {
        let row_paths = super::RowPaths::new(paths, &p.canonical, true);
        let display_path = row_paths.delivered(&p.canonical.to_string_lossy());
        let outline = match outline_file(q, &p.canonical, &display_path)? {
            Ok(outline) => outline,
            Err(terminal) => return Ok(terminal),
        };
        let set = select(q, &p.canonical, outline, cancel)?;
        return Ok(render_page(q, &set, row_paths, response_window));
    }
    let row_paths = super::RowPaths::new(paths, &p.canonical, false);
    // Later pages of a directory outline reuse page 1's selection while no
    // scanned file changed (same paths, sizes and modification times). A
    // repeated page 1 reuses only a selection whose files were all settled
    // (last written over two seconds before) when it was stored and still
    // are, so a write in the clock tick of a scan cannot hide behind an
    // unchanged time.
    let mapped = mapped_files(q, &p.canonical)?;
    // One walk serves the memo fingerprint and, on a miss, the outline scan.
    let walk = walk_directory(q, &p.canonical, paths, cancel)?;
    let (memo_key, settled) = directory_memo_key(q, &p.canonical, &walk);
    let other_key = || with_settled(memo_key.clone(), !settled);
    let hit = if ast_search_query_symbols::page(q) > 1 {
        SYMBOL_PAGES
            .get(&memo_key)
            .or_else(|| SYMBOL_PAGES.get(&other_key()))
    } else if settled {
        SYMBOL_PAGES.get(&memo_key)
    } else {
        None
    };
    if let Some(hit) = hit {
        let mut page = render_page(q, &hit.value, row_paths, response_window);
        note_skipped(q, &p.canonical, mapped.as_ref(), paths, cancel, &mut page);
        return Ok(page);
    }
    let outline = outline_walked(q, &p.canonical, walk, cancel)?;
    let set = select(q, &p.canonical, outline, cancel)?;
    let mut page = render_page(q, &set, row_paths, response_window);
    note_skipped(q, &p.canonical, mapped.as_ref(), paths, cancel, &mut page);
    let bytes = octocode_engine::json_size::bytes(&set).unwrap_or(usize::MAX);
    SYMBOL_PAGES.put(
        memo_key,
        set,
        bytes,
        CacheClass::Volatile {
            ttl: std::time::Duration::from_secs(600),
        },
    );
    Ok(page)
}

/// `parser` names one file's grammar; `parserGlobs` maps a directory's.
fn check_language_scope(
    q: &AstSearchQuerySymbols,
    canonical: &std::path::Path,
    is_file: bool,
) -> Result<(), ToolError> {
    if is_file && ast_search_query_symbols::language_globs(q).is_some() {
        return Err(ToolError::new(
            ErrorCode::LanguageDirectoryRequired,
            "parserGlobs is for directory symbols. Use parser for a single file.",
        ));
    }
    if is_file {
        return super::language::validate_file_language(
            canonical,
            ast_search_query_symbols::language(q).as_deref(),
        );
    }
    if q.parser.is_none() {
        return Ok(());
    }
    let mut error = ToolError::new(
        ErrorCode::LanguageFileRequired,
        "language on symbols requires a single source file.",
    );
    // Directory symbols pick each file's grammar from its extension, so
    // the same query without language is the exact repair.
    if let Ok(mut repaired) = serde_json::to_value(q) {
        if let Some(object) = repaired.as_object_mut() {
            object.remove("parser");
        }
        error.calls = Some(Box::new(json!({
            "repair": crate::response::result::restart_query(ToolId::AstSearch, &repaired)
        })));
    }
    Err(error)
}
