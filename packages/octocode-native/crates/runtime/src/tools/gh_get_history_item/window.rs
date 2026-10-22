//! Public-page windows over GitHub REST collections: provider batch loading
//! derived from public page cursors, and the page objects reported back.
use super::inventory::max_inventory_page;
use super::{DEFAULT_PAGE_SIZE, HistoryItemRequest, fetch};
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use octocode_github::MAX_PER_PAGE;
use octocode_github::{GitHubTransport, ProviderError, RequestContext};
use serde_json::{Value, json};

// GitHub's collection caps, in one table: provider facts, not tuning.

/// GitHub's compare endpoint lists at most this many changed files.
pub(super) const COMPARE_FILE_LIMIT: usize = 300;
/// PR and commit file lists stop at 3000 files (30 batches of 100).
pub(super) const MAX_FILE_BATCHES: usize = 30;
/// Comments/reviews scanned for one page (3000 items).
pub(super) const MAX_COLLECTION_BATCHES: usize = 30;
/// GitHub lists at most 250 commits for a pull request.
pub(super) const MAX_PR_COMMIT_BATCHES: usize = 3;

/// The `(page, pageSize)` pair whose page starts exactly at item `start`
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

/// The public page a load must cover, read in `MAX_PER_PAGE`-sized batches.
pub(super) struct WindowSpec {
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
            (self.start() / MAX_PER_PAGE).min(self.max_batches.saturating_sub(1)) + 1
        }
    }

    /// Whether the page may need more than its first provider batch: a
    /// filtered scan (its matches lie anywhere), or a page whose items cross
    /// a batch boundary.
    pub(super) fn spans_batches(&self) -> bool {
        self.filtered || self.need().div_ceil(MAX_PER_PAGE) > self.first_batch()
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
            span.div_ceil(MAX_PER_PAGE)
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
                skipped: (first_batch - 1) * MAX_PER_PAGE,
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
            let batches = fetch_batches(transport, segments, next..=last, held, context).await?;
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

/// Provider batches `batches` in order, [`BATCH_CONCURRENCY`] in flight;
/// a `held` batch (the first of the range) is used as is.
async fn fetch_batches(
    transport: &GitHubTransport,
    segments: &[&str],
    batches: std::ops::RangeInclusive<usize>,
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
                    ("per_page", MAX_PER_PAGE.to_string()),
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

/// A provider list that ended before an authoritative item count is partial,
/// even when its last (short) batch carries no next link.
pub(super) fn reconcile_collection_total(
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
pub(super) fn reconcile_file_totals(
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
        let current = page["currentPage"].as_u64().unwrap_or(1) as usize;
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
pub(super) fn paginate_window<T>(
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
    let pages = loaded.div_ceil(per).max(1);
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
pub(super) fn lift_page_limits(row: &mut Value, page: &mut Value) {
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

/// Patch characters one call carries across a page's files by default,
/// derived from the effective automatic response page (the runtime's
/// `auto_page_chars`, or a call's `responseLength`). Rendered text prints
/// patches verbatim (no escaping), so the default window takes 4/5 of the
/// page less a fixed reserve for the row header and metadata, never below
/// 2/5 of it: a default window plus row metadata fits one response page.
/// An explicit `length` is honoured up to that one-row budget: a larger
/// window would only be re-sliced into response `rowPart`s, more calls for
/// the same patch text. A row that still outgrows its page is lossless:
/// automatic paging splits it by whole rows, and the walk's
/// `next.continuePatch`/`nextFilePage` rides the row's last part
/// (`id::resumes_after_shown`), so following it never skips a part.
/// A bigger window comes with a bigger response page (`responseLength`).
pub(super) const PATCH_DEFAULT_SHARE: (usize, usize) = (4, 5);

pub(super) const PATCH_DEFAULT_RESERVE: usize = 5_000;

/// The page a call carries: the runtime's, else (direct callers, tests) the
/// constant automatic page.
pub(super) fn auto_page(auto_page: Option<usize>) -> usize {
    auto_page
        .filter(|page| *page > 0)
        .unwrap_or(crate::execution::RuntimeLimits::DEFAULT.auto_page_chars)
}

/// The default window of a `matchString` view: the page share without the
/// fixed reserve.
pub(super) fn literal_patch_window(auto_page_chars: Option<usize>) -> usize {
    let page = auto_page(auto_page_chars);
    let (num, den) = PATCH_DEFAULT_SHARE;
    (page * num / den).max(1)
}

/// The warning of a patch read whose explicit `length` was clamped to
/// one response page; `None` when it fit.
pub(super) fn clamp_warning(query: &HistoryItemRequest) -> Option<String> {
    let length = gh_get_history_item_query::char_length(query)?;
    let window = patch_window(Some(length), query.auto_page_chars);
    (window < length).then(|| {
        format!("length {length} exceeds one response page; patch windows hold {window} chars.")
    })
}

/// The patch window of one row: most of its response page. An explicit
/// `length` is clamped to that budget, so one row's window always fits one
/// response page.
pub(super) fn patch_window(char_length: Option<usize>, auto_page_chars: Option<usize>) -> usize {
    let page = auto_page(auto_page_chars);
    let (num, den) = PATCH_DEFAULT_SHARE;
    let budget = (page * num / den)
        .min(page.saturating_sub(PATCH_DEFAULT_RESERVE))
        .max(page * 2 / 5)
        .max(1);
    char_length.map_or(budget, |length| length.clamp(1, budget))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::gh_shared::test_support::{default_context, one_shot_provider};
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Mount provider batch `page` of `/files` (100 items each, `total` in
    /// all), answering after `delay` and expecting `reads` requests.
    async fn mount_batch(server: &MockServer, page: usize, total: usize, reads: u64) {
        let items = ((page - 1) * MAX_PER_PAGE..(page * MAX_PER_PAGE).min(total))
            .map(|n| json!({"filename": format!("f{n}")}))
            .collect::<Vec<_>>();
        let mut answer = ResponseTemplate::new(200)
            .set_delay(std::time::Duration::from_millis(300))
            .set_body_json(items);
        if page * MAX_PER_PAGE < total {
            answer = answer.insert_header(
                "link",
                format!(
                    "<{}/api/v3/files?page={}>; rel=\"next\"",
                    server.uri(),
                    page + 1
                ),
            );
        }
        Mock::given(method("GET"))
            .and(path("/api/v3/files"))
            .and(query_param("page", page.to_string()))
            .respond_with(answer)
            .expect(reads)
            .mount(server)
            .await;
    }

    fn spec(page_size: usize, filtered: bool, total: Option<usize>) -> WindowSpec {
        WindowSpec {
            max_batches: MAX_FILE_BATCHES,
            page: 1,
            page_size,
            filtered,
            provider_total: total,
            prefetched: None,
        }
    }

    /// An unfiltered page whose files span several provider batches, with
    /// the provider total known, reads those batches at once and in order:
    /// three 300 ms batches take about one batch's time, not three.
    #[tokio::test]
    async fn a_page_spanning_batches_reads_them_at_once_in_order() {
        let server = MockServer::builder().start().await;
        for page in 1..=3 {
            mount_batch(&server, page, 1000, 1).await;
        }
        let provider = one_shot_provider(&server);
        let started = std::time::Instant::now();
        let loaded = load_window(
            &provider.transport,
            &["files"],
            spec(250, false, Some(1000)),
            |_| true,
            &default_context(),
        )
        .await
        .expect("window");
        assert!(
            started.elapsed() < std::time::Duration::from_millis(800),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(loaded.items.len(), 300);
        let names = loaded
            .items
            .iter()
            .map(|item| item["filename"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(names, (0..300).map(|n| format!("f{n}")).collect::<Vec<_>>());
        assert!(!loaded.state.exhausted);
    }

    /// A batch the read already holds (fetched beside the PR) is used as
    /// is: a filtered scan of 250 files reads only batches 2 and 3, and
    /// every file lands once, in provider order.
    #[tokio::test]
    async fn a_prefetched_batch_is_never_read_again() {
        let server = MockServer::builder().start().await;
        mount_batch(&server, 1, 250, 0).await;
        mount_batch(&server, 2, 250, 1).await;
        mount_batch(&server, 3, 250, 1).await;
        let provider = one_shot_provider(&server);
        let first = (0..100)
            .map(|n| json!({"filename": format!("f{n}")}))
            .collect::<Vec<_>>();
        let loaded = load_window(
            &provider.transport,
            &["files"],
            WindowSpec {
                prefetched: Some((1, (json!(first), true))),
                ..spec(100, true, Some(250))
            },
            |_| true,
            &default_context(),
        )
        .await
        .expect("window");
        assert_eq!(loaded.items.len(), 250);
        assert_eq!(loaded.items[0]["filename"], "f0");
        assert_eq!(loaded.items[249]["filename"], "f249");
        assert!(loaded.state.exhausted && !loaded.state.capped);
    }

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

    #[test]
    fn a_page_past_an_exhausted_window_is_empty_and_flagged_not_clamped() {
        let items = (0..10).map(|n| json!(n)).collect::<Vec<_>>();
        let (slice, page) = paginate_window(items, 0, true, Some(4), Some(5));
        assert!(slice.is_empty(), "must not repeat the last page: {slice:?}");
        assert_eq!(page["currentPage"], 4);
        assert_eq!(page["totalPages"], 2);
        assert_eq!(page["hasMore"], false);
        assert_eq!(page["outOfRange"], true);
    }
}
