//! Discovery paging: the page facts, the next page and the deepest reachable page.
use super::{ArtifactSearchQuery, artifact_search_query};
use crate::id::ToolId;
use crate::num::usize_of;
use crate::providers::artifact::{ArtifactProviderState, page_out_of_range, reachable_end};
use crate::response::paging;
use crate::response::result::Continuation;
use serde_json::{Value, json};

/// A discovery page's facts, from the shared page facts: the page, its size,
/// whether a reachable page follows and the registry's total when it counts
/// one, and the next page, which carries the whole query plus the cursor a
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
    // `totalItems` counts the rows the pages reach; the registry's total
    // past them stays in the warning.
    let facts = match total.and_then(|total| usize::try_from(total.min(reach)).ok()) {
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
    let next = usize_of(artifact_search_query::page(&next_query));
    // The contract's last page: the row ends at that terminal limit.
    if paging::next_cursor(data, ToolId::ArtifactSearch, None, "page", next).is_none() {
        data["pagination"]["hasMore"] = json!(false);
        return;
    }
    if let Ok(next_query) = serde_json::to_value(next_query) {
        data["next"]["nextPage"] = Continuation::new(ToolId::ArtifactSearch, next_query).build();
    }
    if let Some(total) = capped_total {
        paging::mark_partial(data, "providerResultCap", &[]);
        paging::push_notice(
            data,
            format!(
                "{artifact_type} matched {total} results; paging reaches only the first {reach} (through page {last})."
            ),
        );
    }
}

/// The deepest discovery page `query`'s page size reaches: the last the
/// contract's `page` maximum admits whose registry position is in the
/// paging range.
pub(crate) fn last_reachable_page(query: &ArtifactSearchQuery) -> u64 {
    let reaches = |page: u64| {
        let mut probe = query.clone();
        artifact_search_query::set_page(&mut probe, page);
        page_out_of_range(&probe).is_none()
    };
    let maximum = crate::contracts::query_limits::artifact_search::PAGE_MAXIMUM as u64;
    // Reach is monotone in the page: `low` reaches, `high` bounds the search.
    let (mut low, mut high) = (1, maximum);
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

/// A repaired row's `page` past the deepest page its query reaches,
/// lowered to that page: the schema's `maximum` admits pages no search of
/// this size reaches. Any other row (no discovery query, or a reachable
/// page) stays as it is.
pub(crate) fn settle_repaired_row(row: &mut Value) {
    let Ok(query) = serde_json::from_value::<ArtifactSearchQuery>(row.clone()) else {
        return;
    };
    let last = last_reachable_page(&query);
    if artifact_search_query::page(&query) > last {
        row["page"] = json!(last);
    }
}
