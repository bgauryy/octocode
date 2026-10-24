//! The view pipeline: one fetched source, one localFetch query, one result.
//! `process_fetched_content` performs no filesystem access, so local reads and
//! remote reads (ghGetFileContent) render through the same code.
use super::extraction::extract;
use super::types::*;
use super::{no_match::*, outline::*, prepare::*};
use crate::execution::cancel::CancellationCheck;
use crate::security::scan::ContentScan;
use octocode_contract_types::{ErrorCode, LocalFetchQuery, ReadView};

mod classified;
mod page;
mod warnings;

use classified::{SelectionInput, refused_selection_input, select_no_match, select_view_chunks};
use page::{cut_page, page_result};

pub(crate) use classified::unsupported_selection;

/// Shared post-acquisition content processing. Performs no filesystem access;
/// callers own source authorization, binary/transport limits and provenance.
pub fn process_fetched_content(
    q: &LocalFetchQuery,
    bytes: &[u8],
    source_path: &std::path::Path,
    facts: &SourceFacts,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    let provider_path = source_path.file_name().map_or_else(
        || ".".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    process_fetched_content_selected(
        q,
        bytes,
        source_path,
        &provider_path,
        facts,
        security,
        cancel,
        None,
    )
}

pub(crate) fn process_fetched_content_selected(
    q: &LocalFetchQuery,
    bytes: &[u8],
    source_path: &std::path::Path,
    provider_path: &str,
    facts: &SourceFacts,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
    selection: Option<&crate::tools::selection::Runtime>,
) -> LocalFetchResult {
    let sha256 = octocode_engine::digest::sha256(bytes);
    // Refused on its input alone: no decode, redaction or extraction runs;
    // the digest binds the unfiltered lead to the version the refusal saw.
    if let Some(refused) = refused_selection_input(q, &sha256) {
        return refused;
    }
    let mut source = match prepare_source(q, bytes, sha256, facts, source_path, security, cancel) {
        Ok(source) => source,
        Err(result) => return *result,
    };
    // A whole-text read moves the source into its view instead of copying
    // it: nothing after the extraction reads the source text of such a read.
    let extracted = if super::extraction::selects_whole(q) {
        Ok(super::extraction::Extraction::whole(
            std::sync::Arc::unwrap_or_clone(std::mem::take(&mut source.text)),
        ))
    } else {
        extract(q, &source.text)
    };
    let mut ext = match extracted {
        Ok(extraction) => extraction,
        Err(error) => return extraction_error(q, error, source.total_lines),
    };
    let selection_input = SelectionInput {
        q,
        source_path,
        provider_path,
        sha256: &source.sha256,
        runtime: selection,
        query_json: std::cell::OnceCell::new(),
    };
    if ext.count == Some(0) {
        return select_no_match(&selection_input, no_match(q, &source));
    }
    let mut view = select_view(
        q,
        &mut ext,
        &mut source.redacted_lines,
        source_path,
        security,
    );
    let selection_outcome = match select_view_chunks(
        &selection_input,
        &source.text,
        &mut view,
        &mut ext,
        security,
    ) {
        Ok(outcome) => outcome,
        Err(result) => return *result,
    };
    if let Err(e) = cancel.check() {
        return LocalFetchResult::error(
            local_fetch_query::path(q).to_owned(),
            ErrorCode::Cancelled,
            e,
        );
    }
    // The row's share of the response window, which the whole row (and
    // what the caller adds beside it) fits; a tiny window still keeps half
    // of itself, as page budgets do.
    let own = SourceFacts {
        window: facts
            .window
            .map(|window| window.saturating_sub(facts.beside).max(window / 2).max(1)),
        beside: 0,
        ..facts.clone()
    };
    let facts = &own;
    // A classified page's plan token, receipt and unfiltered leads ride the
    // same row: the text window leaves room for them.
    let classified_facts;
    let facts = match &selection_outcome {
        Some(chosen) if facts.window.is_some() => {
            let beside = chosen.row_chars(selection_input.query_json());
            classified_facts = SourceFacts {
                window: facts
                    .window
                    .map(|window| window.saturating_sub(beside).max(1)),
                ..facts.clone()
            };
            &classified_facts
        }
        _ => facts,
    };
    let cut = match cut_page(q, &view, &ext, &source, facts, source_path, security) {
        Ok(cut) => cut,
        Err(result) => return *result,
    };
    if let Err(e) = cancel.check() {
        return LocalFetchResult::error(
            local_fetch_query::path(&cut.q).to_owned(),
            ErrorCode::Cancelled,
            e,
        );
    }
    page_result(
        cut,
        &source,
        &ext,
        &view,
        selection_outcome.as_ref(),
        source_path,
        facts,
    )
}

/// The selected text in the view it is returned in (`mode` `None`: as is).
struct View {
    text: String,
    mode: Option<ReadView>,
    fallback: Option<ViewFallback>,
}

/// Apply the requested `view` to the extracted text. A match read
/// stays verbatim evidence; an outline the file type cannot give falls
/// back to the standard view.
fn select_view(
    q: &LocalFetchQuery,
    ext: &mut super::extraction::Extraction,
    source_redactions: &mut Vec<(usize, usize)>,
    source_path: &std::path::Path,
    security: &impl ContentScan,
) -> View {
    let requested = local_fetch_query::view(q);
    let selected = std::mem::take(&mut ext.text);
    // matchString reads are verbatim source; the tool description says so,
    // so the row carries no fallback notice. Byte windows address the
    // selected view (a numbered walk pages an oversized line by view bytes),
    // so they take no view of their own here.
    if local_fetch_query::has_matches(q) {
        return View {
            text: selected,
            mode: None,
            fallback: None,
        };
    }
    // Every transformed view keeps the citation gutter (`N\t`): the standard
    // view numbers each kept line with its source line, the outline uses the
    // same separator in every language.
    let standard_view = |selected: &str| {
        let view = octocode_engine::minify::apply::apply_content_view_minification(
            selected,
            local_fetch_query::path(q),
        );
        let lines = ext.source_lines.as_deref();
        // A compaction that joins lines would cite them wrong: keep lines whole.
        crate::response::numbered::number_view(selected, lines, &view)
            .unwrap_or_else(|| crate::response::numbered::line_faithful_view(selected, lines))
    };
    let view = |text, mode| View {
        text,
        mode,
        fallback: None,
    };
    match requested {
        ReadView::Content => view(selected, None),
        ReadView::Compact => view(standard_view(&selected), Some(ReadView::Compact)),
        ReadView::Symbols => {
            match symbols_view(
                crate::tools::declarations_cache::symbols_store(),
                &selected,
                local_fetch_query::path(q),
                source_path,
                security,
            )
            .as_ref()
            {
                crate::tools::declarations_cache::SymbolsView {
                    outline: Ok(outline),
                    warning,
                    redacted_lines,
                } => {
                    // An outline from a recovered parse says where it is blind,
                    // once: on the outline's first page.
                    if local_fetch_query::offset(q).unwrap_or(0) == 0 {
                        ext.warnings.extend(warning.clone());
                    }
                    // The outline shows the placeholders its render put in: the
                    // page counts them as source-wide redactions.
                    add_redacted_lines(source_redactions, redacted_lines);
                    view(outline.clone(), Some(ReadView::Symbols))
                }
                // `viewFallback` is the typed notice of the applied view.
                crate::tools::declarations_cache::SymbolsView {
                    outline: Err(reason),
                    ..
                } => View {
                    text: standard_view(&selected),
                    mode: Some(ReadView::Compact),
                    fallback: Some(ViewFallback {
                        requested: ReadView::Symbols,
                        applied: ReadView::Compact,
                        reason: reason.as_str().into(),
                    }),
                },
            }
        }
    }
}

/// The one shape of a page asked past the end of its view, whatever reads
/// the source (whole or streamed): nothing returned and the restart at
/// offset 0. `pagination` (`outOfRange`, its offset and the view's total)
/// states the condition; no warning restates it.
pub(crate) fn past_end(q: &LocalFetchQuery) -> NextCalls {
    let mut query = q.clone();
    local_fetch_query::set_progress(&mut query, None);
    NextCalls {
        restart: Some(FetchCall::related(query)),
        ..NextCalls::default()
    }
}

/// Same file and caller metadata, every extraction selector reset to one raw line.
pub(super) fn single_line_query(q: &LocalFetchQuery, line: usize) -> Option<LocalFetchQuery> {
    line_read(
        q,
        &[LineRange {
            start: line,
            end: line,
        }],
    )
}

/// The continued source changed since its page was cut. Pages of two file
/// versions must not be combined. The row names no restart of its own: the
/// runtime's one restart owner (`response::pages::restart_stale`, through
/// [`crate::response::result::restart_query`]) adds the caller's whole query
/// without its cursors, so a streamed range read restarts every range.
pub(crate) fn stale_snapshot(q: &LocalFetchQuery) -> LocalFetchResult {
    LocalFetchResult::error(
        local_fetch_query::path(q).to_owned(),
        ErrorCode::StaleSnapshot,
        crate::response::result::STALE_SNAPSHOT_ERROR.into(),
    )
}

pub(crate) fn compress_ranges(lines: &[usize]) -> Vec<LineRange> {
    crate::tools::line_spans::runs(lines.iter().copied())
        .into_iter()
        .map(|(start, end)| LineRange { start, end })
        .collect()
}

#[cfg(test)]
mod tests;
