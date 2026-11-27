//! The view pipeline: one fetched source, one localFetch query, one result.
//! Local and remote reads use the same request-owned pipeline; source acquisition
//! stays with the caller.
#[cfg(test)]
use super::extraction::extract;
use super::types::*;
use super::{no_match::*, prepare::*};
use octocode_contract_types::{ErrorCode, LocalFetchQuery, ReadView};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::security::scan::ContentScan;

mod compact;
mod page;
mod select;
mod warnings;

use page::{cut_page, page_result};
use select::{Minified, Refused, select_view};

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
    process_fetched_content_as(
        q,
        bytes,
        source_path,
        &provider_path,
        facts,
        security,
        cancel,
    )
}

/// [`process_fetched_content`] of a source the provider names `provider_path`
/// (the path a redaction scan judges the text as).
pub fn process_fetched_content_as<S: ContentScan>(
    q: &LocalFetchQuery,
    bytes: &[u8],
    source_path: &std::path::Path,
    provider_path: &str,
    facts: &SourceFacts,
    security: &S,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    let sha256 = octocode_engine::digest::sha256(bytes);
    // A continuation page of these bytes replays the view its cursor's
    // first page built, and only cuts its own page from it.
    let retention =
        super::retained::Retention::of(q, &sha256, (source_path, provider_path), facts, security);
    if let Some(kept) = retention.as_ref().and_then(|kept| kept.replay(q)) {
        let page = Page {
            source: &kept.source,
            ext: &kept.ext,
            view: &kept.view,
        };
        return render(q, page, facts, source_path, cancel);
    }
    // A range or match read of bytes already scanned at this path reuses that
    // scan; a cancelled row still ends in `prepare_source`.
    let scan = super::retained::ScannedSource::of(
        q,
        &sha256,
        (source_path, provider_path),
        facts,
        security,
    );
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
        // A pattern or range read reuses the offsets its preparation built,
        // and keeps them: its byte pages are placed in the source by them.
        let offsets = source
            .offsets
            .get_or_insert_with(|| {
                std::sync::Arc::new(super::extraction::LineOffsets::new(&source.text))
            })
            .clone();
        super::extraction::extract_indexed(q, &source.text, &source.written(), &offsets)
    };
    let mut ext = match extracted {
        Ok(extraction) => extraction,
        Err(error) => return extraction_error(q, error, source.total_lines),
    };
    if ext.count == Some(0) {
        // A misremembered declaration head reads the file's one declaration
        // of its name, as that read of the declaration's own line.
        if let Some((declared, warning)) = declared_read(q, &source.text, &source.written()) {
            let mut read = process_fetched_content_as(
                &declared,
                bytes,
                source_path,
                provider_path,
                facts,
                security,
                cancel,
            );
            read.warnings.insert(0, warning);
            return read;
        }
        let mut missed = no_match(q, &source);
        if ext.match_limited {
            disclose_match_limit(&mut missed, ext);
        }
        return missed;
    }
    let view = match select_view(q, &mut ext, &source.text, cancel) {
        Ok(view) => view,
        Err(Refused::Minified(minified)) => {
            return minified_text(q, minified, source.total_lines);
        }
    };
    let page = Page {
        source: &source,
        ext: &ext,
        view: &view,
    };
    let result = render(q, page, facts, source_path, cancel);
    if let Some(retention) = retention {
        retention.keep(&result, source, ext, view);
    }
    result
}

/// The view a page is cut from.
struct Page<'a> {
    source: &'a Source<'a>,
    ext: &'a super::extraction::Extraction,
    view: &'a View,
}

/// Cut this page from its view and build its row.
fn render(
    q: &LocalFetchQuery,
    page: Page<'_>,
    facts: &SourceFacts,
    source_path: &std::path::Path,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    let Page { source, ext, view } = page;
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
    // The first cut takes the row's whole share; a row wider than its share
    // as published is cut again to the share less its measured skeleton,
    // until it fits or holds one line. Each cut is a complete page with its
    // own continuation, so a shorter one loses nothing.
    let mut fill = None;
    loop {
        let cut = match cut_page(q, view, ext, source, facts, fill) {
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
        let budget = cut.budget;
        let result = page_result(cut, source, ext, view, source_path, facts);
        match facts.window.and_then(|share| refit(&result, budget, share)) {
            Some(less) => fill = Some(less),
            None => return result,
        }
    }
}

/// The smaller fill a line page of several lines is cut to when its row, as
/// published, is wider than `share`: the share less the row's skeleton (every
/// published char but its text's), as every page budget reserves its row
/// ([`octocode_runtime_base::response::page_windows::page_chars`]: a share its skeleton
/// crowds still gives its lines half of itself, and the response pager
/// splits the row without loss). `None` when the row fits, or no smaller fill
/// sheds a line. Each refit takes fewer lines than the cut before it.
fn refit(
    result: &LocalFetchResult,
    budget: super::pagination::LineBudget,
    share: usize,
) -> Option<usize> {
    let text = result.content.as_deref()?;
    if result.status != ReadStatus::Success
        || result.pagination.as_ref()?.unit != octocode_contract_types::ReadProgressUnit::Lines
        || text.split_inclusive('\n').nth(1).is_none()
    {
        return None;
    }
    let row = serde_json::to_value(result).ok()?;
    let published = octocode_runtime_base::response::read_share::published_chars(&row);
    if published.row <= share {
        return None;
    }
    let counted: usize = text
        .split_inclusive('\n')
        .map(|line| budget.rendered(line))
        .sum();
    let fill = octocode_runtime_base::response::page_windows::page_chars(
        Some(share),
        published.row - published.text,
    );
    (fill < counted).then_some(fill)
}

/// The selected text in the view it is returned in (`mode` `None`: as is).
pub(super) struct View {
    pub(super) text: String,
    pub(super) mode: Option<ReadView>,
    pub(super) fallback: Option<ViewFallback>,
    /// The view's note for its first page (an outline's recovered parse).
    pub(super) note: Option<String>,
}

/// The row of a compact or symbols read of minified text: neither view is
/// made (an outline would list minifier names, a compact layout keeps
/// nearly every byte, and both cost a parse), and the read that answers is
/// a `matchString` selection, each match with its window.
fn minified_text(q: &LocalFetchQuery, minified: Minified, total_lines: usize) -> LocalFetchResult {
    let view = match local_fetch_query::view(q) {
        ReadView::Symbols => "symbols",
        _ => "compact",
    };
    let Minified { lines, average } = minified;
    let mut result = LocalFetchResult::error(
        local_fetch_query::path(q).to_owned(),
        ErrorCode::LargeSourceWindowOnly,
        format!(
            "Minified text: {lines} line(s) averaging {average} bytes, so no {view} view is made. Select what you need with matchString, which returns each match with its window."
        ),
    );
    result.total_lines = Some(total_lines);
    result
}

/// The one shape of a page asked past the end of its view, whatever reads
/// the source (whole or streamed): nothing returned and the restart at
/// offset 0. `pagination` (`outOfRange`, its offset and the view's total)
/// states the condition; no warning restates it.
pub fn past_end(q: &LocalFetchQuery) -> NextCalls {
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
/// runtime's one restart owner ([`octocode_runtime_base::response::rows::require_restart`])
/// adds the caller's whole query without its cursors, so a streamed range read restarts every range.
pub fn stale_snapshot(q: &LocalFetchQuery) -> LocalFetchResult {
    LocalFetchResult::error(
        local_fetch_query::path(q).to_owned(),
        ErrorCode::StaleSnapshot,
        octocode_runtime_base::response::result::STALE_SNAPSHOT_ERROR.into(),
    )
}

/// The refusal for a binary file: a NUL byte, or a binary head.
pub fn refuse_binary(q: &LocalFetchQuery, display_path: &str) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(
        local_fetch_query::path(q).to_owned(),
        ErrorCode::BinaryFileUnsupported,
        format!(
            "Binary file unsupported: {display_path}. Read a text source file, or use structureSearch operation:\"files\" for file metadata."
        ),
    );
    // No range or matchString can make binary bytes readable; say so
    // instead of the generic "remove matchString" recovery.
    result.hints =
        vec!["Do not retry this path with other ranges; choose a text source file instead.".into()];
    result
}

/// [`refuse_binary`] for a read the caller judged: when its head is a PNG or
/// JPEG that clasify can judge (`clasify_image`, decided by the caller, which
/// may know clasify's images), the refusal names the clasify `source` route
/// instead of telling the caller to pick a text file. A hint, not a ready
/// call: the questions are the caller's.
pub fn refuse_binary_with_head(
    q: &LocalFetchQuery,
    display_path: &str,
    clasify_image: bool,
) -> LocalFetchResult {
    let mut result = refuse_binary(q, display_path);
    if clasify_image {
        let path = serde_json::Value::String(local_fetch_query::path(q).to_owned());
        result.hints = vec![format!(
            "This is a PNG or JPEG image: judge it with clasify operation:\"evaluate\", source {{\"tool\":\"localFetch\",\"query\":{{\"path\":{path}}}}}, with your questions."
        )];
    }
    result
}

pub(crate) fn compress_ranges(lines: &[usize]) -> Vec<LineRange> {
    crate::line_spans::runs(lines.iter().copied())
        .into_iter()
        .map(|(start, end)| LineRange { start, end })
        .collect()
}

#[cfg(test)]
mod tests;

/// A scan the memory budget stopped before any match it held fell in the
/// selection: the empty row is partial, not a miss, and says why.
fn disclose_match_limit(row: &mut LocalFetchResult, ext: super::extraction::Extraction) {
    row.is_partial = Some(true);
    row.partial_reasons = vec![PartialReason::ScanBudget];
    row.warnings.extend(ext.warnings);
    if let Some(lead) = ext.next.text_search {
        row.next.get_or_insert_with(NextCalls::default).text_search = Some(lead);
    }
}
