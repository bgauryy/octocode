//! The page objects a window reports back: provider totals reconciled,
//! the public page of a loaded window, and lifted or capped limits.
use super::{MAX_FILE_BATCHES, WindowState};
use crate::tools::gh_get_history_item::DEFAULT_PAGE_SIZE;
use crate::tools::gh_get_history_item::inventory::max_inventory_page;
use octocode_github::MAX_PER_PAGE;
use serde_json::{Value, json};

/// A provider list that ended before an authoritative item count is partial,
/// even when its last (short) batch carries no next link.
pub(in crate::tools::gh_get_history_item) fn reconcile_collection_total(
    page: &mut Value,
    state: WindowState,
    listed: usize,
    total: Option<usize>,
    reason: &str,
) -> bool {
    let missing = state.exhausted && total.is_some_and(|total| listed < total);
    if missing {
        page["countScope"] = json!("partial");
        page["terminalLimit"] = json!(true);
        page["providerLimit"] = json!({"reason": reason});
    }
    missing
}

/// Reconcile a changed-file page with the PR's own `changed_files` count.
/// GitHub lists at most 3000 files, and ends the list there without saying
/// so: when the listing ran out before the reported count, the page is not
/// complete. An unfiltered page that stopped early reports the provider
/// total so the caller knows how many pages remain.
pub(in crate::tools::gh_get_history_item) fn reconcile_file_totals(
    page: &mut Value,
    state: WindowState,
    listed: usize,
    changed_files: Option<usize>,
    filtered: bool,
) {
    let Some(total) = changed_files else { return };
    let listable = total.min(MAX_FILE_BATCHES * MAX_PER_PAGE);
    if !state.exhausted && !filtered {
        let per = page["pageSize"].as_u64().unwrap_or(1).max(1) as usize;
        let current = page["page"].as_u64().unwrap_or(1) as usize;
        let more = page["hasMore"] == true;
        // Rebuilt from the facts, so the block keeps the one key order.
        let mut rebuilt = crate::response::pages::PageFacts::open(current, Some(per), more)
            .with_total(listable)
            .to_value();
        if let (Some(rebuilt), Some(old)) = (rebuilt.as_object_mut(), page.as_object()) {
            for (key, value) in old {
                rebuilt.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }
        rebuilt["countScope"] = json!("complete");
        *page = rebuilt;
    }
    let unlisted = if state.exhausted {
        listed < total
    } else {
        total > listable
    };
    if unlisted && !state.capped {
        page["countScope"] = json!("partial");
        page["terminalLimit"] = json!(true);
        page["providerLimit"] = json!({
            "reason":"providerFileListLimit",
            "listed": if state.exhausted { listed } else { listable },
            "changedFilesCount": total
        });
    }
}

/// Page `values` (the filtered list, preceded by `skipped` items that were
/// never loaded). An exhausted list reports exact totals and flags a page past its end;
/// otherwise the counts cover only what was loaded.
pub(in crate::tools::gh_get_history_item) fn paginate_window<T>(
    values: Vec<T>,
    skipped: usize,
    exhausted: bool,
    page: Option<usize>,
    page_size: Option<usize>,
) -> (Vec<T>, Value) {
    let per = page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, max_inventory_page());
    let loaded = skipped + values.len();
    let pages = crate::response::page_windows::count_pages(loaded, per);
    let requested = page.unwrap_or(1).max(1);
    // A page past an exhausted list is empty and flagged, never clamped to
    // the last page, which would repeat rows the caller already has.
    let current = requested;
    let past_end = exhausted && current > pages;
    let start = (current - 1) * per;
    let more = if exhausted { current < pages } else { true };
    let slice = values
        .into_iter()
        .skip(start.saturating_sub(skipped))
        .take(per)
        .collect();
    let facts =
        crate::response::pages::PageFacts::open(current, Some(per), more).out_of_range(past_end);
    // Only an exhausted listing knows its page count.
    let mut page = if exhausted {
        facts.with_total(loaded)
    } else {
        facts.with_items(loaded)
    }
    .to_value();
    if more {
        page["nextPage"] = json!(current + 1);
    }
    page["countScope"] = json!(if exhausted { "complete" } else { "loaded" });
    (slice, page)
}

/// A file page's counters are stats (dropped by default), so a limit that
/// ends its coverage is restated on the row itself: `terminalLimit`, and
/// the provider limit's reason in `partialReasons`. The page schema does not
/// declare `terminalLimit`, so lifting moves it off the page.
pub(in crate::tools::gh_get_history_item) fn lift_page_limits(row: &mut Value, page: &mut Value) {
    if page
        .as_object_mut()
        .and_then(|page| page.remove("terminalLimit"))
        != Some(Value::Bool(true))
    {
        return;
    }
    match page
        .pointer("/providerLimit/reason")
        .and_then(Value::as_str)
    {
        Some(reason) => crate::response::paging::terminal_limit(row, reason, &[]),
        None => row["terminalLimit"] = json!(true),
    }
}

pub(super) fn mark_capped(page: &mut Value, capped: bool) {
    if capped {
        // Totals cover only the batches read before the cap, not the item.
        page["countScope"] = json!("partial");
        page["terminalLimit"] = json!(true);
        page["providerLimit"] = json!({"reason":"providerBatchLimit"});
    }
}
