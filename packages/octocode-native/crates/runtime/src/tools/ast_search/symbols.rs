use crate::response::result::ToolError;
use crate::{execution::cancel::CancellationCheck, policy::path::PathPolicy};
use octocode_contract_types::{AstSearchQuerySymbols, ErrorCode};
use serde_json::Value;
use std::sync::Arc;

mod continuation;
mod directory;
mod empty_outline;
mod left_out;
mod names;
mod outline;
mod page;
mod rows;
mod select;

use continuation::Continuation;
#[cfg(test)]
pub(super) use directory::outline_directory;
use directory::{mapped_files, outline_walked, walk_directory};
use empty_outline::note_skipped;
use outline::outline_file;
#[cfg(test)]
pub(super) use outline::{Outline, PARSED_FILES};
pub(super) use page::render_page;
#[cfg(test)]
pub(super) use select::SymbolSet;
use select::select;

/// Engine-unit views over the generated `symbols` query.
pub(crate) mod ast_search_query_symbols {
    use super::names::{NameEntry, NameFilter};
    use octocode_contract_types::AstSearchQuerySymbols;
    use octocode_engine::num::saturating_u32;
    use std::collections::BTreeMap;
    pub(crate) fn language(query: &AstSearchQuerySymbols) -> Option<String> {
        query.parser.as_ref().map(ToString::to_string)
    }
    pub(crate) fn language_globs(
        query: &AstSearchQuerySymbols,
    ) -> Option<BTreeMap<String, Vec<String>>> {
        query
            .parser_globs
            .as_ref()
            .map(crate::tools::ast_rule::parser_globs)
    }
    pub(crate) fn kinds(query: &AstSearchQuerySymbols) -> Option<Vec<String>> {
        (!query.symbol_kinds.is_empty())
            .then(|| query.symbol_kinds.iter().map(ToString::to_string).collect())
    }
    pub(crate) fn exclude(query: &AstSearchQuerySymbols) -> Option<Vec<String>> {
        crate::tools::variant_field::non_empty(&query.exclude)
    }
    /// Leading files (walk order) an earlier window outlined.
    pub(crate) fn scan_offset(query: &AstSearchQuerySymbols) -> u32 {
        query
            .scan_offset
            .map_or(0, |offset| saturating_u32(offset.get()))
    }
    pub(crate) fn page(query: &AstSearchQuerySymbols) -> u32 {
        saturating_u32(query.page.get())
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
        let scoped = crate::tools::test_code::Scoped::of(q.test_code, paths);
        let outline = match outline_file(q, (&p.canonical, &display_path), scoped.as_ref(), cancel)?
        {
            Ok(outline) => outline,
            Err(terminal) => return Ok(terminal),
        };
        let set = select(q, &p.canonical, outline, cancel)?;
        return render_page(q, &set, row_paths, response_window, cancel);
    }
    let row_paths = super::RowPaths::new(paths, &p.canonical, false);
    let mapped = mapped_files(q, &p.canonical)?;
    let scoped = crate::tools::test_code::Scoped::of(q.test_code, paths);
    let started = octocode_engine::stamp::settle_clock();
    let walk = walk_directory(q, &p.canonical, (paths, scoped.as_ref()), cancel)?;
    let continuation = Continuation::of(q, paths, &p.canonical, &walk, started);
    let stored = continuation.stored(q);
    let set = match &stored {
        Some(set) => Arc::clone(set),
        None => {
            let outline = outline_walked(q, &p.canonical, (walk, scoped.as_ref()), cancel)?;
            Arc::new(select(q, &p.canonical, outline, cancel)?)
        }
    };
    let mut page = render_page(q, &set, row_paths, response_window, cancel)?;
    if stored.is_none() && page["calls"]["nextPage"].is_object() {
        continuation.remember(&set);
    }
    note_skipped(q, &p.canonical, mapped.as_ref(), paths, cancel, &mut page);
    note_directory_parser(q, &mut page);
    Ok(page)
}

/// A directory reads each file by `parserGlobs`, else its extension:
/// `parser`, one file's grammar, does not apply, and the page says so.
fn note_directory_parser(q: &AstSearchQuerySymbols, page: &mut serde_json::Value) {
    if let Some(parser) = ast_search_query_symbols::language(q) {
        crate::response::paging::push_notice(
            page,
            format!(
                "parser \"{parser}\" applies to one file and was not applied: a directory reads each file by parserGlobs, else its extension."
            ),
        );
    }
}

/// `parser` names one file's grammar; `parserGlobs` maps a directory's, and
/// a directory with `parser` still reads each file by its extension
/// ([`note_directory_parser`]).
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
            octocode_engine::text::analysis::AnalysisOperation::Declarations,
        );
    }
    // A directory reads each file by `parserGlobs`, else its extension.
    Ok(())
}
