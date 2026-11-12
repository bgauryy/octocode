mod projection;
mod scanned;
mod settle;

use super::cursor::cursor;
use super::layout::{Layout, NamePricing};
use super::local_search_query;
use super::page_view::{PageView, StoredPage};
use super::probes::Alternates;
use super::types::{LocalSearchResult, SearchStatus};
use super::verify::{
    CutOutlines, Outlines, PageScope, Redacted, Redaction, check_key_scans, outline_for_cut,
    redact_scan, validate_matched, verify_shown,
};
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::response::result::ToolError;
use crate::security::ContentSecurity;
use octocode_contract_types::LocalSearchQuery;
use octocode_engine::types::TextSearchResult;
use projection::annotate_rows;
pub(super) use projection::order_files;
#[cfg(test)]
pub(super) use scanned::scan;
#[cfg(test)]
pub(super) use scanned::scan_query;
#[cfg(test)]
pub(super) use scanned::test_held_limit;
pub(super) use scanned::{Scanned, scan_resumed, search_options};
use serde_json::json;
pub(super) use settle::search_stats;
use settle::{Page, settle_row};
use std::sync::Arc;

/// One localSearch row; `selection` judges it under `classification`. A list
/// `matchString` runs as the one regex query that finds the same lines.
pub fn execute_local_search(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &impl CancellationCheck,
    response_window: Option<usize>,
    selection: Option<&crate::tools::selection::Runtime>,
) -> Result<LocalSearchResult, ToolError> {
    cancel.check().map_err(ToolError::cancelled)?;
    let (resumed, query) = super::window::resumed(query)?;
    let query = &*query;
    check_each_term(query)?;
    let terms = local_search_query::term_patterns(query);
    let joined = &*local_search_query::normalized(query)?;
    let mut scan = scan_resumed(joined, paths, cancel, (response_window, terms), resumed)?;
    if let Some(pause) = scan.paused.take() {
        return super::window::paused_row(query, pause, (&scan.query_key, &scan.policy_key));
    }
    let alternates = Alternates::new(query, &scan);
    let mut row = page_of(
        joined,
        paths,
        security,
        cancel,
        scan,
        selection,
        &alternates,
    )?;
    alternates.offer_regex_reading(paths, &mut row, cancel);
    Ok(row)
}

/// A bad term of a list fails naming that term: the list runs as one joined
/// regex, whose error would name a pattern the caller never wrote.
fn check_each_term(query: &LocalSearchQuery) -> Result<(), ToolError> {
    let terms = &local_search_query::patterns(query).0;
    if terms.len() < 2
        || local_search_query::regex_mode(query) == octocode_contract_types::RegexEngine::Literal
    {
        return Ok(());
    }
    for term in terms {
        let Ok(mut row) = serde_json::to_value(query) else {
            return Ok(());
        };
        row["matchString"] = serde_json::json!([term.as_str()]);
        if let Ok(single) = serde_json::from_value::<LocalSearchQuery>(row) {
            super::pattern::check_pattern(&single)?;
        }
    }
    Ok(())
}

/// The page `query` asks for, cut from `scan` (fresh, or stored and proved
/// unchanged when [`scan_resumed`] took it). Every file the page reads or
/// shows is checked again here, so a file changed since then restarts the
/// snapshot or is denied rather than shown.
pub(super) fn page_of(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &impl CancellationCheck,
    mut scan: Scanned,
    selection: Option<&crate::tools::selection::Runtime>,
    alternates: &Alternates<'_>,
) -> Result<LocalSearchResult, ToolError> {
    // The bytes a stored scan's secret check must read, or `None` when the
    // scan is fresh; a stored file whose digest is missing reads as changed.
    let digests = scan.digests.take();
    let expected_digest = |source: &std::path::Path| match &digests {
        Some(digests) => super::sources::Expected::Stored(digests.get(source).copied()),
        None => super::sources::Expected::Fresh,
    };
    let root = &scan.validated.canonical;
    refuse_binary_target(root, &scan.validated.display, &scan.parsed.stats)?;
    let output_root = if root.is_file() {
        root.parent().unwrap_or(root)
    } else {
        root.as_path()
    };
    let scope = PageScope {
        query,
        paths,
        output_root,
        expected_digest: &expected_digest,
        security,
        cancel,
        judged: &scan.learned.judged,
    };
    let shaped = match scan.seek.take() {
        Some(packed) => shape_stored_page(&scope, &packed)?,
        None => shape_page(
            &scope,
            &mut scan.parsed,
            scan.page_budget,
            (scan.stored_redaction.as_deref(), scan.reusable.is_some()),
            &scan.learned,
            selection,
            &scan.identity,
        )?,
    };
    let mut view = shaped.view;
    let reusable = scan
        .reusable
        .take()
        .zip(shaped.packed)
        .map(|(walked, value)| super::manifest::Fresh {
            value,
            query_key: std::mem::take(&mut scan.query_key),
            redaction: shaped.redaction,
            skipped: Arc::clone(&scan.skipped),
            walked,
            learned: Arc::clone(&scan.learned),
        });
    let selection_token = shaped
        .selection
        .as_ref()
        .map(super::selection::Selected::token);
    let (next, match_next, leftover_matches) = cursor(
        query,
        &view,
        &scan.identity,
        reusable,
        std::mem::take(&mut scan.policy_key),
        selection_token,
        &|| cancel.check().is_err(),
    )?;
    cancel.check().map_err(ToolError::cancelled)?;
    let rows = annotate_rows(
        &scope,
        &view.layout,
        (std::mem::take(&mut view.parsed.files), &view.facts),
        &shaped.redacted,
        shaped.outlines,
    );
    let page = Page {
        root,
        layout: &view.layout,
        scanned: &view.parsed.stats,
        skipped: &scan.skipped,
        search_options: &scan.search_options,
        alternates,
        total_files: view.totals.files,
        unverified: shaped.unverified,
        leftover_matches,
        classified_hits: shaped
            .selection
            .as_ref()
            .is_some_and(|selected| selected.receipt().total_items > 0),
    };
    let continuation_missing = next.is_none() && match_next.is_none();
    let mut result = settle_row(&scope, &page, rows, next, continuation_missing)?;
    if let Some(call) = match_next {
        for file in &mut result.files {
            if file.pagination.as_ref().is_some_and(|page| page.has_more) {
                file.next = Some(json!({"nextPage":call.clone()}));
            }
        }
    }
    if let Some(selected) = &shaped.selection {
        let leads = selected.leads(query, &scan.identity)?;
        if !leads.is_empty() {
            let next = result.next.get_or_insert_with(|| json!({}));
            for (name, lead) in leads {
                next[name] = lead;
            }
        }
        // The pre-selection totals hold for every page: said on the first.
        if local_search_query::page(query).max(1) == 1
            && local_search_query::match_page(query).max(1) == 1
        {
            result.warnings.extend(selected.disclosure());
        }
        // Hits were judged and none kept: an empty row whose unfiltered
        // lead is its recovery (zero kept is not absence).
        if selected.receipt().total_items > 0 && result.files.is_empty() {
            result.status = SearchStatus::Empty;
        }
    }
    for file in &mut result.files {
        let absolute = output_root.join(&file.path);
        // A row of a source judged whole shows cuts of that judgment, with
        // the line numbers and joins of its window: judged, for the gate.
        if security.judged_whole(&absolute) {
            for row in file.matches.iter().flatten() {
                security.show(&absolute, &row.value);
            }
        }
        file.path = paths
            .workspace_relative(&absolute)
            .unwrap_or_else(|| absolute.to_string_lossy().into_owned());
    }
    result.source_snapshot = Some(scan.identity);
    result.selection = shaped
        .selection
        .and_then(super::selection::Selected::into_receipt);
    Ok(result)
}

/// A search whose whole target is one file the scan skipped as binary from
/// its leading bytes searched nothing: an empty row there would read as
/// absence, so it is refused as astSearch and localFetch refuse it. A file
/// cut at a later NUL (`binaryQuit`) was searched up to it and stays a row.
fn refuse_binary_target(
    root: &std::path::Path,
    shown: &str,
    scanned: &octocode_engine::types::TextSearchStats,
) -> Result<(), ToolError> {
    if root.is_file() && scanned.skipped_binary_count.unwrap_or(0) > 0 {
        return Err(ToolError::policy(crate::policy::PolicyError::new(
            crate::policy::PolicyErrorCode::BinaryContent,
            format!(
                "{shown} is binary content (a NUL in its leading bytes), not text; it was not searched."
            ),
        )));
    }
    Ok(())
}

/// A cut page whose shown values are verified.
pub(super) struct Shaped {
    pub(super) packed: Option<super::packed::Encoded>,
    /// The page cut, as its own small scan.
    pub(super) view: PageView,
    pub(super) redacted: Redacted,
    /// What redacting the scan changed, for a stored scan's later pages.
    pub(super) redaction: Redaction,
    /// Some values were redacted because their source could not be re-read.
    pub(super) unverified: bool,
    /// Outlines parsed from the verified reads (see [`verify_shown`]).
    pub(super) outlines: Outlines,
    pub(super) selection: Option<super::selection::Selected>,
}

/// Redact the scan, order it, cut the page, and verify what it shows.
/// A fresh scan (`fresh`) is encoded for its cursor: by page once its cut
/// is made, or whole when its page judges rows before cutting
/// (classification), shows none (a list view) or cuts a match-page grid;
/// see [`super::packed`].
///
/// Every file the page reads or shows passes the read policy first: the
/// key-block reads inside [`redact_scan`] and each shown file here. A file
/// the page neither reads nor shows is checked on the page that shows it,
/// so a path swapped for a symlink after the walk never has its bytes read
/// or its rows shown.
pub(super) fn shape_page(
    scope: &PageScope<'_, impl CancellationCheck>,
    parsed: &mut octocode_engine::types::TextSearchResult,
    page_budget: usize,
    (stored_redaction, fresh): (Option<&Redaction>, bool),
    learned: &super::manifest::Learned,
    selection: Option<&crate::tools::selection::Runtime>,
    identity: &str,
) -> Result<Shaped, ToolError> {
    let PageScope {
        query,
        paths,
        output_root,
        ..
    } = *scope;
    let cancelled = || scope.cancel.check().is_err();
    let (mut redacted, withheld, redaction) = redact_scan(scope, parsed, stored_redaction)?;
    let whole = local_search_query::has_classification(query)
        || super::layout::list_view(local_search_query::view(query))
        || !local_search_query::streamed_layout(query);
    // A whole scan is encoded before the page reorders and fills it;
    // deflated only if the page's cursor keeps it (see
    // `manifest::put_checked`).
    let mut packed = (fresh && whole)
        .then(|| super::manifest::encode_checked(parsed, &cancelled))
        .flatten();
    // The files with a key-block scan, by path (the frames of a seekable
    // scan are written in page order): each page that shows one reads it
    // again.
    let key_scans = if fresh && !whole {
        redaction
            .key_files
            .iter()
            .map(|key| key.path.clone())
            .collect::<std::collections::HashSet<_>>()
    } else {
        std::collections::HashSet::new()
    };
    order_files(query, &mut parsed.files);
    let selection = super::selection::select_hits(
        scope,
        parsed,
        selection,
        identity,
        (&mut redacted, &withheld),
    )?;
    let mut cut = CutOutlines::new();
    let mut outline = |files: &[(usize, &octocode_engine::types::TextSearchFile)]| {
        outline_for_cut(scope, files, &mut cut)
    };
    let mut pricing = NamePricing::new(learned.prices.by_index(&parsed.files), &mut outline);
    // A selection keeps other rows than the stored scan's cut was made of.
    let cuts = selection.is_none().then_some(&learned.cuts);
    let layout = Layout::cut(
        query,
        paths,
        output_root,
        parsed,
        page_budget,
        (&mut pricing, cuts),
    )?;
    learned.prices.learn(&parsed.files, pricing.priced);
    for (index, _) in &layout.shown {
        validate_matched(paths, &output_root.join(&parsed.files[*index].path))?;
    }
    // A seekable scan is encoded once its cut is made and before the page
    // verifies its own rows: a stored page holds values as the scan had
    // them, never as one page materialized them.
    if fresh && !whole {
        packed = super::page_view::encode_pages(
            parsed,
            &layout,
            (&redacted, &key_scans),
            octocode_engine::memory::MEMORY.limit(),
            &cancelled,
        );
    }
    let whole = TextSearchResult {
        files: std::mem::take(&mut parsed.files),
        stats: parsed.stats.clone(),
    };
    let (mut view, indexes) = PageView::dense(whole, layout);
    // The cut's outlines, by the page file each was parsed for.
    let mut cut = indexes
        .iter()
        .enumerate()
        .filter_map(|(page, index)| Some((page, cut.remove(index)?)))
        .collect::<CutOutlines>();
    let mut outlines = Outlines::new();
    let unverified = verify_shown(
        scope,
        &mut view.parsed,
        &view.layout,
        (&mut redacted, &withheld),
        (&mut cut, &mut outlines),
    )?;
    Ok(Shaped {
        packed,
        view,
        redacted,
        redaction,
        unverified,
        outlines,
        selection,
    })
}

/// The page of a seekable stored scan, read from its own frames: no row is
/// redacted, ordered, ranked or priced again (its first page did that once
/// for every page). Its files' key-block scans are checked against their
/// stored bytes, and its shown values verified, as on any page.
fn shape_stored_page(
    scope: &PageScope<'_, impl CancellationCheck>,
    packed: &super::packed::Packed,
) -> Result<Shaped, ToolError> {
    let PageScope {
        query,
        paths,
        output_root,
        cancel,
        ..
    } = *scope;
    // A selection token continues only the classification page that
    // issued it.
    if local_search_query::selection(query).is_some() {
        return Err(super::cursor::stale_snapshot(query));
    }
    let Some(StoredPage {
        mut view,
        mut redacted,
        key_scans,
    }) = PageView::stored(packed, query, &|| cancel.check().is_err())
    else {
        cancel.check().map_err(ToolError::cancelled)?;
        return Err(super::cursor::changed_since_scan(query));
    };
    check_key_scans(scope, &view.parsed.files, &key_scans)?;
    for (index, _) in &view.layout.shown {
        validate_matched(paths, &output_root.join(&view.parsed.files[*index].path))?;
    }
    let mut outlines = Outlines::new();
    let unverified = verify_shown(
        scope,
        &mut view.parsed,
        &view.layout,
        (&mut redacted, &super::verify::Withheld::new()),
        (&mut CutOutlines::new(), &mut outlines),
    )?;
    Ok(Shaped {
        packed: None,
        view,
        redacted,
        redaction: Redaction::default(),
        unverified,
        outlines,
        selection: None,
    })
}
