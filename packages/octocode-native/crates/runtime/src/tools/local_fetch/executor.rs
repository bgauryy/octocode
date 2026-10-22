use super::extraction::extract;
use super::pagination::{SourceLines, continuation, result_counts};
use super::types::*;
use super::{acquire::*, no_match::*, outline::*, page_scan::*, prepare::*};
use crate::execution::cancel::CancellationCheck;
use crate::id::ToolId;
use crate::security::scan::ContentScan;

/// One localFetch row; `selection` judges it under `clasifyContext`.
pub fn execute_local_fetch(
    q: &LocalFetchQuery,
    paths: &impl PathAccess,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
    window: Option<usize>,
    selection: Option<&crate::tools::selection::Runtime>,
) -> LocalFetchResult {
    if let Err(e) = cancel.check() {
        return LocalFetchResult::error(q.path.to_string(), "cancelled", e);
    }
    let (validated, file, meta) = match resolve_file(q, paths) {
        Ok(resolved) => resolved,
        Err(result) => return *result,
    };
    let path = &validated.canonical;
    let display_path = &validated.display;
    // Past the whole-file ceiling, only a streamed line window is served
    // (see `large_source`); past the streaming ceiling nothing is read.
    if meta.len() > super::large_source::MAX_STREAM_SOURCE_BYTES {
        return source_too_large(&q.path, meta.len(), SourceSizeLimit::Streaming);
    }
    let in_memory = meta.len() <= MAX_SOURCE_BYTES;
    let (file, sample) = match read_sample(file, meta.len(), in_memory) {
        Ok(opened) => opened,
        Err(error) => {
            return LocalFetchResult::error(
                q.path.to_string(),
                "fileReadFailed",
                error.to_string(),
            );
        }
    };
    if octocode_engine::text::sniff(&sample) == octocode_engine::text::BinaryVerdict::Binary {
        return refuse_binary(q, display_path);
    }
    if !in_memory && octocode_engine::text::has_utf16_bom(&sample) {
        let mut result = LocalFetchResult::error(
            q.path.to_string(),
            "fileTooLarge",
            format!(
                "BOM-marked UTF-16 sources require a bounded whole-file decode (limit: {MAX_SOURCE_BYTES} bytes); this source is {} bytes.",
                meta.len()
            ),
        );
        result.source_bytes = Some(meta.len() as usize);
        result.terminal_limit(PartialReason::FullContentSource);
        result.hints = vec!["Convert a copy to UTF-8 or split the source into smaller text files; line ranges do not lift this decoding limit.".into()];
        return result;
    }
    if !in_memory {
        return fetch_streamed(q, &validated, &file, &meta, window, security, cancel);
    }
    // The read stops past the cap, in case the file grew past the ceiling
    // between the stat above and this read (TOCTOU).
    let bytes = match read_rest(file, sample, MAX_SOURCE_BYTES, cancel) {
        Ok(bytes) => bytes,
        Err(octocode_engine::text::ReadError::TooLarge(len)) => {
            return source_too_large(&q.path, len, SourceSizeLimit::InMemoryGrowth);
        }
        Err(octocode_engine::text::ReadError::Io(error)) => return read_failure(&q.path, error),
        Err(error @ octocode_engine::text::ReadError::NotRegular) => {
            return LocalFetchResult::error(
                q.path.to_string(),
                "fileReadFailed",
                error.to_string(),
            );
        }
    };
    if octocode_engine::text::sniff_complete(&bytes) == octocode_engine::text::BinaryVerdict::Binary
    {
        return refuse_binary(q, display_path);
    }
    let mut result = process_fetched_content_selected(
        q,
        &bytes,
        path,
        display_path,
        &SourceFacts {
            modified: meta.modified().ok().and_then(system_time_iso),
            window,
            beside: 0,
            interior: false,
            byte_origin: None,
        },
        security,
        cancel,
        selection,
    );
    bind_continuation_to_source(&mut result);
    name_read_file(&mut result, q, &validated);
    result
}

/// The row for a selection that could not order the source.
fn selection_failure(
    q: &LocalFetchQuery,
    failure: crate::tools::selection::Failure,
) -> LocalFetchResult {
    match failure {
        crate::tools::selection::Failure::Stale => stale_snapshot(q),
        crate::tools::selection::Failure::Cancelled => LocalFetchResult::error(
            q.path.to_string(),
            "cancelled",
            "Selection was cancelled".into(),
        ),
        crate::tools::selection::Failure::Provider(failure) => {
            let crate::tools::selection::ProviderFailure { error, outcome } = *failure;
            let mut result =
                LocalFetchResult::error(q.path.to_string(), &error.code, error.message);
            result.hints = error.hints;
            result.selection = Some(outcome.receipt);
            result.next = Some(NextCalls {
                unfiltered: unfiltered_read(q),
                ..NextCalls::default()
            });
            result
        }
    }
}

/// Bind pages and recovery of unread or unfiltered evidence to the captured
/// file version, so they cannot silently read a different source.
/// (Remote reads pin a commit SHA in their continuation instead.)
fn bind_continuation_to_source(result: &mut LocalFetchResult) {
    let snapshot = result
        .source_sha256
        .as_deref()
        .and_then(|digest| digest.parse().ok());
    if let Some(next) = result.next.as_mut() {
        for read in next.reads_of_mut(ReadSource::Page) {
            read.query.snapshot = snapshot.clone();
        }
        if let Some(digest) = result.source_sha256.as_deref() {
            for rows in next
                .unfiltered_unknown_rest
                .values_mut()
                .filter_map(|call| call.pointer_mut("/query/queries"))
                .filter_map(serde_json::Value::as_array_mut)
            {
                for row in rows {
                    row["snapshot"] = serde_json::json!(digest);
                }
            }
        }
    }
}

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
        return LocalFetchResult::error(q.path.to_string(), "cancelled", e);
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
    let share = facts.window;
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
        return LocalFetchResult::error(cut.q.path.to_string(), "cancelled", e);
    }
    let paged = |cut| {
        page_result(
            cut,
            &source,
            &ext,
            &view,
            selection_outcome.as_ref(),
            source_path,
            facts,
        )
    };
    let page = paged(cut);
    // A page over the row's share is cut again from the view already read:
    // a shorter line page, whose leads page on at the caller's size.
    let Some(lines) = share.and_then(|share| fitted_lines(&page, q, share)) else {
        return page;
    };
    let shorter = shorter_page(q, lines);
    let cut = match cut_page(&shorter, &view, &ext, &source, facts, source_path, security) {
        Ok(cut) => cut,
        Err(_) => return page,
    };
    let mut fitted = paged(cut);
    if fitted.status != ReadStatus::Success {
        return page;
    }
    keep_asked_page(&mut fitted, q);
    fitted
}

/// The lines of `page` that fit `share` response chars, when the page is a
/// line page over it ([`crate::response::read_share::fitted_lines`]).
pub(super) fn fitted_lines(
    page: &LocalFetchResult,
    q: &LocalFetchQuery,
    share: usize,
) -> Option<usize> {
    if page.status != ReadStatus::Success {
        return None;
    }
    let row = serde_json::to_value(page).ok()?;
    crate::response::read_share::fitted_lines(&row, &crate::response::result::query_value(q), share)
}

/// `q` as a line page of `lines` from where it starts: a whole-file read
/// becomes the same view's first line page.
fn shorter_page(q: &LocalFetchQuery, lines: usize) -> LocalFetchQuery {
    let mut query = q.clone();
    if query.full_content == Some(true) {
        query.full_content = None;
        query.offset = Some(0);
    }
    query.unit = Some(WindowUnit::Lines);
    query.length = wire_positive(lines);
    query
}

/// A page shortened to its share pages on at the size `asked` named
/// ([`page_on_as_asked`]); a whole-file read cut to its share is partial for
/// the response share.
fn keep_asked_page(page: &mut LocalFetchResult, asked: &LocalFetchQuery) {
    if let Some(next) = page.next.as_mut() {
        page_on_as_asked(next, asked);
    }
    if asked.full_content == Some(true)
        && !page.partial_reasons.contains(&PartialReason::ResponseShare)
    {
        page.partial_reasons.push(PartialReason::ResponseShare);
    }
}

/// Reads that page on from a page cut for the response share keep the
/// size `asked` named, not the share: a line read takes `asked`'s `length`
/// (and, when it is no window of the read, its `unit` and `fullContent`).
fn page_on_as_asked(next: &mut NextCalls, asked: &LocalFetchQuery) {
    for read in next.reads_mut() {
        let query = &mut read.query;
        if query.unit == Some(WindowUnit::Bytes) {
            continue;
        }
        query.length = asked.length;
        if query.offset.is_none() && !local_fetch_query::has_ranges(query) {
            query.unit = asked.unit;
            query.full_content = asked.full_content;
        }
        if query.full_content.is_some() {
            query.length = None;
        }
    }
}

/// Source and runtime needed to select evidence before paging.
struct SelectionInput<'a> {
    q: &'a LocalFetchQuery,
    source_path: &'a std::path::Path,
    provider_path: &'a str,
    sha256: &'a str,
    runtime: Option<&'a crate::tools::selection::Runtime>,
    /// The query as the selection keys and prices it, serialized once.
    query_json: std::cell::OnceCell<serde_json::Value>,
}

impl SelectionInput<'_> {
    fn query_json(&self) -> &serde_json::Value {
        self.query_json
            .get_or_init(|| crate::response::result::query_value(self.q))
    }

    /// The selection's source key: the path and the digest of its bytes.
    fn source_key(&self) -> String {
        format!(
            "fetch-chunks-v2:{}:{}",
            self.source_path.display(),
            self.sha256
        )
    }
}

/// A read that selected nothing carries the selection receipt of its goals; a
/// ranked replay without goals is stale.
fn select_no_match(input: &SelectionInput, mut result: LocalFetchResult) -> LocalFetchResult {
    let q = input.q;
    if q.clasify_context.is_none() {
        return if q.selection.is_some() {
            stale_snapshot(q)
        } else {
            result
        };
    }
    match crate::tools::selection::select(
        input.runtime,
        input.query_json(),
        &input.source_key(),
        q.selection.as_ref().map(|token| token.as_str()),
        &[],
        None,
        octocode_contract_types::SelectionUnit::Chunks,
    ) {
        Ok(outcome) => result.selection = Some(outcome.receipt),
        Err(failure) => return selection_failure(q, failure),
    }
    result
}

/// A classified read its input alone rules out, refused before the source
/// is decoded, redacted or extracted: selection judges raw text in line
/// units, and pages only through its own selection token.
fn refused_selection_input(q: &LocalFetchQuery, digest: &str) -> Option<LocalFetchResult> {
    q.clasify_context.as_ref()?;
    // A first call with an offset is a request, not a stale cursor.
    if local_fetch_query::offset(q).is_some_and(|offset| offset > 0) && q.selection.is_none() {
        return Some(unsupported_selection(
            q,
            "A classified read pages only through its continueWindow selection token; drop offset or clasifyContext.",
            Some(digest),
        ));
    }
    // A match read is verbatim whatever its minify (see `select_view`).
    let transformed = local_fetch_query::minify_mode(q).is_some() && q.match_string.is_none();
    if transformed || (q.unit == Some(WindowUnit::Bytes) && q.selection.is_none()) {
        return Some(unsupported_selection(
            q,
            "clasifyContext requires raw text in line units; omit minify, and omit unit or use unit:lines.",
            Some(digest),
        ));
    }
    None
}

/// Select complete source windows before response paging.
fn select_view_chunks(
    input: &SelectionInput,
    source_text: &str,
    view: &mut View,
    ext: &mut super::extraction::Extraction,
    security: &impl ContentScan,
) -> Result<Option<super::selection::Chosen>, Box<LocalFetchResult>> {
    let q = input.q;
    if q.clasify_context.is_none() {
        if q.selection.is_some() {
            return Err(Box::new(stale_snapshot(q)));
        }
        return Ok(None);
    }
    let unsupported = None;
    let (candidates, chunks) = super::selection::chunks(
        &view.text,
        ext.source_lines.as_deref(),
        Some(source_text),
        input.source_path,
        input.provider_path,
        security,
        input.runtime,
        q.selection.as_ref().map(|token| token.as_str()),
        unsupported,
    );
    let (outcome, provider) =
        crate::tools::selection::unless_transient(crate::tools::selection::select(
            input.runtime,
            input.query_json(),
            &input.source_key(),
            q.selection.as_ref().map(|s| s.as_str()),
            &candidates,
            unsupported,
            octocode_contract_types::SelectionUnit::Chunks,
        ))
        .map_err(|failure| Box::new(selection_failure(q, failure)))?;
    let shown = outcome.shown(chunks.len());
    let (text, source_lines) = super::selection::project(&view.text, &chunks, &shown);
    view.text = text;
    ext.source_lines = Some(source_lines);
    Ok(Some(super::selection::Chosen::new(
        outcome, provider, &chunks,
    )))
}

/// The scanned page of a view, and the query it was cut for.
struct CutPage<'a> {
    /// The read's query; bounded when a complete view was over the window.
    q: std::borrow::Cow<'a, LocalFetchQuery>,
    /// The read as asked: classified routes (the whole view, the next
    /// candidate page) repeat it, not its bounded first page.
    asked: &'a LocalFetchQuery,
    view_limited: bool,
    /// The page is the head of an unanchored read.
    head: bool,
    scanned: ScannedPage,
}

/// Cut the returned page from the view. A complete view over the limit
/// returns its first bounded page inline.
fn cut_page<'a>(
    q: &'a LocalFetchQuery,
    view: &View,
    ext: &super::extraction::Extraction,
    source: &Source,
    facts: &SourceFacts,
    source_path: &std::path::Path,
    security: &impl ContentScan,
) -> Result<CutPage<'a>, Box<LocalFetchResult>> {
    let asked = q;
    let view_limited =
        q.full_content == Some(true) && facts.window.is_some_and(|window| view.text.len() > window);
    let q = if view_limited {
        std::borrow::Cow::Owned(bounded_query(q))
    } else {
        std::borrow::Cow::Borrowed(q)
    };
    // A configured response window owns the page budget. Keep the smaller
    // fallback for direct callers without one and any explicit preview.
    let line_page_bytes = facts
        .window
        .map_or(super::pagination::LINE_PAGE_BYTES, |window| {
            crate::response::page_windows::page_budget(
                Some(window),
                &crate::response::result::query_row(q.as_ref(), &[]),
            )
        });
    let head_view = head_query(&q, view.mode, view.text.len());
    let head = head_view.is_some();
    let page_q = head_view.as_ref().unwrap_or(&q);
    // A byte page of a streamed window is cut at its offset in the window.
    let in_window = facts.byte_origin.map(|origin| origin.within(page_q));
    let page_q = in_window.as_ref().unwrap_or(page_q);
    // A transformed view pages in the lines a raw read of the same
    // selection pages in: raw source lines for a whole file (the unit of
    // `totalLines`, the gutter and `ranges`), the extracted lines for
    // `ranges`; an outline pages its own lines.
    let source_lines = (view.mode == Some(MinifyMode::Standard))
        .then(|| match ext.source_lines.as_deref() {
            None => SourceLines::of_gutter(&view.text, source.total_lines),
            Some(selected) => SourceLines::of_selected_gutter(&view.text, selected),
        })
        .flatten();
    // A line page takes up to the row's whole share; the reader then cuts it
    // to fit exactly ([`fitted_lines`]). A line longer than the byte budget
    // reads in byte pages.
    let budget = super::pagination::LineBudget {
        fill: facts
            .window
            .map_or(line_page_bytes, |share| share.max(line_page_bytes)),
        line: line_page_bytes,
    };
    let window = PageWindow {
        query: page_q,
        view: &view.text,
        budget,
        source_lines: source_lines.as_ref(),
    };
    let mut scanned = scan_page(&q, &window, source_path, security, source, ext.start)?;
    if let Some(origin) = facts.byte_origin
        && scanned.page.pagination.unit == WindowUnit::Bytes
    {
        origin.place(&mut scanned.page.pagination);
    }
    Ok(CutPage {
        q,
        asked,
        view_limited,
        head,
        scanned,
    })
}

/// The success row for a scanned page: its warnings, continuations and
/// source anchors.
fn page_result(
    cut: CutPage,
    source: &Source,
    ext: &super::extraction::Extraction,
    view: &View,
    selection_outcome: Option<&super::selection::Chosen>,
    source_path: &std::path::Path,
    facts: &SourceFacts,
) -> LocalFetchResult {
    let CutPage {
        q,
        asked,
        view_limited,
        head,
        scanned,
    } = cut;
    let q = q.as_ref();
    let mut warnings = source.warnings.clone();
    warnings.extend(ext.warnings.iter().cloned());
    warnings.extend(scanned.warnings);
    let pg = scanned.page;
    let source_redactions = if source.redacted_lines.is_empty() {
        0
    } else {
        source.redactions_on(page_lines(&pg, ext.source_lines.as_deref(), view.mode))
    };
    // The page shows a key block the source-wide pass redacted.
    if source.key_blocks_redacted
        && source_redactions > 0
        && pg
            .text
            .contains(&crate::security::key_fragment_placeholder())
    {
        warnings.push(KEY_BLOCK_NOTE.into());
    }
    warn_redacted(
        &pg.text,
        scanned.redacted,
        scanned.redactions + source_redactions,
        &mut warnings,
    );
    if pg.pagination.offset == 0 {
        warn_unmatchable(q, source, ext, &mut warnings);
        warnings.extend(selection_outcome.and_then(super::selection::Chosen::disclosure));
    }
    let out_of_range = pg.out_of_range;
    let token = selection_outcome.map(|chosen| chosen.outcome.token.as_str());
    let mut next = continue_calls(q, &pg, head, token, ext, &mut warnings);
    // A whole view over the share was cut as its first line page.
    if view_limited && let Some(next) = next.as_mut() {
        page_on_as_asked(next, asked);
    }
    if let Some(chosen) = selection_outcome
        && chosen.outcome.leaves_out()
    {
        attach_unfiltered(next.get_or_insert_with(NextCalls::default), asked, chosen);
    }
    read_scope_warnings(q, &pg, &mut warnings);
    // Redaction (source-wide for matchString, or on the page) replaces text
    // within lines; the anchors stay and the warning above says the text is
    // not verbatim.
    let source_ranges =
        if !out_of_range && !scanned.empty && view.mode.is_none() && scanned.lines_kept {
            page_source_ranges(ext.source_lines.as_deref(), pg.view_lines)
        } else {
            vec![]
        };
    let blocks = if out_of_range {
        vec![]
    } else {
        ext.blocks.clone()
    };
    let mut matched_lines = ext.matched_lines.clone();
    matched_lines.retain(|n| source_ranges.iter().any(|r| *n >= r.start && *n <= r.end));
    let match_keys =
        crate::tools::match_keys::line_key_paths(source_path, &source.text, &matched_lines);
    let (chars, ret_bytes, ret_lines) = result_counts(&pg.text);
    LocalFetchResult {
        path: q.path.to_string(),
        source_sha256: Some(source.sha256.clone()),
        content: Some(pg.text),
        content_view: view.mode,
        minify_fallback: view.fallback.clone(),
        warnings,
        total_lines: Some(source.total_lines),
        start_line: ext.start,
        end_line: ext.end,
        source_line_ranges: source_ranges,
        match_ranges: ext.match_ranges.clone(),
        matched_lines,
        match_keys,
        blocks,
        selected_match_count: ext.count,
        modified: (view.mode != Some(MinifyMode::Symbols))
            .then(|| facts.modified.clone())
            .flatten(),
        source_chars: Some(source.chars),
        source_bytes: Some(source.bytes),
        returned_chars: Some(chars),
        returned_bytes: Some(ret_bytes),
        returned_lines: Some(ret_lines),
        pagination: Some(pg.pagination),
        // A capped regex scan selected a prefix of the matches.
        is_partial: ((next.as_ref().is_some_and(NextCalls::leaves_more) && !out_of_range)
            || ext.match_limited)
            .then_some(true),
        partial_reasons: [
            (view_limited, PartialReason::ResponseShare),
            (ext.match_limited, PartialReason::RegexMatchLimit),
        ]
        .into_iter()
        .filter_map(|(applies, reason)| applies.then_some(reason))
        .collect(),
        out_of_range,
        next,
        selection: selection_outcome.map(|chosen| chosen.outcome.receipt.clone()),
        ..LocalFetchResult::blank(ReadStatus::Success)
    }
}

/// A classified page's routes to what it left out: `unfiltered` reads the
/// whole unfiltered view and, when the unknown chunks are narrower,
/// `unfilteredUnknown` reads exactly them (`unfilteredUnknown2`, … the rest
/// of their ranges, as many rows per call as a call takes).
/// `nextCandidatePage` judges what the
/// candidate budget left unjudged.
fn attach_unfiltered(
    calls: &mut NextCalls,
    q: &LocalFetchQuery,
    chosen: &super::selection::Chosen,
) {
    let query = crate::response::result::query_value(q);
    let mut reads = chosen.unknown_reads(&query).into_iter();
    let first = reads
        .next()
        .and_then(|row| serde_json::from_value(row).ok())
        .map(|query| FetchCall { query });
    calls.unfiltered = unfiltered_read(q);
    calls.unfiltered_unknown = first;
    let rest = reads.collect::<Vec<_>>();
    for (index, batch) in rest
        .chunks(crate::response::result::rows_per_call(ToolId::LocalFetch))
        .enumerate()
    {
        calls.unfiltered_unknown_rest.insert(
            format!("unfilteredUnknown{}", index + 2),
            crate::response::result::Continuation::input(
                ToolId::LocalFetch,
                serde_json::json!({ "queries": batch }),
            )
            .build(),
        );
    }
    if let Some(token) = chosen.next_candidates() {
        let mut page = q.clone();
        page.offset = None;
        page.selection = token.parse().ok();
        calls.next_candidate_page = Some(FetchCall { query: page });
    }
}

/// The classified read as an ordinary read ([`crate::tools::selection::unfiltered_row`]).
fn unfiltered_read(q: &LocalFetchQuery) -> Option<FetchCall> {
    let row = crate::tools::selection::unfiltered_row(&serde_json::to_value(q).ok()?, &[]);
    serde_json::from_value(row)
        .ok()
        .map(|query| FetchCall { query })
}

/// A classified read refused before any selection ran: its unfiltered lead
/// is the caller's own read without `clasifyContext`, window and view kept
/// (`unit`, `offset`, `length`, `ranges`, `minify`), so dropping the
/// classifier is the only change. `digest` binds the lead to the source
/// version the refusal saw.
pub(super) fn unsupported_selection(
    q: &LocalFetchQuery,
    message: &str,
    digest: Option<&str>,
) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(q.path.to_string(), "invalidInput", message.into());
    result.source_sha256 = digest.map(str::to_owned);
    let mut query = q.clone();
    query.clasify_context = None;
    query.selection = None;
    result.next = Some(NextCalls {
        unfiltered: Some(FetchCall { query }),
        ..NextCalls::default()
    });
    result
}

/// The page's continuations: its next page bound to the selection token, plus
/// follow-ups to data the extraction selected but did not return.
fn continue_calls(
    q: &LocalFetchQuery,
    pg: &super::pagination::Page,
    head: bool,
    token: Option<&str>,
    ext: &super::extraction::Extraction,
    warnings: &mut Vec<String>,
) -> Option<NextCalls> {
    let mut next = page_next(q, pg, head, warnings);
    if let Some(token) = token
        && let Some(read) = next.as_mut().and_then(|next| next.continue_window.as_mut())
    {
        read.query.selection = token.parse().ok();
    }
    let follow_ups = ext.next.clone();
    if !pg.out_of_range && !follow_ups.is_empty() {
        next.get_or_insert_with(NextCalls::default)
            .absorb(follow_ups);
    }
    next
}

/// The head page of an untargeted first read whose view would not fit one
/// line page: paging through it is rarely what an unanchored read needs, and
/// the head plus a locate (or an anchored read) costs a fraction of a page.
/// An outline (`symbols`) is already the orientation view and is never cut.
fn head_query(
    q: &LocalFetchQuery,
    mode: Option<MinifyMode>,
    view_bytes: usize,
) -> Option<LocalFetchQuery> {
    (unanchored_first_read(q)
        && mode != Some(MinifyMode::Symbols)
        && view_bytes > super::pagination::LINE_PAGE_BYTES)
        .then(|| LocalFetchQuery {
            unit: Some(WindowUnit::Lines),
            length: wire_positive(HEAD_LINES),
            ..q.clone()
        })
}

/// The selected text in the view it is returned in (`mode` `None`: as is).
struct View {
    text: String,
    mode: Option<MinifyMode>,
    fallback: Option<MinifyFallback>,
}

/// Apply the requested `minify` view to the extracted text. A match read
/// stays verbatim evidence; an outline the file type cannot give falls
/// back to the standard view.
fn select_view(
    q: &LocalFetchQuery,
    ext: &mut super::extraction::Extraction,
    source_redactions: &mut Vec<(usize, usize)>,
    source_path: &std::path::Path,
    security: &impl ContentScan,
) -> View {
    let requested = local_fetch_query::minify_mode(q);
    let selected = std::mem::take(&mut ext.text);
    // matchString reads are verbatim source; the tool description says so,
    // so the row carries no fallback notice. Byte windows address the
    // selected view (a numbered walk pages an oversized line by view bytes),
    // so they take no view of their own here.
    if q.match_string.is_some() {
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
        let view =
            octocode_engine::minify::apply::apply_content_view_minification(selected, &q.path);
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
        None => view(selected, None),
        Some(MinifyMode::Standard) => view(standard_view(&selected), Some(MinifyMode::Standard)),
        Some(MinifyMode::Symbols) => {
            match symbols_view(
                crate::tools::declarations_cache::symbols_store(),
                &selected,
                &q.path,
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
                    view(outline.clone(), Some(MinifyMode::Symbols))
                }
                // `minifyFallback` is the typed notice of the applied view.
                crate::tools::declarations_cache::SymbolsView {
                    outline: Err(reason),
                    ..
                } => View {
                    text: standard_view(&selected),
                    mode: Some(MinifyMode::Standard),
                    fallback: Some(MinifyFallback {
                        requested: MinifyMode::Symbols,
                        applied: MinifyMode::Standard,
                        reason: reason.as_str().into(),
                    }),
                },
            }
        }
    }
}

/// The page's own continuation: the next page, or a restart for an offset
/// past the end of the view.
fn page_next(
    q: &LocalFetchQuery,
    pg: &super::pagination::Page,
    head: bool,
    warnings: &mut Vec<String>,
) -> Option<NextCalls> {
    if !pg.out_of_range {
        let mut next = continuation(q, &pg.pagination);
        // Paging on from a line head uses the default line page size. A head
        // that fell back to a byte page (one oversized line) keeps its unit
        // and window: they give `offset` its meaning.
        if head
            && pg.pagination.unit == WindowUnit::Lines
            && let Some(read) = next.as_mut().and_then(|next| next.continue_window.as_mut())
        {
            read.query.length = None;
            read.query.unit = None;
        }
        return next;
    }
    let total = match pg.pagination.unit {
        WindowUnit::Lines => pg.pagination.total_lines,
        WindowUnit::Bytes => pg.pagination.total_bytes,
    };
    Some(past_end(q, total, pg.pagination.unit, warnings))
}

/// The one shape of a page asked past the end of its view, whatever reads
/// the source (whole or streamed): nothing returned, a warning naming the
/// view's `total` in `unit`, and the restart at offset 0.
pub(super) fn past_end(
    q: &LocalFetchQuery,
    total: usize,
    unit: WindowUnit,
    warnings: &mut Vec<String>,
) -> NextCalls {
    let unit = match unit {
        WindowUnit::Lines => "lines",
        WindowUnit::Bytes => "bytes",
    };
    warnings.push(format!(
        "offset {} is past the end of the selected view ({total} {unit}); nothing was returned.",
        local_fetch_query::offset(q).unwrap_or(0),
    ));
    let mut query = q.clone();
    query.offset = Some(0);
    NextCalls {
        restart: Some(FetchCall { query }),
        ..NextCalls::default()
    }
}

/// Notes for a read's scope: a reversed range and a long-line byte walk.
/// A large file's head needs none: the description states the head size and
/// the row carries `totalLines` and `continueWindow`.
fn read_scope_warnings(
    q: &LocalFetchQuery,
    pg: &super::pagination::Page,
    warnings: &mut Vec<String>,
) {
    warnings.extend(reversed_range_warnings(q));
    let page = &pg.pagination;
    if let Some(requested) = local_fetch_query::offset(q)
        && q.unit == Some(WindowUnit::Bytes)
        && page.unit == WindowUnit::Bytes
        && !pg.out_of_range
        && page.offset < requested
    {
        warnings.push(format!(
            "Byte offset {requested} falls inside a UTF-8 character; the page starts at its first byte, {}.",
            page.offset
        ));
    }
    if let Some(pages) = long_line_pages(q, pg) {
        warnings.push(long_line_warning(pages));
    }
}

/// A line read fell back to byte pages inside one line this long (a
/// minified bundle, a one-line JSON dump) takes this many pages or more
/// before its first page names the faster routes.
const LONG_LINE_PAGES: usize = 8;

/// The byte pages a file's first page opens, when a line read had to page by
/// bytes (one line over a line page) and there are [`LONG_LINE_PAGES`] or
/// more of them.
fn long_line_pages(q: &LocalFetchQuery, pg: &super::pagination::Page) -> Option<usize> {
    let asked_bytes = q.unit == Some(WindowUnit::Bytes) || q.full_content == Some(true);
    let page = &pg.pagination;
    let pages = page.total_bytes.checked_div(page.length)?;
    (!asked_bytes
        && page.unit == WindowUnit::Bytes
        && page.offset == 0
        && page.has_more
        && pages >= LONG_LINE_PAGES)
        .then(|| page.total_bytes.div_ceil(page.length))
}

fn long_line_warning(pages: usize) -> String {
    use crate::contracts::query_limits::local_fetch::LENGTH_MAXIMUM;
    format!(
        "One line fills this file: {pages} byte pages to read it through. Faster: matchString returns byte windows around each hit \
         (not with `contextLines:0`), or unit:\"bytes\" with length:{LENGTH_MAXIMUM} walks it in fewer, larger pages."
    )
}

/// A reversed range (`"50-10"`) reads the same span low end first; the row
/// says so, so the caller sees how its range was read.
fn reversed_range_warnings(q: &LocalFetchQuery) -> Vec<String> {
    q.ranges
        .iter()
        .filter_map(|range| {
            let (start, end) = crate::tools::line_spans::parse_span_raw::<u64>(range)?;
            (end < start)
                .then(|| format!("Range \"{range}\" is reversed: read as lines {end}-{start}."))
        })
        .collect()
}

/// The source lines a page touches: a raw page's through the view-to-source
/// line map (or its own lines), a transformed view's from its gutter.
fn page_lines(
    pg: &super::pagination::Page,
    source_lines: Option<&[usize]>,
    mode: Option<MinifyMode>,
) -> Vec<usize> {
    if mode.is_some() {
        return pg
            .text
            .lines()
            .filter_map(|line| {
                let (number, _) = line.split_once(crate::response::numbered::SEPARATOR)?;
                number.split('-').next()?.parse().ok()
            })
            .collect();
    }
    let (first, last) = pg.view_lines;
    match source_lines {
        Some(lines) => lines
            .get(first.saturating_sub(1)..last.min(lines.len()))
            .unwrap_or_default()
            .to_vec(),
        None => (first..=last).collect(),
    }
}

/// The source lines a page shows: through the view-to-source line map when
/// the extraction kept one, else the page's own line span.
fn page_source_ranges(
    source_lines: Option<&[usize]>,
    view_lines: (usize, usize),
) -> Vec<LineRange> {
    let Some(lines) = source_lines else {
        return vec![LineRange {
            start: view_lines.0,
            end: view_lines.1,
        }];
    };
    let mut page_lines: Vec<usize> = lines
        [view_lines.0.saturating_sub(1)..view_lines.1.min(lines.len())]
        .iter()
        .copied()
        .filter(|line| *line != super::extraction::OMISSION_LINE)
        .collect();
    page_lines.dedup();
    compress_ranges(&page_lines)
}

/// A first read with nothing selecting what to return: no match, range,
/// block, page cursor, page size, or whole-file request.
fn unanchored_first_read(q: &LocalFetchQuery) -> bool {
    q.match_string.is_none()
        && !local_fetch_query::has_ranges(q)
        && local_fetch_query::start_line(q).is_none()
        && local_fetch_query::end_line(q).is_none()
        && !local_fetch_query::block(q)
        && q.full_content != Some(true)
        && local_fetch_query::offset(q).is_none()
        && local_fetch_query::window_length(q).is_none()
        && q.unit.is_none()
}

/// Same file and caller metadata, every extraction selector reset to one raw line.
pub(super) fn single_line_query(q: &LocalFetchQuery, line: usize) -> LocalFetchQuery {
    let mut query = LocalFetchQuery {
        minify: None,
        full_content: None,
        match_string: None,
        regex: None,
        case_mode: None,
        whole_word: None,
        context_lines: None,
        unit: None,
        offset: None,
        length: None,
        ..q.clone()
    };
    local_fetch_query::set_line_span(&mut query, line, line);
    query
}

/// The continued source changed since its page was cut. Pages of two file
/// versions must not be combined. The row names no restart of its own: the
/// runtime's one restart owner (`response::pages::restart_stale`, through
/// [`crate::response::result::restart_query`]) adds the caller's whole query
/// without its cursors, so a streamed range read restarts every range.
pub(super) fn stale_snapshot(q: &LocalFetchQuery) -> LocalFetchResult {
    LocalFetchResult::error(
        q.path.to_string(),
        "staleSnapshot",
        crate::response::pages::STALE_SNAPSHOT_ERROR.into(),
    )
}

/// The first bounded line page of the same view as a complete read.
fn bounded_query(q: &LocalFetchQuery) -> LocalFetchQuery {
    let mut query = q.clone();
    query.full_content = None;
    query.unit = Some(WindowUnit::Lines);
    query.offset = Some(0);
    query.length = wire_positive(DEFAULT_LINE_CHUNK);
    query
}

pub(crate) fn compress_ranges(lines: &[usize]) -> Vec<LineRange> {
    crate::tools::line_spans::runs(lines.iter().copied())
        .into_iter()
        .map(|(start, end)| LineRange { start, end })
        .collect()
}
pub(super) fn system_time_iso(t: std::time::SystemTime) -> Option<String> {
    let elapsed = t.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(crate::civil_date::iso8601_millis(
        i64::try_from(elapsed.as_millis()).ok()?,
    ))
}

#[cfg(test)]
#[path = "executor_tests.rs"]
mod tests;
