//! Discovery paging: the page facts, the next page and the deepest reachable page.
use super::{ArtifactSearchQuery, artifact_search_query};
use crate::id::ToolId;
use crate::num::usize_of;
use crate::providers::artifact::{ArtifactProviderState, page_out_of_range, reachable_end};
use crate::response::paging;
use crate::response::result::Continuation;
use serde_json::Value;

/// A discovery page's facts, from the shared page facts: the page, its size,
/// whether a reachable page follows and the registry's matched total when
/// it counts one, and the next page, which carries the whole query plus the cursor a
/// token-paged registry issued for it (`next_state`). A registry total
/// past the reachable rows is no page count: the first page states the
/// reach once (a static notice) and the last reachable page ends the row at
/// that terminal limit (`hasMore:false`), with no next page.
pub(super) fn discovery_page(
    query: &ArtifactSearchQuery,
    data: &mut Value,
    next_state: Option<&ArtifactProviderState>,
    total: Option<u64>,
) {
    let current = artifact_search_query::page(query);
    let more = next_state.is_some();
    let mut next_query = query.clone();
    artifact_search_query::set_page(&mut next_query, current.saturating_add(1));
    artifact_search_query::set_cursor(
        &mut next_query,
        next_state.and_then(|next| next.token.as_deref()),
    );
    let unreachable = more.then(|| page_out_of_range(&next_query)).flatten();
    let facts = crate::response::pages::PageFacts::open(
        usize_of(current),
        Some(artifact_search_query::page_size(query)),
        more && unreachable.is_none(),
    );
    let last = last_reachable_page(query);
    let artifact_type = artifact_search_query::artifact_type(query);
    let reach = last
        .saturating_mul(artifact_search_query::page_size(query) as u64)
        .min(reachable_end(artifact_type).unwrap_or(u64::MAX));
    // `totalItems` is the registry's matched total, whatever the page
    // size; the rows paging reaches (the registry's paging range) are the
    // warning's.
    let facts = match total.and_then(|total| usize::try_from(total).ok()) {
        Some(total) => facts.with_items(total),
        None => facts,
    };
    data["pagination"] = facts.to_value();
    let capped_total = total.filter(|total| *total > reach);
    if let Some(limit) = unreachable {
        let warning = capped_total.map_or(limit.warning, |total| {
            format!(
                "{artifact_type} matched {total} results; paging reaches only the first {reach}, which end on this page."
            )
        });
        paging::terminal_limit(data, limit.reason, &[warning]);
        return;
    }
    if !more {
        return;
    }
    if let Ok(next_query) = serde_json::to_value(next_query) {
        data["calls"]["nextPage"] =
            Continuation::continued(ToolId::ArtifactSearch, next_query).build();
    }
    if let Some(total) = capped_total {
        paging::mark_partial(
            data,
            crate::response::paging::PartialReason::ProviderResultCap,
            &[],
        );
        paging::push_notice(
            data,
            format!(
                "{artifact_type} matched {total} results; paging reaches only the first {reach} (through page {last})."
            ),
        );
    }
}

/// The deepest discovery page `query`'s page size reaches: the last whose
/// registry position is in the registry's paging range
/// ([`page_out_of_range`], the provider's own rule).
pub(crate) fn last_reachable_page(query: &ArtifactSearchQuery) -> u64 {
    let reaches = |page: u64| {
        let mut probe = query.clone();
        artifact_search_query::set_page(&mut probe, page);
        page_out_of_range(&probe).is_none()
    };
    // Reach is monotone in the page: double past the last reachable page,
    // then bisect; `low` reaches, `high` bounds the search.
    let mut high = 1u64;
    while reaches(high) {
        match high.checked_mul(2) {
            Some(next) => high = next,
            None => return high,
        }
    }
    let mut low = 1;
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if reaches(middle) {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    low
}
