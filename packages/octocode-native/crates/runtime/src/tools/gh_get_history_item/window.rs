//! Public-page windows over GitHub REST collections: provider batch loading
//! derived from public page cursors, and the page objects reported back.
use super::fetch;
use octocode_github::MAX_PER_PAGE;
use octocode_github::{GitHubTransport, ProviderError, RequestContext};
use serde_json::Value;

mod budget;
mod pages;
#[cfg(test)]
mod tests;

pub(super) use budget::{auto_page, literal_patch_window, patch_window};
use pages::mark_capped;
pub(super) use pages::{
    lift_page_limits, paginate_window, reconcile_collection_total, reconcile_file_totals,
};

// GitHub's collection caps, in one table: provider facts, not tuning.

/// GitHub's compare endpoint lists at most this many changed files.
pub(super) const COMPARE_FILE_LIMIT: usize = 300;
/// PR and commit file lists stop at 3000 files (30 batches of 100): a
/// documented provider ceiling, not tuning. GitHub REST "List pull request
/// files": "Responses include a maximum of 3000 files"; "Get a commit":
/// pagination "up to a limit of 3000 files". Files past it are unreachable
/// through the API, so a capped window discloses the terminal limit.
pub(super) const MAX_FILE_BATCHES: usize = 30;
/// A commit's files page 300 at a time (GitHub's default and largest
/// `per_page` there), so a commit of up to 300 files is one batch with an
/// exact count.
pub(super) const COMMIT_FILE_BATCH: usize = 300;
/// Commit file batches up to the 3000-file stop.
pub(super) const MAX_COMMIT_FILE_BATCHES: usize =
    MAX_FILE_BATCHES * MAX_PER_PAGE / COMMIT_FILE_BATCH;
/// Comments, reviews and discussion: GitHub caps only `per_page` (100)
/// for these lists, so a window reads every batch its page needs and the
/// call's own deadline bounds a filtered scan.
pub(super) const MAX_COLLECTION_BATCHES: usize = usize::MAX;
/// GitHub lists at most 250 commits for a pull request.
pub(super) const MAX_PR_COMMIT_BATCHES: usize = 3;

/// The private `(page, pageSize)` pair whose page starts exactly at item `start`
/// of a list GitHub pages by whole pages: the largest page size within
/// `max_size` that divides `start`, so every item past `start` falls on
/// exactly one later page.
pub(super) fn page_starting_at(start: usize, max_size: usize) -> (usize, usize) {
    let size = (1..=max_size.max(1))
        .rev()
        .find(|size| start.is_multiple_of(*size))
        .unwrap_or(1);
    (start / size + 1, size)
}

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

    /// Page `values` loaded under this state and mark a capped scan.
    pub(super) fn paginate<T>(
        self,
        values: Vec<T>,
        page: Option<usize>,
        page_size: Option<usize>,
    ) -> (Vec<T>, Value) {
        let (slice, mut page) =
            paginate_window(values, self.skipped, self.exhausted, page, page_size);
        mark_capped(&mut page, self.capped);
        (slice, page)
    }
}

/// Provider items loaded for one result page. The private saved `page` and
/// `pageSize` define its item window; provider batch numbers are derived
/// here and never exposed as authored input.
pub(super) struct Loaded {
    pub(super) items: Vec<Value>,
    pub(super) state: WindowState,
    /// First fetched response with `items` taken out (commit metadata).
    pub(super) first: Value,
}

impl Loaded {
    /// The commit endpoint can stop at its file limit without a next link or
    /// authoritative total. Count skipped raw files too: later unfiltered
    /// windows can reach that boundary without loading the earlier batches.
    pub(super) fn mark_commit_file_limit(&mut self) {
        if self.state.exhausted
            && self.state.skipped.saturating_add(self.items.len())
                >= MAX_COMMIT_FILE_BATCHES * COMMIT_FILE_BATCH
        {
            self.state.capped = true;
        }
    }

    pub(super) fn complete(items: Vec<Value>) -> Self {
        Self {
            items,
            state: WindowState::COMPLETE,
            first: Value::Null,
        }
    }

    /// The first items of a longer collection: they cover the first page,
    /// and later pages read on from where they end.
    pub(super) fn prefix(items: Vec<Value>) -> Self {
        Self {
            items,
            state: WindowState {
                skipped: 0,
                exhausted: false,
                capped: false,
            },
            first: Value::Null,
        }
    }
}

/// Provider batches read at once: a known span loads in waves of this many
/// requests, in provider order.
const BATCH_CONCURRENCY: usize = 8;

/// One provider batch response: the body and whether a next batch exists.
pub(super) type Batch = (Value, bool);

/// The public page a load must cover, read in `batch_size`-sized batches.
pub(super) struct WindowSpec {
    /// Provider items per batch (`per_page`).
    pub(super) batch_size: usize,
    pub(super) max_batches: usize,
    pub(super) page: usize,
    pub(super) page_size: usize,
    /// A filter hides items, so the window cannot jump ahead by index.
    pub(super) filtered: bool,
    /// Provider item count when known up front (a PR's `changed_files`).
    /// It sizes the batches a window spans, so they load concurrently: every
    /// batch of a filtered scan, the batches under an unfiltered page.
    pub(super) provider_total: Option<usize>,
    /// A batch the read already holds, by its 1-based number (the first
    /// batch fetched beside the PR): it is used, never fetched again.
    pub(super) prefetched: Option<(usize, Batch)>,
}

impl WindowSpec {
    /// The provider batch the window starts at: the first for a filtered
    /// window, else the one holding the page's first item.
    pub(super) fn first_batch(&self) -> usize {
        if self.filtered {
            1
        } else {
            (self.start() / self.batch_size).min(self.max_batches.saturating_sub(1)) + 1
        }
    }

    /// Whether the page may need more than its first provider batch: a
    /// filtered scan (its matches lie anywhere), or a page whose items cross
    /// a batch boundary.
    pub(super) fn spans_batches(&self) -> bool {
        self.filtered || self.need().div_ceil(self.batch_size) > self.first_batch()
    }

    fn start(&self) -> usize {
        self.page.saturating_sub(1).saturating_mul(self.page_size)
    }

    /// Items (matches, for a filtered window) the page needs loaded.
    fn need(&self) -> usize {
        self.start().saturating_add(self.page_size)
    }

    /// The last batch of the wave that starts at `next`: through every batch
    /// the known provider total says the window spans, else `next` alone.
    fn wave_end(&self, next: usize) -> usize {
        self.provider_total.map_or(next, |total| {
            let span = if self.filtered {
                total
            } else {
                self.need().min(total)
            };
            span.div_ceil(self.batch_size)
                .clamp(next, self.max_batches.max(next))
        })
    }
}

/// Load provider batches until `keep`-matching items cover the requested
/// public page, the provider is exhausted, or the batch cap is hit.
pub(super) async fn load_window(
    transport: &GitHubTransport,
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

/// Read the window in waves: each wave fetches, concurrently and in
/// provider order, the batches the known provider total says the window
/// spans (one batch when the total is unknown). Every batch a wave fetched
/// is kept; whether the page is covered is judged between waves.
pub(super) async fn load_window_with(
    transport: &GitHubTransport,
    segments: &[&str],
    mut spec: WindowSpec,
    keep: impl Fn(&Value) -> bool,
    extract: fn(&mut Value) -> Vec<Value>,
    context: &RequestContext,
) -> Result<Loaded, ProviderError> {
    let need = spec.need();
    let mut first_batch = spec.first_batch();
    loop {
        let mut loaded = Loaded {
            items: Vec::new(),
            state: WindowState {
                skipped: (first_batch - 1) * spec.batch_size,
                exhausted: false,
                capped: false,
            },
            first: Value::Null,
        };
        let mut matched = loaded.state.skipped;
        let mut next = first_batch;
        'waves: loop {
            let last = spec.wave_end(next);
            let held = spec.prefetched.take_if(|(batch, _)| *batch == next);
            let batches = fetch_batches(
                transport,
                segments,
                (next..=last, spec.batch_size),
                held,
                context,
            )
            .await?;
            for (batch, (mut value, more)) in (next..=last).zip(batches) {
                let items = extract(&mut value);
                if batch == first_batch {
                    loaded.first = value;
                }
                matched += items.iter().filter(|item| keep(item)).count();
                loaded.items.extend(items);
                if !more {
                    loaded.state.exhausted = true;
                    break 'waves;
                }
                if batch >= spec.max_batches {
                    loaded.state.capped = true;
                    loaded.state.exhausted = true;
                    break 'waves;
                }
            }
            if matched >= need {
                break;
            }
            next = last + 1;
        }
        // A jump past the real end (stale page cursor) restarts from batch 1
        // so a page past the true last page is flagged out of range.
        if loaded.items.is_empty() && first_batch > 1 {
            first_batch = 1;
            continue;
        }
        return Ok(loaded);
    }
}

/// Provider batches `batches` of `per_page` items in order,
/// [`BATCH_CONCURRENCY`] in flight; a `held` batch (the first of the range)
/// is used as is.
async fn fetch_batches(
    transport: &GitHubTransport,
    segments: &[&str],
    (batches, per_page): (std::ops::RangeInclusive<usize>, usize),
    held: Option<(usize, Batch)>,
    context: &RequestContext,
) -> Result<Vec<Batch>, ProviderError> {
    use futures_util::{StreamExt as _, TryStreamExt as _};
    let held = held.map(|(_, batch)| batch);
    let skip = usize::from(held.is_some());
    let fetched = futures_util::stream::iter(batches.skip(skip))
        .map(|batch| async move {
            fetch(
                transport,
                segments,
                &[
                    ("per_page", per_page.to_string()),
                    ("page", batch.to_string()),
                ],
                context,
            )
            .await
        })
        .buffered(BATCH_CONCURRENCY)
        .try_collect::<Vec<_>>()
        .await?;
    Ok(held.into_iter().chain(fetched).collect())
}
