//! One page of an issue's discussion comments, each body windowed, sized
//! to the response page.
use crate::gh_get_history_item::Source;
use crate::gh_get_history_item::axis::Axis;
use crate::gh_get_history_item::body_match::{Kept, humans_only, keeps};
use crate::gh_get_history_item::gh_get_history_item_query;
use crate::gh_get_history_item::remaining::{BodyWindows, Remainder};
use crate::gh_get_history_item::text::{delivered_earlier, shape_comment};
use crate::gh_get_history_item::util::{array, map_comments};
use crate::gh_get_history_item::window::{
    MAX_COLLECTION_BATCHES, WindowSpec, auto_page, load_window, page_starting_at_after,
};
use crate::gh_get_history_item::{DEFAULT_PAGE_SIZE, HistoryItemRequest, fetch};
use octocode_engine::num::saturating_usize;
use octocode_github::{MAX_PER_PAGE, ProviderError, RequestContext};
use octocode_runtime_base::response::pages::PageFacts;
use serde_json::{Map, Value, json};

/// Where an issue's comment page reads and what the row already holds.
pub(super) struct CommentRead<'a> {
    pub(super) issue_path: &'a [&'a str],
    /// Response characters the row's header and body already use.
    pub(super) used: usize,
    /// GitHub's count of the issue's comments.
    pub(super) total: Option<u64>,
}

impl CommentRead<'_> {
    /// The response chars the comments fill: what the header and body leave
    /// of the response page, less a reserve for the row's cursors and leads,
    /// so one comment page is one response page.
    fn budget(&self, query: &HistoryItemRequest) -> usize {
        auto_page(query.auto_page_chars)
            .saturating_sub(COMMENT_PAGE_RESERVE)
            .saturating_sub(self.used)
    }
}

/// One page of an issue's discussion comments.
pub(super) struct CommentPage {
    comments: Vec<Value>,
    /// The page object (`hasMore`, `nextPage`).
    pagination: Value,
    /// The page's comment-body windows.
    bodies: BodyWindows,
    /// The private `pageSize` that resumes at the first unshown
    /// comment ([`page_starting_at_after`]).
    resume_size: Option<usize>,
    /// The discussion's size: GitHub's count, or with a `matchString` the
    /// matching comments once the scan has listed them all.
    total: Option<u64>,
    /// What a `matchString` dropped of a first page, disclosed once.
    pub(super) dropped: Option<String>,
}

impl CommentPage {
    /// The comment page on `row` and its page facts. The provider comments
    /// after this page (its pages do not tile `page × pageSize`) stay on
    /// `rest`.
    pub(super) fn attach(
        self,
        row: &mut Value,
        pagination: &mut Map<String, Value>,
        remainder: &mut Remainder,
    ) {
        // GitHub's order: the timeline is evidence, and `bot` marks the
        // comments a reader may filter.
        if !self.comments.is_empty() {
            row["comments"] = Value::Array(self.comments);
        }
        let mut comments = self.pagination;
        comments["total"] = json!(self.total);
        if let Some(size) = self.resume_size {
            remainder.resume_page_size(size);
        }
        pagination.insert(Axis::Comments.key().into(), comments);
        self.bodies.attach(Axis::CommentBody, pagination);
    }
}

/// The saved comment page grid: the page starts at comment
/// `(page − 1) × pageSize`. The grid only places the page; how many comments
/// it shows is the response budget's call.
fn comment_grid(query: &HistoryItemRequest) -> usize {
    gh_get_history_item_query::page_size(query)
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .max(1)
}

/// The provider batch (of [`MAX_PER_PAGE`] comments) a comment page starts
/// in; `None` for page 0, which reads no comments.
pub(super) fn first_comment_batch(query: &HistoryItemRequest) -> Option<usize> {
    let page_no = gh_get_history_item_query::page(query).unwrap_or(1);
    (page_no > 0).then(|| page_no.saturating_sub(1) * comment_grid(query) / MAX_PER_PAGE + 1)
}

/// One provider batch of the issue's discussion comments.
pub(super) async fn comment_batch(
    source: &Source<'_>,
    issue_path: &[&str],
    batch: usize,
    context: &RequestContext,
) -> Result<(Value, bool), ProviderError> {
    fetch(
        source,
        &[issue_path, &["comments"]].concat(),
        &[
            ("per_page", MAX_PER_PAGE.to_string()),
            ("page", batch.to_string()),
        ],
        context,
    )
    .await
}

/// One page of an issue's discussion comments, each body windowed, filling
/// `budget` characters. The page starts at comment
/// `(page − 1) × pageSize` and shows up to one provider batch of the
/// comments that fit, read from at most two provider batches: the saved
/// `pageSize` places the page and never caps it, so a page after a short one
/// still fills the budget. The page resumes at its first unshown comment
/// through a private `page`/`pageSize` pair that starts there: the saved
/// `pageSize` when it does, else the widest grid that does with a later page
/// number ([`page_starting_at_after`]), so the public continuation carries only
/// its opaque cursor and page numbers only climb.
pub(super) async fn issue_comments(
    source: &Source<'_>,
    query: &HistoryItemRequest,
    context: &RequestContext,
    at: &CommentRead<'_>,
    mut prefetched: Option<Result<(Value, bool), ProviderError>>,
) -> Result<CommentPage, ProviderError> {
    let (issue_path, budget) = (at.issue_path, at.budget(query));
    let page_no = gh_get_history_item_query::page(query).unwrap_or(1);
    let grid = comment_grid(query);
    let start = page_no.saturating_sub(1) * grid;
    let cap = MAX_PER_PAGE;
    let mut raw_comments = Vec::new();
    let mut provider_more = false;
    let mut total = at.total;
    let mut dropped = None;
    let needle = query.matcher.as_deref();
    let humans = !gh_get_history_item_query::bots(query);
    if needle.is_some() || humans {
        // A `matchString` keeps the comments whose body holds it, and
        // `bots:false` those a person wrote: the kept ones lie anywhere, so
        // the scan reads the batches from the first, in concurrent waves,
        // until the page's are in.
        let spec = WindowSpec {
            batch_size: MAX_PER_PAGE,
            max_batches: MAX_COLLECTION_BATCHES,
            page: page_no,
            page_size: grid,
            filtered: true,
            provider_total: at.total.map(saturating_usize),
            prefetched: None,
        };
        let path = [issue_path, &["comments"]].concat();
        let loaded = load_window(
            source,
            &path,
            spec,
            |comment| keeps(needle, humans, comment),
            context,
        )
        .await?;
        let mut left_out = Vec::new();
        let people = humans_only(
            loaded.items,
            humans,
            loaded.state.exhausted,
            "comments",
            &mut left_out,
        );
        let kept = Kept::of(people, needle, loaded.state.exhausted);
        // The disclosure and the total state the list the page walks.
        left_out.extend(kept.warning("comments"));
        dropped = (!left_out.is_empty() && start == 0).then(|| left_out.join(" "));
        total = kept.exhausted.then_some(kept.entries.len() as u64);
        provider_more = !kept.exhausted;
        raw_comments = kept.entries.into_iter().skip(start).collect();
    } else if let Some(first) = first_comment_batch(query) {
        let mut skip = start % MAX_PER_PAGE;
        for batch in (first..).take(2) {
            let (value, more) = match prefetched.take() {
                Some(loaded) => loaded?,
                None => comment_batch(source, issue_path, batch, context).await?,
            };
            raw_comments.extend(array(value).into_iter().skip(skip));
            provider_more = more;
            if !more || raw_comments.len() >= cap {
                break;
            }
            skip = 0;
        }
    }
    let fetched = map_comments(raw_comments);
    // Comments that fit the budget and the cap; at least the first one.
    // Each comment costs its first window, so a body continuation
    // (`offset` > 0) pages exactly the comments its first window showed.
    let (mut consumed, mut used) = (0, 0usize);
    for comment in &fetched {
        let (first, _) = shape_comment(comment, None, query, &mut BodyWindows::default());
        let chars = octocode_engine::json_size::utf16_units(&first).unwrap_or(0) + 1;
        if consumed > 0 && (used + chars > budget || consumed >= cap) {
            break;
        }
        used += chars;
        consumed += 1;
    }
    let more = consumed < fetched.len() || provider_more;
    // The page's bodies at the call's offset. A continuation lists only the
    // bodies it continues: a body an earlier window delivered whole is not
    // repeated as an empty row.
    let mut bodies = BodyWindows::default();
    let comments = fetched[..consumed]
        .iter()
        .filter_map(|comment| {
            let (shaped, page) = shape_comment(
                comment,
                gh_get_history_item_query::char_offset(query),
                query,
                &mut bodies,
            );
            (!delivered_earlier(gh_get_history_item_query::char_offset(query), &page))
                .then_some(shaped)
        })
        .collect();
    // A page past the discussion's last is the shared out-of-range outcome:
    // its block states the discussion's size on the page grid it was asked on.
    let past_end = (consumed == 0 && !more)
        .then(|| total.and_then(|total| usize::try_from(total).ok()))
        .flatten()
        .map(|total| PageFacts::nth(page_no, grid, total))
        .filter(|facts| facts.is_past_end());
    let mut pagination = past_end
        .unwrap_or_else(|| PageFacts::open(start, consumed, more))
        .to_value();
    let mut resume_size = None;
    if more {
        let next = start + consumed;
        // The next page and the page size that places it (`nextPage` alone
        // under another size would land on other comments); the size rides
        // the continuation only, as the block describes the page shown.
        let (page, size) = if next.is_multiple_of(grid) {
            (next / grid + 1, grid)
        } else {
            page_starting_at_after(next, MAX_PER_PAGE, page_no)
        };
        pagination["nextPage"] = json!(page);
        resume_size = Some(size);
    }
    Ok(CommentPage {
        comments,
        pagination,
        bodies,
        resume_size,
        total,
        dropped,
    })
}

/// Response chars an issue comment page leaves to the row's cursors,
/// leads, and warnings.
const COMMENT_PAGE_RESERVE: usize = 2_500;
