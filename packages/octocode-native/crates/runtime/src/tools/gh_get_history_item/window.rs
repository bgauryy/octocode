//! Public-page windows over GitHub REST collections: provider batch loading
//! derived from public page cursors, and the page objects reported back.
use super::files::max_inventory_page;
use super::{default_page_size, fetch};
use crate::providers::github::{
    CredentialResolver, GitHubTransport, ProviderError, RequestContext,
};
use serde_json::{Value, json};

/// GitHub REST collection batch size (the `per_page` maximum).
pub(super) const PROVIDER_BATCH: usize = 100;
/// PR and commit file lists stop at 3000 files (30 batches of 100).
pub(super) const MAX_FILE_BATCHES: usize = 30;
/// Comments/reviews scanned for one page (3000 items).
pub(super) const MAX_COLLECTION_BATCHES: usize = 30;
/// GitHub lists at most 250 commits for a pull request.
pub(super) const MAX_PR_COMMIT_BATCHES: usize = 3;

/// How much of a provider collection a load covered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WindowState {
    /// Items before the loaded ones, skipped by jumping straight to the first
    /// batch the window needs (unfiltered lists only).
    pub(super) skipped: usize,
    pub(super) exhausted: bool,
    /// Stopped at the batch cap with provider items left.
    pub(super) capped: bool,
}

impl WindowState {
    /// A fully loaded collection (also the state of an absent one).
    pub(super) const COMPLETE: Self = Self {
        skipped: 0,
        exhausted: true,
        capped: false,
    };

    /// Combined state of two collections served as one list.
    pub(super) fn merge(self, other: Self) -> Self {
        Self {
            skipped: 0,
            exhausted: self.exhausted && other.exhausted,
            capped: self.capped || other.capped,
        }
    }

    /// Page `values` loaded under this state and mark a capped scan.
    pub(super) fn paginate(
        self,
        values: Vec<Value>,
        page: Option<usize>,
        page_size: Option<usize>,
    ) -> (Vec<Value>, Value) {
        let (slice, mut page) =
            paginate_window(values, self.skipped, self.exhausted, page, page_size);
        mark_capped(&mut page, self.capped);
        (slice, page)
    }
}

/// Provider items loaded to serve one public page. Batches are derived from
/// the public window `[(page-1)*pageSize, page*pageSize)` so continuations
/// carry only public page cursors, never provider batch numbers.
pub(super) struct Loaded {
    pub(super) items: Vec<Value>,
    pub(super) state: WindowState,
    /// First fetched response with `items` taken out (commit metadata).
    pub(super) first: Value,
}

impl Loaded {
    pub(super) fn complete(items: Vec<Value>) -> Self {
        Self {
            items,
            state: WindowState::COMPLETE,
            first: Value::Null,
        }
    }
}

/// The public page a load must cover, read in `PROVIDER_BATCH`-sized batches.
pub(super) struct WindowSpec {
    pub(super) max_batches: usize,
    pub(super) page: usize,
    pub(super) page_size: usize,
    /// A filter hides items, so the window cannot jump ahead by index.
    pub(super) filtered: bool,
    /// Provider item count when known up front (a PR's `changed_files`). A
    /// filtered scan then reads every batch concurrently instead of one
    /// round trip per batch.
    pub(super) provider_total: Option<usize>,
}

/// Load provider batches until `keep`-matching items cover the requested
/// public page, the provider is exhausted, or the batch cap is hit.
pub(super) async fn load_window<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    segments: &[&str],
    spec: WindowSpec,
    keep: impl Fn(&Value) -> bool,
    context: &RequestContext,
) -> Result<Loaded, ProviderError> {
    load_window_with(transport, segments, spec, keep, array_items, context).await
}

fn array_items(value: &mut Value) -> Vec<Value> {
    match value.take() {
        Value::Array(items) => items,
        _ => Vec::new(),
    }
}

pub(super) fn commit_file_items(value: &mut Value) -> Vec<Value> {
    value
        .get_mut("files")
        .map(Value::take)
        .and_then(|files| match files {
            Value::Array(items) => Some(items),
            _ => None,
        })
        .unwrap_or_default()
}

pub(super) async fn load_window_with<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    segments: &[&str],
    spec: WindowSpec,
    keep: impl Fn(&Value) -> bool,
    extract: fn(&mut Value) -> Vec<Value>,
    context: &RequestContext,
) -> Result<Loaded, ProviderError> {
    if spec.filtered
        && let Some(total) = spec.provider_total.filter(|total| *total > PROVIDER_BATCH)
    {
        return load_all_batches(
            transport,
            segments,
            spec.max_batches,
            total,
            extract,
            context,
        )
        .await;
    }
    let start = spec.page.saturating_sub(1).saturating_mul(spec.page_size);
    let need = start.saturating_add(spec.page_size);
    let mut first_batch = if spec.filtered {
        1
    } else {
        (start / PROVIDER_BATCH).min(spec.max_batches - 1) + 1
    };
    loop {
        let mut loaded = Loaded {
            items: Vec::new(),
            state: WindowState {
                skipped: (first_batch - 1) * PROVIDER_BATCH,
                exhausted: false,
                capped: false,
            },
            first: Value::Null,
        };
        let mut matched = loaded.state.skipped;
        let mut batch = first_batch;
        loop {
            let (mut value, more) = fetch(
                transport,
                segments,
                &[
                    ("per_page", PROVIDER_BATCH.to_string()),
                    ("page", batch.to_string()),
                ],
                context,
            )
            .await?;
            let items = extract(&mut value);
            if batch == first_batch {
                loaded.first = value;
            }
            matched += items.iter().filter(|item| keep(item)).count();
            loaded.items.extend(items);
            if !more {
                loaded.state.exhausted = true;
                break;
            }
            if matched >= need {
                break;
            }
            if batch >= spec.max_batches {
                loaded.state.capped = true;
                loaded.state.exhausted = true;
                break;
            }
            batch += 1;
        }
        // A jump past the real end (stale page cursor) restarts from batch 1
        // so the page clamps to the true last page.
        if loaded.items.is_empty() && first_batch > 1 {
            first_batch = 1;
            continue;
        }
        return Ok(loaded);
    }
}

/// Read every provider batch a collection of `total` items spans (up to
/// `max_batches`) concurrently, in provider order.
async fn load_all_batches<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    segments: &[&str],
    max_batches: usize,
    total: usize,
    extract: fn(&mut Value) -> Vec<Value>,
    context: &RequestContext,
) -> Result<Loaded, ProviderError> {
    let batches = total.div_ceil(PROVIDER_BATCH).clamp(1, max_batches);
    let pages = futures_util::future::try_join_all((1..=batches).map(|batch| async move {
        fetch(
            transport,
            segments,
            &[
                ("per_page", PROVIDER_BATCH.to_string()),
                ("page", batch.to_string()),
            ],
            context,
        )
        .await
    }))
    .await?;
    let last_has_more = pages.last().is_some_and(|(_, more)| *more);
    let mut loaded = Loaded {
        items: Vec::new(),
        state: WindowState {
            skipped: 0,
            exhausted: true,
            capped: last_has_more && batches >= max_batches,
        },
        first: Value::Null,
    };
    for (index, (mut value, _)) in pages.into_iter().enumerate() {
        let items = extract(&mut value);
        if index == 0 {
            loaded.first = value;
        }
        loaded.items.extend(items);
    }
    Ok(loaded)
}

/// Reconcile a changed-file page with the PR's own `changed_files` count.
/// GitHub lists at most 3000 files, and ends the list there without saying
/// so: when the listing ran out before the reported count, the page is not
/// complete. An unfiltered page that stopped early reports the provider
/// total so the caller knows how many pages remain.
pub(super) fn reconcile_file_totals(
    page: &mut Value,
    state: WindowState,
    listed: usize,
    changed_files: Option<usize>,
    filtered: bool,
) {
    let Some(total) = changed_files else { return };
    let listable = total.min(MAX_FILE_BATCHES * PROVIDER_BATCH);
    if !state.exhausted && !filtered {
        let per = page["itemsPerPage"].as_u64().unwrap_or(1).max(1) as usize;
        page["totalItems"] = json!(listable);
        page["totalPages"] = json!(listable.div_ceil(per));
        page["countScope"] = json!("complete");
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
            "changedFiles": total
        });
    }
}

pub(super) fn paginate_collection(
    values: Vec<Value>,
    page: Option<usize>,
    page_size: Option<usize>,
) -> (Vec<Value>, Value) {
    paginate_window(values, 0, true, page, page_size)
}

/// Page `values` (the filtered list, preceded by `skipped` items that were
/// never loaded). An exhausted list reports exact totals and clamps the page;
/// otherwise the counts cover only what was loaded.
pub(super) fn paginate_window(
    values: Vec<Value>,
    skipped: usize,
    exhausted: bool,
    page: Option<usize>,
    page_size: Option<usize>,
) -> (Vec<Value>, Value) {
    let per = page_size
        .unwrap_or_else(default_page_size)
        .clamp(1, max_inventory_page());
    let loaded = skipped + values.len();
    let pages = loaded.div_ceil(per).max(1);
    let requested = page.unwrap_or(1).max(1);
    let current = if exhausted {
        requested.min(pages)
    } else {
        requested
    };
    let start = (current - 1) * per;
    let more = if exhausted { current < pages } else { true };
    let slice = values
        .into_iter()
        .skip(start.saturating_sub(skipped))
        .take(per)
        .collect();
    let mut page = json!({"currentPage":current,"itemsPerPage":per,"totalItems":loaded,"hasMore":more,"nextPage":more.then_some(current+1),"countScope":if exhausted {"complete"} else {"loaded"}});
    if exhausted {
        page["totalPages"] = json!(pages);
    }
    (slice, page)
}

pub(super) fn mark_capped(page: &mut Value, capped: bool) {
    if capped {
        // Totals cover only the batches read before the cap, not the item.
        page["countScope"] = json!("partial");
        page["terminalLimit"] = json!(true);
        page["providerLimit"] = json!({"reason":"providerBatchLimit"});
    }
}

/// Rename a generic page to the changed-file page vocabulary.
pub(super) fn commit_files_pagination(mut page: Value) -> Value {
    if let Some(map) = page.as_object_mut() {
        if let Some(v) = map.remove("totalItems") {
            map.insert("totalFiles".into(), v);
        }
        if let Some(v) = map.remove("nextPage") {
            map.insert("nextFilePage".into(), v);
        }
    }
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_pages_span_provider_batches_without_provider_cursors() {
        // Page 5 × 30 = items 120..150: an unfiltered load starts at provider
        // batch 2 (items 100..), so 100 items are skipped, never fetched.
        let items = (100..140).map(|n| json!(n)).collect::<Vec<_>>();
        let (slice, page) = paginate_window(items.clone(), 100, false, Some(5), Some(30));
        assert_eq!(slice.first(), Some(&json!(120)));
        assert_eq!(slice.len(), 20);
        assert_eq!(page["hasMore"], true);
        assert_eq!(page["nextPage"], 6);
        assert_eq!(page["countScope"], "loaded");
        let (slice, page) = paginate_window(items, 100, true, Some(5), Some(30));
        assert_eq!(slice.len(), 20);
        assert_eq!(page["hasMore"], false);
        assert_eq!(page["totalItems"], 140);
        assert_eq!(page["totalPages"], 5);
        assert_eq!(page["countScope"], "complete");
    }
}
