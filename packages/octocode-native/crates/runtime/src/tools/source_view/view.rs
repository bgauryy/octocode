//! The view pipeline: one fetched source, one localFetch query, one result.
//! Local and remote reads use the same request-owned pipeline; source acquisition
//! stays with the caller.
#[cfg(test)]
use super::extraction::extract;
use super::types::*;
use super::{no_match::*, outline::*, prepare::*};
use crate::execution::cancel::CancellationCheck;
use crate::security::scan::ContentScan;
use octocode_contract_types::{ErrorCode, LocalFetchQuery, ReadView};

mod classified;
mod compact;
mod page;
mod warnings;

use classified::{SelectionInput, refused_selection_input, select_no_match, select_view_chunks};
use page::{cut_page, page_result};

pub(crate) use classified::unsupported_selection;

/// Shared post-acquisition content processing. Callers own source authorization,
/// binary/transport limits and provenance.
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

pub(crate) fn process_fetched_content_selected<S: ContentScan>(
    q: &LocalFetchQuery,
    bytes: &[u8],
    source_path: &std::path::Path,
    provider_path: &str,
    facts: &SourceFacts,
    security: &S,
    cancel: &impl CancellationCheck,
    selection: Option<&crate::tools::selection::Runtime>,
) -> LocalFetchResult {
    let sha256 = octocode_engine::digest::sha256(bytes);
    // Refused on its input alone: no decode, redaction or extraction runs;
    // the digest binds the unfiltered lead to the version the refusal saw.
    if let Some(refused) = refused_selection_input(q, &sha256) {
        return refused;
    }
    // A continuation page of these bytes replays the view its cursor's
    // first page built, and only cuts its own page from it.
    let retention =
        super::retained::Retention::of::<S>(q, &sha256, (source_path, provider_path), facts);
    if let Some(kept) = retention.as_ref().and_then(|kept| kept.replay(q)) {
        let page = Page {
            source: &kept.source,
            ext: &kept.ext,
            view: &kept.view,
            selection: None,
        };
        return render(q, page, facts, source_path, security, cancel);
    }
    // A range or match read of bytes already scanned at this path reuses that
    // scan; a cancelled row still ends in `prepare_source`.
    let scan =
        super::retained::ScannedSource::of::<S>(q, &sha256, (source_path, provider_path), facts);
    let scanned = scan.as_ref().and_then(|scan| scan.get(q));
    let mut source = match scanned.as_deref() {
        Some(scanned) if cancel.check().is_ok() => scanned.shared(),
        _ => match prepare_source(q, bytes, sha256, facts, source_path, security, cancel) {
            Ok(source) => source,
            Err(result) => return *result,
        },
    };
    if scanned.is_none()
        && let Some(scan) = scan
    {
        scan.keep(&source);
    }
    // A whole-text read moves the source into its view instead of copying
    // it: nothing after the extraction reads the source text of such a read.
    let extracted = if super::extraction::selects_whole(q) {
        Ok(super::extraction::Extraction::whole(
            std::mem::take(&mut source.text).into_owned(),
        ))
    } else {
        // A pattern or range read reuses the offsets its preparation built.
        let offsets = source.offsets.take().unwrap_or_else(|| {
            std::sync::Arc::new(super::extraction::LineOffsets::new(&source.text))
        });
        super::extraction::extract_indexed(q, &source.text, &offsets)
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
        &source.text,
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
    // A classified page's plan token, receipt and unfiltered leads ride the
    // same row: the text window leaves room for them.
    let selection = selection_outcome
        .as_ref()
        .map(|chosen| (chosen, chosen.row_chars(selection_input.query_json())));
    let page = Page {
        source: &source,
        ext: &ext,
        view: &view,
        selection,
    };
    let result = render(q, page, facts, source_path, security, cancel);
    if let Some(retention) = retention
        && selection_outcome.is_none()
    {
        retention.keep(&result, source, ext, view);
    }
    result
}

/// The view a page is cut from, and a classified page's choice with the
/// row characters it adds beside the text.
struct Page<'a> {
    source: &'a Source<'a>,
    ext: &'a super::extraction::Extraction,
    view: &'a View,
    selection: Option<(&'a super::selection::Chosen, usize)>,
}

/// Cut this page from its view and build its row.
fn render(
    q: &LocalFetchQuery,
    page: Page<'_>,
    facts: &SourceFacts,
    source_path: &std::path::Path,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    let Page {
        source,
        ext,
        view,
        selection,
    } = page;
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
    let classified_facts;
    let facts = match selection {
        Some((_, beside)) if facts.window.is_some() => {
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
    let cut = match cut_page(q, view, ext, source, facts, source_path, security) {
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
        source,
        ext,
        view,
        selection.map(|(chosen, _)| chosen),
        source_path,
        facts,
    )
}

/// The selected text in the view it is returned in (`mode` `None`: as is).
pub(super) struct View {
    pub(super) text: String,
    pub(super) mode: Option<ReadView>,
    pub(super) fallback: Option<ViewFallback>,
    /// The view's note for its first page (an outline's recovered parse).
    pub(super) note: Option<String>,
    /// Every text the view and its source hold passed the source-wide
    /// secret scan, so later pages may replay them (see `retained`).
    pub(super) sanitized: bool,
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
    full_source: &str,
) -> View {
    let requested = local_fetch_query::view(q);
    let selected = std::mem::take(&mut ext.text);
    // matchString reads are verbatim source; the tool description says so,
    // so the row carries no fallback notice. Byte windows address the
    // selected view (a numbered walk pages an oversized line by view bytes),
    // so they take no view of their own here.
    // Their preparation scanned the whole source (see `prepare_source`).
    if local_fetch_query::has_matches(q) {
        return View {
            text: selected,
            mode: None,
            fallback: None,
            note: None,
            sanitized: true,
        };
    }
    // A range read's source passed the same scan; a whole read's did not.
    let scanned = local_fetch_query::has_ranges(q);
    // Every transformed view keeps the citation gutter (`N\t`): the standard
    // view numbers each kept line with its source line, the outline uses the
    // same separator in every language.
    let standard_view = |selected: &str| {
        compact::render(
            full_source,
            selected,
            local_fetch_query::path(q),
            ext.source_lines.as_deref(),
        )
    };
    let view = |text, mode| View {
        text,
        mode,
        fallback: None,
        note: None,
        sanitized: scanned,
    };
    match requested {
        ReadView::Content => view(selected, None),
        ReadView::Compact => view(standard_view(&selected), Some(ReadView::Compact)),
        ReadView::Symbols => {
            match symbols_view(&selected, local_fetch_query::path(q), source_path, security) {
                super::outline::SymbolsView {
                    outline: Ok(outline),
                    warning,
                    redacted_lines,
                    sanitized,
                } => {
                    // The outline shows the placeholders its render put in: the
                    // page counts them as source-wide redactions.
                    add_redacted_lines(source_redactions, &redacted_lines);
                    View {
                        // An outline from a recovered parse says where it is
                        // blind, once: on the outline's first page.
                        note: warning,
                        sanitized: sanitized || scanned,
                        ..view(outline, Some(ReadView::Symbols))
                    }
                }
                // `viewFallback` is the typed notice of the applied view.
                super::outline::SymbolsView {
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
                    note: None,
                    sanitized: scanned,
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
