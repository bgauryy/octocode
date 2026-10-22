//! `operation: "issue"`: issue metadata, body window, and one page of
//! discussion comments.
use super::axis::Axis;
use super::promotion::promote_issue_continuations;
use super::remaining::{BodyWindows, Remainder};
use super::text::{delivered_earlier, paginate_text, shape_comment};
use super::util::{array, map_comments, str_at, string};
use super::window::{auto_page, page_starting_at};
use super::{DEFAULT_PAGE_SIZE, HistoryItemRequest, Section, Sections, fetch, validation};
use crate::id::ToolId;
use crate::response::result::Confidence;
use crate::response::result::{Continuation, ToolData, remove_nulls};
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_shared::ensure_next;
use octocode_github::MAX_PER_PAGE;
use octocode_github::{
    GitHubTransport, ProviderError, ProviderErrorKind, ProviderErrorReason, RequestContext,
};
use serde_json::{Map, Value, json};

/// A pull-request read that 404ed: when the number is an issue, say so with
/// a typed reason (the caller offers the issue read); otherwise keep the
/// original not-found.
pub(super) async fn name_issue_number(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
    not_found: ProviderError,
) -> ProviderError {
    let Some(number) = gh_get_history_item_query::number(query).map(|number| number.to_string())
    else {
        return not_found;
    };
    let issue_path = [
        "repos",
        gh_get_history_item_query::owner(query),
        gh_get_history_item_query::repo(query),
        "issues",
        &number,
    ];
    match fetch(transport, &issue_path, &[], context).await {
        Ok((raw, _)) if raw.get("pull_request").is_none_or(Value::is_null) => {
            let mut error = ProviderError::new(
                ProviderErrorKind::NotFound,
                format!("#{number} is an issue, not a pull request"),
            )
            .with_reason(ProviderErrorReason::PullRequestIsIssue);
            error.status = not_found.status;
            error
        }
        _ => not_found,
    }
}

pub(super) async fn issue(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    let number = gh_get_history_item_query::number(query)
        .ok_or_else(|| validation("number is required"))?
        .to_string();
    let issue_path = [
        "repos",
        gh_get_history_item_query::owner(query),
        gh_get_history_item_query::repo(query),
        "issues",
        &number,
    ];
    // The closing PRs load beside the issue on its first window, which
    // states their bound once; later windows list no `closedBy`.
    let first_window = gh_get_history_item_query::page(query).unwrap_or(1) <= 1
        && gh_get_history_item_query::char_offset(query).is_none_or(|offset| offset == 0);
    // The discussion's first comment batch depends only on the page asked
    // for, so it loads beside the issue too (wasted only when the number
    // names a pull request).
    let first_batch = query
        .sections()
        .has(Section::Comments)
        .then(|| first_comment_batch(query))
        .flatten();
    let (fetched, closed_by, prefetched) = tokio::join!(
        fetch(transport, &issue_path, &[], context),
        async {
            if first_window {
                closing_pull_requests(transport, query, context).await
            } else {
                None
            }
        },
        async {
            match first_batch {
                Some(batch) => Some(comment_batch(transport, &issue_path, batch, context).await),
                None => None,
            }
        }
    );
    let (raw, _) = fetched?;
    if raw.get("pull_request").is_some_and(|v| !v.is_null()) {
        return Err(validation(&format!("Issue #{number} is a pull request"))
            .with_reason(ProviderErrorReason::IssueIsPullRequest));
    }
    // A summary (no section named) shows the body; a section read what it
    // names.
    let sections = query.sections();
    let want_body = sections.is_empty() || sections.has(Section::Body);
    let want_comments = sections.has(Section::Comments);
    // A comment-body hop (`offset` > 0 with the discussion) continues the
    // comment bodies of its first window. The issue body is not shown again
    // (its own `continueBody` reads it at its own offset), but its first
    // window still counts against the page, so the hop pages exactly the
    // comments that window showed.
    let comment_hop = want_comments
        && gh_get_history_item_query::char_offset(query).is_some_and(|offset| offset > 0);
    let show_body = want_body && !comment_hop;
    // A later comment page continues a read whose first page named the
    // issue; a section read's sections carry its evidence: either restates
    // only the number and state (the summary owns the rest).
    let later_page = gh_get_history_item_query::page(query).unwrap_or(1) > 1;
    let slim = later_page || gh_get_history_item_query::has_sections(query);
    let header = issue_header(&raw);
    let header_for = || {
        let mut row = header.clone();
        if slim && let Some(fields) = row.as_object_mut() {
            fields.retain(|key, _| key == "number" || key == "state");
        }
        row
    };
    let body_view = want_body.then(|| str_at(&raw, "/body").unwrap_or(""));
    let mut row = header_for();
    let mut pagination = Map::new();
    let mut remainder = Remainder::default();
    if let Some(body_view) = body_view.filter(|_| show_body) {
        let (body, page) = paginate_text(
            body_view,
            gh_get_history_item_query::char_offset(query),
            gh_get_history_item_query::char_length(query),
        );
        row["body"] = json!(body);
        if gh_get_history_item_query::char_offset(query).is_some()
            || gh_get_history_item_query::char_length(query).is_some()
            || page["hasMore"] == true
        {
            pagination.insert(Axis::Body.key().into(), page);
        }
    }
    if want_comments {
        // A comment-body hop counts the row its first window showed.
        let used = if comment_hop {
            let mut first = header_for();
            if let Some(body_view) = body_view {
                first["body"] = json!(
                    paginate_text(
                        body_view,
                        None,
                        gh_get_history_item_query::char_length(query)
                    )
                    .0
                );
            }
            octocode_engine::json_size::utf16_units(&first).unwrap_or(0)
        } else {
            octocode_engine::json_size::utf16_units(&row).unwrap_or(0)
        };
        let at = CommentRead {
            issue_path: &issue_path,
            used,
            total: raw.get("comments").and_then(Value::as_u64),
        };
        let page = issue_comments(transport, query, context, &at, prefetched).await?;
        page.attach(&at, &mut row, &mut pagination, &mut remainder);
    }
    if !pagination.is_empty() {
        row["contentPagination"] = Value::Object(pagination);
    }
    let closed = raw.get("state").and_then(Value::as_str) == Some("closed");
    if let Some(references) = closed_by.as_ref().filter(|refs| !refs.prs.is_empty()) {
        row["closedBy"] = Value::Array(references.prs.iter().map(|pr| pr.row.clone()).collect());
    }
    let mut out = json!({"issues":[row]});
    let remaining = promote_issue_continuations(&mut out, query, &remainder);
    if !want_comments && !later_page {
        attach_comments_read(&mut out, query, &raw);
    }
    let bounded = closed_by.as_ref().and_then(|references| {
        references
            .bounded_total
            .map(|total| (references.listed, total))
    });
    if let Some((listed, total)) = bounded {
        mark_bounded_closing(&mut out, listed, total);
    }
    if first_window {
        let prs = closed_by
            .as_ref()
            .map(|references| references.prs.as_slice());
        attach_fix_pr(&mut out, query, prs, closed, bounded.is_some());
    }
    Ok(ToolData {
        remaining,
        ..ToolData::from(out)
    })
}

/// The issue's identity and decision facts: number, title, state, author,
/// labels (when any), creation, the discussion's size, the last update of
/// an open issue, and the close date.
fn issue_header(raw: &Value) -> Value {
    json!({
        "number":raw["number"],"title":string(raw.get("title")),
        "state":str_at(raw,"/state").unwrap_or("open"),"author":str_at(raw,"/user/login").unwrap_or("unknown"),
        // Labels only when there are any.
        "labels":Some(raw.get("labels").and_then(Value::as_array).into_iter().flatten().filter_map(|v|str_at(v,"/name").map(str::to_owned)).collect::<Vec<_>>()).filter(|labels| !labels.is_empty()),
        "createdAt":string(raw.get("created_at")),
        // The discussion's size, so the caller can choose to read it.
        "commentsCount":super::util::nonzero(raw.get("comments")),
        // Only an open issue's last update says whether it is still moving.
        "updatedAt":(str_at(raw,"/state").unwrap_or("open") == "open").then(|| string(raw.get("updated_at"))),
        "closedAt":raw.get("closed_at").filter(|v|!v.is_null())
    })
}

/// Where an issue's comment page reads and what the row already holds.
struct CommentRead<'a> {
    issue_path: &'a [&'a str],
    /// Response characters the row's header and body already use.
    used: usize,
    /// GitHub's count of the issue's comments.
    total: Option<u64>,
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
struct CommentPage {
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
    fn attach(
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
fn first_comment_batch(query: &HistoryItemRequest) -> Option<usize> {
    let page_no = gh_get_history_item_query::page(query).unwrap_or(1);
    (page_no > 0).then(|| page_no.saturating_sub(1) * comment_grid(query) / MAX_PER_PAGE + 1)
}

/// One provider batch of the issue's discussion comments.
async fn comment_batch(
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
/// `(page − 1) × pageSize`; a first page shows at most an explicit
/// `pageSize`, and every page as many comments as fit (up to one provider
/// batch), read from at most two provider batches. The page resumes at its
/// first unshown comment through a `page`/`pageSize` pair that starts
/// there ([`page_starting_at`]), so the comment cursor is the response's only
/// cursor.
async fn issue_comments(
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
    let cap = match gh_get_history_item_query::page_size(query) {
        Some(size) if page_no <= 1 => size,
        _ => MAX_PER_PAGE,
    };
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
        let (page, size) = page_starting_at(start + consumed, MAX_PER_PAGE);
        // The next page and the page size that places it: `nextPage` alone
        // under another size would land on other comments.
        pagination["nextPage"] = json!(page);
        pagination["pageSize"] = json!(size);
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

/// `next.readDiscussion`: an issue read without its discussion offers it
/// when GitHub counts any comment (the same issue, `sections:["comments"]`).
fn attach_comments_read(out: &mut Value, query: &HistoryItemRequest, raw: &Value) {
    if super::util::nonzero(raw.get("comments")).is_none() {
        return;
    }
    let mut nq = super::pr_menu::base_public_query(query, super::ItemOperation::Issue);
    for key in ["offset", "length", "page"] {
        super::pr_menu::remove_key(&mut nq, key);
    }
    Sections::of([Section::Comments]).set_on(&mut nq);
    ensure_next(out);
    out["next"]["readDiscussion"] = super::pr_menu::continuation(nq);
}

/// The closing-reference set ends at a bound: no cursor continues it, and the
/// fix it names is a candidate among the listed references only.
fn mark_bounded_closing(out: &mut Value, listed: usize, total: u64) {
    crate::response::paging::terminal_limit(
        out,
        "closingReferenceLimit",
        &[format!(
            "closedBy lists {listed} of {total} linked pull requests; the first is a candidate, not the complete fix set."
        )],
    );
}

/// Closing references one issue read lists. Past it the set is bounded and
/// the response says so.
const MAX_CLOSING_REFERENCES: usize = 25;

/// The closing-reference lookup; `first` is [`MAX_CLOSING_REFERENCES`].
pub(super) const CLOSING_REFERENCES_DOCUMENT: &str = "query($owner:String!,$repo:String!,$number:Int!,$first:Int!){ repository(owner:$owner,name:$repo){ issue(number:$number){ closedByPullRequestsReferences(first:$first,includeClosedPrs:true){ totalCount pageInfo{ hasNextPage } nodes{ number state mergedAt additions deletions changedFiles } } } } }";

/// One linked pull request: its public `closedBy` row and whether its whole
/// diff fits one patch read.
struct ClosingPr {
    row: Value,
    small: bool,
}

/// The linked pull requests one read returned, merged first, how many the
/// first window lists, and GitHub's total when more exist than that.
struct ClosingReferences {
    prs: Vec<ClosingPr>,
    listed: usize,
    bounded_total: Option<u64>,
}

/// The pull requests whose merge closes (or closed) the issue, merged first:
/// `{number, state, mergedAt?}`. `None` when GraphQL is unavailable or
/// failed (the caller falls back to search); empty when none are linked.
async fn closing_pull_requests(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Option<ClosingReferences> {
    let references = closed_by_references(transport, query, context).await?;
    let more = references
        .pointer("/pageInfo/hasNextPage")
        .and_then(Value::as_bool)
        == Some(true);
    let prs = map_closing_pull_requests(references.get("nodes")?.as_array()?);
    let listed = u64::try_from(prs.len()).unwrap_or(u64::MAX);
    Some(ClosingReferences {
        bounded_total: more.then(|| {
            references
                .get("totalCount")
                .and_then(Value::as_u64)
                .map_or(listed, |total| total.max(listed))
        }),
        listed: prs.len(),
        prs,
    })
}

/// The issue's first [`MAX_CLOSING_REFERENCES`]
/// `closedByPullRequestsReferences`. `None` when GraphQL is unavailable or
/// failed.
async fn closed_by_references(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Option<Value> {
    if !transport.graphql_enabled || !transport.graphql_available(context) {
        return None;
    }
    let variables = json!({
        "owner": gh_get_history_item_query::owner(query),
        "repo": gh_get_history_item_query::repo(query),
        "number": gh_get_history_item_query::number(query)?,
        "first": MAX_CLOSING_REFERENCES,
    });
    let mut page = transport
        .execute_graphql(CLOSING_REFERENCES_DOCUMENT, variables, context)
        .await
        .ok()?;
    page.data
        .as_mut()?
        .pointer_mut("/repository/issue/closedByPullRequestsReferences")
        .map(Value::take)
}

fn map_closing_pull_requests(nodes: &[Value]) -> Vec<ClosingPr> {
    let mut prs = nodes
        .iter()
        .filter_map(|node| {
            let number = node.get("number").and_then(Value::as_u64)?;
            let mut row = json!({
                "number": number,
                "state": str_at(node, "/state").unwrap_or("closed").to_ascii_lowercase(),
                "mergedAt": node.get("mergedAt").filter(|v| !v.is_null()),
            });
            remove_nulls(&mut row);
            let count = |key: &str| node.get(key).and_then(Value::as_u64);
            let small = super::pr_menu::is_small_pr(
                count("additions"),
                count("deletions"),
                count("changedFiles"),
            );
            Some(ClosingPr { row, small })
        })
        .collect::<Vec<_>>();
    // Stable: merged first, otherwise GitHub's order.
    prs.sort_by_key(|pr| pr.row.get("mergedAt").is_none());
    prs
}

/// `next.readPullRequest` reads the first linked (merged-first) pull request;
/// with no link information a closed issue offers the keyword search hop.
fn attach_fix_pr(
    out: &mut Value,
    query: &HistoryItemRequest,
    closed_by: Option<&[ClosingPr]>,
    closed: bool,
    bounded: bool,
) {
    let confidence = if bounded {
        Confidence::Medium
    } else {
        Confidence::High
    };
    let next = match closed_by.and_then(<[ClosingPr]>::first) {
        // A small fix reads whole in one call: its patches are the evidence
        // (`closedBy` already links it to this issue). A larger one names
        // its files, beside its description, so the review can pick patches.
        // Files the caller's goal names: their patches are the evidence, and
        // the filter rides the copied query, so the narrowing is visible.
        Some(pr) => {
            let named = goal_file_globs(query);
            let mut read = json!({
                "operation":"pullRequest","owner":gh_get_history_item_query::owner(query),"repo":gh_get_history_item_query::repo(query),
                "number":pr.row["number"],
                "sections": if pr.small || !named.is_empty() { json!(["patches"]) } else { json!(["body", "files"]) }});
            if !named.is_empty() {
                read["include"] = json!(named);
            }
            (
                "readPullRequest",
                Continuation::new(ToolId::GhGetHistoryItem, read)
                    .confidence(confidence)
                    .build(),
            )
        }
        None if closed && closed_by.is_none() => (
            "findPullRequest",
            Continuation::new(
                ToolId::GhSearchHistory,
                json!({"operation":"pullRequest","owner":gh_get_history_item_query::owner(query),"repo":gh_get_history_item_query::repo(query),
                    "keywords":[gh_get_history_item_query::number(query).unwrap_or_default().to_string()]}),
            )
            .confidence(Confidence::Medium)
            .build(),
        ),
        None => return,
    };
    ensure_next(out);
    // The fix is new evidence: it leads the issue's own reads, so a
    // one-lead answer offers it.
    if let Some(leads) = out["next"].as_object_mut() {
        leads.shift_insert(0, next.0.to_owned(), next.1);
    }
}

/// Most goal-named files one fix-PR read narrows to.
const MAX_GOAL_FILES: usize = 5;

/// `include` globs for file names the query's goal mentions
/// (`merge.go`, `pkg/cmd/merge.go`): a word counts only when its name has a
/// known code, config, doc, or lock file type, so dotted identifiers
/// (`res.redirect`) and versions never narrow a read.
fn goal_file_globs(query: &HistoryItemRequest) -> Vec<String> {
    use crate::content::classify_file_type;
    let goal = query
        .public
        .get("mainGoal")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut globs = Vec::new();
    for word in goal.split(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'))) {
        let path = word.trim_matches(|c| c == '.' || c == '/');
        let name = path.rsplit('/').next().unwrap_or(path);
        let has_stem = name
            .rsplit_once('.')
            .is_some_and(|(stem, _)| stem.chars().any(char::is_alphanumeric));
        if !has_stem || classify_file_type(name).is_none() {
            continue;
        }
        let glob = format!("**/{path}");
        if !globs.contains(&glob) {
            globs.push(glob);
        }
        if globs.len() == MAX_GOAL_FILES {
            break;
        }
    }
    globs
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small merged fix is read with its patches in the same call; a large
    /// one (or one GitHub sized nothing for) reads its file inventory first.
    #[test]
    fn read_fix_pr_carries_the_patches_of_a_small_fix() {
        let query = HistoryItemRequest::from_row(json!({
            "operation":"issue","mainGoal":"g","reasoning":"r","owner":"o","repo":"r","number":1
        }))
        .expect("issue query");
        let prs = map_closing_pull_requests(&[
            json!({"number":7,"state":"CLOSED","mergedAt":null,"additions":1,"deletions":0,"changedFiles":1}),
            json!({"number":8,"state":"MERGED","mergedAt":"2026-09-11T15:55:40Z",
                "additions":63,"deletions":4,"changedFiles":3}),
        ]);
        assert_eq!(
            prs.iter().map(|pr| pr.row.clone()).collect::<Vec<_>>(),
            [
                json!({"number":8,"state":"merged","mergedAt":"2026-09-11T15:55:40Z"}),
                json!({"number":7,"state":"closed"}),
            ]
        );
        let mut out = json!({});
        attach_fix_pr(&mut out, &query, Some(&prs), true, false);
        assert_eq!(
            out["next"]["readPullRequest"]["query"]["queries"][0]["number"],
            8
        );
        assert_eq!(
            out["next"]["readPullRequest"]["query"]["queries"][0]["sections"],
            json!(["patches"])
        );
        for node in [
            json!({"number":9,"state":"MERGED","mergedAt":"x","additions":900,"deletions":4,"changedFiles":3}),
            json!({"number":9,"state":"MERGED","mergedAt":"x"}),
        ] {
            let mut out = json!({});
            attach_fix_pr(
                &mut out,
                &query,
                Some(&map_closing_pull_requests(&[node])),
                true,
                false,
            );
            assert_eq!(
                out["next"]["readPullRequest"]["query"]["queries"][0]["sections"],
                json!(["body", "files"])
            );
        }
        assert!(
            out_for(&query, &prs)["next"]["readPullRequest"]["query"]["queries"][0]
                .get("include")
                .is_none()
        );
    }

    fn out_for(query: &HistoryItemRequest, prs: &[ClosingPr]) -> Value {
        let mut out = json!({});
        attach_fix_pr(&mut out, query, Some(prs), true, false);
        out
    }

    /// A goal that names changed files reads only those files' patches of
    /// the fix; dotted words that are not file names (`res.redirect`,
    /// versions) never narrow it.
    #[test]
    fn read_fix_pr_narrows_to_files_the_goal_names() {
        let issue = |goal: &str| {
            HistoryItemRequest::from_row(json!({
                "operation":"issue","mainGoal":goal,"reasoning":"r","owner":"o","repo":"r","number":1
            }))
            .expect("issue query")
        };
        let large = map_closing_pull_requests(&[
            json!({"number":9,"state":"MERGED","mergedAt":"x","additions":900,"deletions":4,"changedFiles":40}),
        ]);
        let out = out_for(
            &issue("Which PR fixed cli/cli#14404 and what did it change in merge.go?"),
            &large,
        );
        let read = &out["next"]["readPullRequest"]["query"]["queries"][0];
        assert_eq!(read["include"], json!(["**/merge.go"]), "{read}");
        assert_eq!(read["sections"], json!(["patches"]), "{read}");
        let out = out_for(
            &issue("Check pkg/cmd/pr/merge/merge.go and README.md (v2.31.0)."),
            &large,
        );
        assert_eq!(
            out["next"]["readPullRequest"]["query"]["queries"][0]["include"],
            json!(["**/pkg/cmd/pr/merge/merge.go", "**/README.md"])
        );
        for goal in [
            "What does res.redirect default to in express@4.21.2?",
            "Which PR fixed this issue?",
        ] {
            let read =
                &out_for(&issue(goal), &large)["next"]["readPullRequest"]["query"]["queries"][0];
            assert!(read.get("include").is_none(), "{goal}: {read}");
            assert_eq!(read["sections"], json!(["body", "files"]), "{goal}");
        }
    }
}
