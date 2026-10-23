//! One page of an issue's discussion comments, each body windowed, sized
//! to the response page.
use crate::tools::gh_get_history_item::axis::Axis;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_get_history_item::remaining::{BodyWindows, Remainder};
use crate::tools::gh_get_history_item::text::{delivered_earlier, shape_comment};
use crate::tools::gh_get_history_item::util::{array, map_comments};
use crate::tools::gh_get_history_item::window::{auto_page, page_starting_at};
use crate::tools::gh_get_history_item::{DEFAULT_PAGE_SIZE, HistoryItemRequest, fetch};
use octocode_github::{GitHubTransport, MAX_PER_PAGE, ProviderError, RequestContext};
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
    /// Provider comments read up to the end of this page.
    consumed: usize,
    /// The `pageSize` that resumes the next page at its first unshown
    /// comment ([`page_starting_at`]).
    resume_size: Option<usize>,
}

impl CommentPage {
    /// The comment page on `row` and its page facts. `totalItems` is the
    /// issue's real comment count; the provider comments after this page
    /// (its pages do not tile `currentPage × pageSize`) stay on `rest`.
    pub(super) fn attach(
        self,
        at: &CommentRead<'_>,
        row: &mut Value,
        pagination: &mut Map<String, Value>,
        remainder: &mut Remainder,
    ) {
        if !self.comments.is_empty() {
            row["comments"] = Value::Array(self.comments);
        }
        let mut comments = self.pagination;
        comments["totalItems"] = json!(at.total);
        if comments["hasMore"] == true
            && let Some(total) = at.total
        {
            let left = total.saturating_sub(u64::try_from(self.consumed).unwrap_or(u64::MAX));
            if left > 0 {
                remainder.items(Axis::Comments, left);
            }
        }
        if let Some(size) = self.resume_size {
            remainder.resume_page_size(size);
        }
        pagination.insert(Axis::Comments.key().into(), comments);
        remainder.bodies(Axis::CommentBody, self.bodies, pagination);
    }
}

/// The comment page grid: `pageSize` comments per page.
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
    transport: &GitHubTransport,
    issue_path: &[&str],
    batch: usize,
    context: &RequestContext,
) -> Result<(Value, bool), ProviderError> {
    fetch(
        transport,
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
/// `(page − 1) × pageSize` and shows at most an explicit `pageSize` (else up
/// to one provider batch) of the comments that fit, read from at most two
/// provider batches. The page resumes at its first unshown comment through a
/// `page`/`pageSize` pair that starts there: the caller's `pageSize` while
/// pages fill it, else the widest grid that does ([`page_starting_at`]), so
/// the comment cursor is the response's only cursor.
pub(super) async fn issue_comments(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
    at: &CommentRead<'_>,
    mut prefetched: Option<Result<(Value, bool), ProviderError>>,
) -> Result<CommentPage, ProviderError> {
    let (issue_path, budget) = (at.issue_path, at.budget(query));
    let page_no = gh_get_history_item_query::page(query).unwrap_or(1);
    let grid = comment_grid(query);
    let start = page_no.saturating_sub(1) * grid;
    let explicit = gh_get_history_item_query::page_size(query).is_some();
    let cap = if explicit { grid } else { MAX_PER_PAGE };
    let mut raw_comments = Vec::new();
    let mut provider_more = false;
    if let Some(first) = first_comment_batch(query) {
        let mut skip = start % MAX_PER_PAGE;
        for batch in (first..).take(2) {
            let (value, more) = match prefetched.take() {
                Some(loaded) => loaded?,
                None => comment_batch(transport, issue_path, batch, context).await?,
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
    // its block states `page N of M` on the page grid it was asked on.
    let past_end = (consumed == 0 && !more)
        .then(|| at.total.and_then(|total| usize::try_from(total).ok()))
        .flatten()
        .map(|total| crate::response::pages::PageFacts::counted(page_no, grid, total))
        .filter(|facts| facts.past_end().is_some());
    let mut pagination = past_end.map_or_else(
        || json!({"hasMore": more}),
        |facts| facts.out_of_range(true).to_value(),
    );
    let mut resume_size = None;
    if more {
        let next = start + consumed;
        // The next page and the page size that places it (`nextPage` alone
        // under another size would land on other comments); the size rides
        // the continuation only, as the block describes the page shown.
        let (page, size) = if explicit && next.is_multiple_of(grid) {
            (next / grid + 1, grid)
        } else {
            page_starting_at(next, MAX_PER_PAGE)
        };
        pagination["nextPage"] = json!(page);
        resume_size = Some(size);
    }
    Ok(CommentPage {
        comments,
        pagination,
        bodies,
        consumed: start + consumed,
        resume_size,
    })
}

/// Response chars an issue comment page leaves to the row's cursors,
/// leads, and warnings.
const COMMENT_PAGE_RESERVE: usize = 2_500;
