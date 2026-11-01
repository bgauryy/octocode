mod projection;
mod scanned;
mod settle;

use super::cursor::cursor;
use super::layout::{Layout, NamePricing};
use super::local_search_query;
use super::types::{LocalSearchResult, SearchStatus};
use super::verify::{
    CutOutlines, Outlines, PageScope, Redacted, Redaction, outline_for_cut, redact_scan,
    validate_matched, verify_shown,
};
use crate::execution::cancel::CancellationCheck;
use crate::num::u32_of_usize;
use crate::policy::path::PathPolicy;
use crate::response::result::ToolError;
use crate::security::ContentSecurity;
use octocode_contract_types::LocalSearchQuery;
use projection::annotate_rows;
pub(super) use projection::order_files;
#[cfg(test)]
pub(super) use scanned::scan;
pub(super) use scanned::{Scanned, scan_query};
use serde_json::json;
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
    let terms = local_search_query::term_patterns(query);
    let query = &*local_search_query::normalized(query)?;
    let scan = scan_query(query, paths, cancel, (response_window, terms))?;
    page_of(query, paths, security, cancel, scan, selection)
}

/// The page `query` asks for, cut from `scan` (fresh, or stored and proved
/// unchanged when [`scan_query`] took it). Every file the page reads or
/// shows is checked again here, so a file changed since then restarts the
/// snapshot or is denied rather than shown.
pub(super) fn page_of(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &impl CancellationCheck,
    mut scan: Scanned,
    selection: Option<&crate::tools::selection::Runtime>,
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
    };
    let shaped = shape_page(
        &scope,
        &mut scan.parsed,
        scan.page_budget,
        scan.stored_redaction.as_deref(),
        &scan.name_prices,
        selection,
        &scan.identity,
    )?;
    let total_files = u32_of_usize(scan.parsed.files.len());
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
            prices: Arc::clone(&scan.name_prices),
        });
    let selection_token = shaped
        .selection
        .as_ref()
        .map(super::selection::Selected::token);
    let (next, leftover_matches) = cursor(
        query,
        &scan.parsed,
        &shaped.layout,
        &scan.identity,
        reusable,
        std::mem::take(&mut scan.policy_key),
        selection_token,
        &|| cancel.check().is_err(),
    );
    cancel.check().map_err(ToolError::cancelled)?;
    let rows = annotate_rows(
        &scope,
        &shaped.layout,
        std::mem::take(&mut scan.parsed.files),
        &shaped.redacted,
        shaped.outlines,
        scan.page_budget,
    );
    let page = Page {
        root,
        layout: &shaped.layout,
        scanned: &scan.parsed.stats,
        skipped: &scan.skipped,
        search_options: &scan.search_options,
        total_files,
        unverified: shaped.unverified,
        leftover_matches,
    };
    let mut result = settle_row(&scope, &page, rows, next)?;
    if let Some(selected) = &shaped.selection {
        let leads = selected.leads(query, &scan.identity, !root.is_file());
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
        file.path = paths
            .workspace_relative(&absolute)
            .unwrap_or_else(|| absolute.to_string_lossy().into_owned());
    }
    result.source_snapshot = Some(scan.identity);
    result.selection = shaped.selection.map(|selection| selection.outcome.receipt);
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
    pub(super) packed: Option<super::packed::Packed>,
    pub(super) layout: Layout,
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
    stored_redaction: Option<&Redaction>,
    name_prices: &super::manifest::NamePrices,
    selection: Option<&crate::tools::selection::Runtime>,
    identity: &str,
) -> Result<Shaped, ToolError> {
    let PageScope {
        query,
        paths,
        output_root,
        ..
    } = *scope;
    let (mut redacted, redaction) = redact_scan(scope, parsed, stored_redaction)?;
    let packed = if stored_redaction.is_none() {
        super::manifest::pack_checked(parsed, &|| scope.cancel.check().is_err())
    } else {
        None
    };
    order_files(query, &mut parsed.files);
    let selection =
        super::selection::select_hits(scope, parsed, selection, identity, &mut redacted)?;
    let mut cut = CutOutlines::new();
    let mut outline = |files: &[(usize, &octocode_engine::types::TextSearchFile)]| {
        outline_for_cut(scope, files, &mut cut)
    };
    let mut pricing = NamePricing::new(name_prices.by_index(&parsed.files), &mut outline);
    let layout = Layout::cut(query, paths, output_root, parsed, page_budget, &mut pricing)?;
    name_prices.learn(&parsed.files, pricing.priced);
    for (index, _) in &layout.shown {
        validate_matched(paths, &output_root.join(&parsed.files[*index].path))?;
    }
    let mut outlines = Outlines::new();
    let unverified = verify_shown(
        scope,
        parsed,
        &layout,
        &mut redacted,
        (&mut cut, &mut outlines),
    )?;
    Ok(Shaped {
        packed,
        layout,
        redacted,
        redaction,
        unverified,
        outlines,
        selection,
    })
}
