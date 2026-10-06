//! `operation: "issue"`: issue metadata, body window, and one page of
//! discussion comments.
use super::patch::auto_page;
use super::promotion::promote_issue_continuations;
use super::util::{
    array, content_flag, history_body_view, map_comments, merge, paginate_text, str_at, string,
    window_body,
};
use super::{DEFAULT_PAGE_SIZE, HistoryItemRequest, MAX_COLLECTION_PAGE, fetch, validation};
use crate::providers::github::{
    CredentialResolver, GitHubTransport, ProviderError, ProviderErrorKind, ProviderErrorReason,
    RequestContext,
};
use crate::tools::id::ToolId;
use crate::tools::result::{Continuation, remove_nulls};
use serde_json::{Map, Value, json};

/// A pull-request read that 404ed: when the number is an issue, say so with
/// a typed reason (the caller offers the issue read); otherwise keep the
/// original not-found.
pub(super) async fn name_issue_number<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
    not_found: ProviderError,
) -> ProviderError {
    let Some(number) = query.number().map(|number| number.to_string()) else {
        return not_found;
    };
    let issue_path = ["repos", query.owner(), query.repo(), "issues", &number];
    match fetch(transport, &issue_path, &[], context).await {
        Ok((raw, _)) if raw.get("pull_request").is_none_or(Value::is_null) => {
            let mut error = ProviderError::new(
                ProviderErrorKind::NotFound,
                format!(
                    "#{number} is an issue, not a pull request; read it with operation:\"issue\"."
                ),
            )
            .with_reason(ProviderErrorReason::PullRequestIsIssue);
            error.status = not_found.status;
            error
        }
        _ => not_found,
    }
}

pub(super) async fn issue<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<Value, ProviderError> {
    let number = query
        .number()
        .ok_or_else(|| validation("number is required"))?
        .to_string();
    let issue_path = ["repos", query.owner(), query.repo(), "issues", &number];
    // The closing PRs load beside the issue on its first window; later
    // windows ask only for their count so the bounded set stays disclosed.
    let first_window = query.comment_page().unwrap_or(1) <= 1
        && query.char_offset().is_none_or(|offset| offset == 0);
    let (fetched, closed_by) = tokio::join!(fetch(transport, &issue_path, &[], context), async {
        if first_window {
            closing_pull_requests(transport, query, context).await
        } else {
            closing_reference_count(transport, query, context).await
        }
    });
    let (raw, _) = fetched?;
    if raw.get("pull_request").is_some_and(|v| !v.is_null()) {
        return Err(validation(&format!(
            "Issue #{number} is a pull request; use ghGetHistoryItem operation:\"pullRequest\" with number:{number}."
        ))
        .with_reason(ProviderErrorReason::IssueIsPullRequest));
    }
    let content_value = query.content_value();
    let content = content_value.as_ref().and_then(Value::as_object);
    let want_body = content.is_none_or(|v| content_flag(Some(v), "body"));
    let comments = content
        .and_then(|v| v.get("comments"))
        .and_then(Value::as_object);
    let want_comments = content_flag(comments, "discussion");
    let include_bots = content_flag(comments, "includeBots");
    // A later comment page continues a read whose first page named the
    // issue: only the number is restated.
    let later_page = query.comment_page().unwrap_or(1) > 1;
    let mut row = json!({
        "number":raw["number"],"title":string(raw.get("title")),
        "state":str_at(&raw,"/state").unwrap_or("open"),"author":str_at(&raw,"/user/login").unwrap_or("unknown"),
        "labels":raw.get("labels").and_then(Value::as_array).into_iter().flatten().filter_map(|v|str_at(v,"/name").map(str::to_owned)).collect::<Vec<_>>(),
        "createdAt":string(raw.get("created_at")),
        // The discussion's size, so the caller can choose to read it.
        "commentsCount":super::util::nonzero(raw.get("comments")),
        // Only an open issue's last update says whether it is still moving.
        "updatedAt":(str_at(&raw,"/state").unwrap_or("open") == "open").then(|| string(raw.get("updated_at"))),
        "closedAt":raw.get("closed_at").filter(|v|!v.is_null())
    });
    if later_page
        && !want_body
        && let Some(fields) = row.as_object_mut()
    {
        fields.retain(|key, _| key == "number");
    }
    let mut pagination = Map::new();
    if want_body {
        let body_view = history_body_view(str_at(&raw, "/body").unwrap_or(""), query);
        let (body, page) = paginate_text(&body_view, query.char_offset(), query.char_length());
        row["body"] = json!(body);
        if query.char_offset().is_some() || query.char_length().is_some() || page["hasMore"] == true
        {
            pagination.insert("body".into(), page);
        }
    }
    let mut bots_hidden = 0;
    if want_comments {
        // The comments fill what the header and body leave of the response
        // page (less a reserve for the row's cursors and leads), so one
        // comment page is one response page.
        let used = crate::tools::stream_page::json_chars(&row);
        let budget = auto_page(query.auto_page_chars)
            .saturating_sub(COMMENT_PAGE_RESERVE)
            .saturating_sub(used);
        let page =
            issue_comments(transport, query, context, &issue_path, include_bots, budget).await?;
        bots_hidden = page.bots_hidden;
        if !page.comments.is_empty() {
            row["comments"] = Value::Array(page.comments);
        }
        // `raw.comments` is the issue's real comment count (bots included);
        // the page count is `returnedComments`, and `remainingItems` counts
        // the provider comments after this page.
        let mut comments = page.pagination;
        let total = raw.get("comments").and_then(Value::as_u64);
        comments["totalItems"] = json!(total);
        comments["returnedComments"] = json!(row["comments"].as_array().map_or(0, Vec::len));
        if comments["hasMore"] == true
            && let Some(total) = total
        {
            let left = total.saturating_sub(u64::try_from(page.consumed).unwrap_or(u64::MAX));
            if left > 0 {
                comments["remainingItems"] = json!(left);
            }
        }
        pagination.insert("comments".into(), comments);
        if let Some(body) = page.body {
            pagination.insert("commentBody".into(), body);
        }
    }
    if !pagination.is_empty() {
        row["contentPagination"] = Value::Object(pagination);
    }
    let closed = raw.get("state").and_then(Value::as_str) == Some("closed");
    if let Some(references) = closed_by.as_ref().filter(|refs| !refs.prs.is_empty()) {
        row["closedBy"] = Value::Array(references.prs.iter().map(|pr| pr.row.clone()).collect());
    }
    let mut out = json!({"owner":query.owner(),"repo":query.repo(),"issues":[row]});
    promote_issue_continuations(&mut out, query);
    if !want_comments && !later_page {
        attach_comments_read(&mut out, query, &raw);
    }
    if bots_hidden > 0 {
        attach_bot_read(&mut out, query, bots_hidden);
    }
    let bounded = closed_by.as_ref().and_then(|references| {
        references
            .bounded_total
            .map(|total| (references.listed, total))
    });
    if let Some((listed, total)) = bounded {
        mark_bounded_closing(&mut out, listed, total, first_window);
    }
    if first_window {
        let prs = closed_by
            .as_ref()
            .map(|references| references.prs.as_slice());
        attach_fix_pr(&mut out, query, prs, closed, bounded.is_some());
    }
    Ok(out)
}

/// One page of an issue's discussion comments.
struct CommentPage {
    comments: Vec<Value>,
    /// The page object (`hasMore`, `nextPage`/`nextPageSize`, `botsHidden`).
    pagination: Value,
    /// The first comment-body window with more text.
    body: Option<Value>,
    /// Provider comments read up to the end of this page (shown and hidden).
    consumed: usize,
    /// Bot comments this page skipped (`includeBots` omitted).
    bots_hidden: usize,
}

/// One page of an issue's discussion comments, each body windowed, filling
/// `budget` characters. The page starts at comment
/// `(commentPage − 1) × pageSize`; a first page shows at most an explicit
/// `pageSize`, and every page as many comments as fit (up to one provider
/// batch), read from at most two provider batches. The page resumes at its
/// first unshown comment through a `commentPage`/`pageSize` pair that starts
/// there ([`resume_page`]), so the comment cursor is the response's only
/// cursor.
async fn issue_comments<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
    issue_path: &[&str],
    include_bots: bool,
    budget: usize,
) -> Result<CommentPage, ProviderError> {
    let page_no = query.comment_page().unwrap_or(1);
    let grid = query.page_size().unwrap_or(DEFAULT_PAGE_SIZE).max(1);
    let start = page_no.saturating_sub(1) * grid;
    let cap = match query.page_size() {
        Some(size) if page_no <= 1 => size,
        _ => MAX_COLLECTION_PAGE,
    };
    let mut raw_comments = Vec::new();
    let mut provider_more = false;
    if page_no > 0 {
        let mut skip = start % MAX_COLLECTION_PAGE;
        for batch in (start / MAX_COLLECTION_PAGE + 1..).take(2) {
            let (value, more) = fetch(
                transport,
                &[issue_path, &["comments"]].concat(),
                &[
                    ("per_page", MAX_COLLECTION_PAGE.to_string()),
                    ("page", batch.to_string()),
                ],
                context,
            )
            .await?;
            raw_comments.extend(array(value).into_iter().skip(skip));
            provider_more = more;
            if !more || raw_comments.len() >= cap {
                break;
            }
            skip = 0;
        }
    }
    // Each provider comment (a hidden bot's is `None`), shaped, in order.
    let mut body_page = None;
    let fetched: Vec<Option<Value>> = raw_comments
        .into_iter()
        .map(|raw| {
            let comment = map_comments(vec![raw], "discussion", include_bots).pop()?;
            let (body, page) = window_body(
                str_at(&comment, "/body").unwrap_or(""),
                query.char_offset(),
                query,
                &mut body_page,
                // Issue text is never minified.
                &mut false,
            );
            let mut c = merge(comment, json!({"body":body}));
            // A whole body's window restates its length only.
            if page["offset"] != 0 || page["hasMore"] == true {
                c["bodyPagination"] = page;
            }
            if let Some(map) = c.as_object_mut() {
                if let Some(author) = map.remove("author") {
                    map.insert("user".into(), author);
                }
                // Every issue comment is a discussion comment, and an
                // unedited one has one timestamp.
                map.remove("commentType");
                if map.get("updatedAt") == map.get("createdAt") {
                    map.remove("updatedAt");
                }
            }
            remove_nulls(&mut c);
            Some(c)
        })
        .collect();
    // Comments that fit the budget and the cap; at least the first shown
    // one. Hidden bots ride free up to the stop.
    let (mut consumed, mut shown, mut used) = (0, 0, 0usize);
    for comment in &fetched {
        if let Some(comment) = comment {
            let chars = crate::tools::stream_page::json_chars(comment) + 1;
            if shown > 0 && (used + chars > budget || shown >= cap) {
                break;
            }
            used += chars;
            shown += 1;
        }
        consumed += 1;
    }
    let more = consumed < fetched.len() || provider_more;
    let bots_hidden = fetched[..consumed].iter().filter(|c| c.is_none()).count();
    let mut pagination = json!({"hasMore": more});
    if more {
        let (page, size) = resume_page(start + consumed);
        pagination["nextPage"] = json!(page);
        pagination["nextPageSize"] = json!(size);
    }
    if bots_hidden > 0 {
        pagination["botsHidden"] = json!(bots_hidden);
    }
    Ok(CommentPage {
        comments: fetched.into_iter().take(consumed).flatten().collect(),
        pagination,
        body: body_page,
        consumed: start + consumed,
        bots_hidden,
    })
}

/// Response chars an issue comment page leaves to the row's cursors,
/// leads, and warnings.
const COMMENT_PAGE_RESERVE: usize = 2_500;

/// Bot comments a page hid are counted, never silently dropped: a warning
/// names them and `hints.includeBots` re-reads the same page with them.
fn attach_bot_read(out: &mut Value, query: &HistoryItemRequest, hidden: usize) {
    let noun = if hidden == 1 { "comment" } else { "comments" };
    super::patch::push_warning(
        out,
        format!("{hidden} bot {noun} hidden on this page; hints.includeBots shows them."),
    );
    let mut nq = super::pr_menu::base_public_query(query, super::ItemOperation::Issue);
    let mut comments = query
        .content_value()
        .and_then(|content| content.get("comments").cloned())
        .unwrap_or(json!({"discussion":true}));
    comments["includeBots"] = json!(true);
    nq["content"] = json!({"comments": comments});
    if !out.get("next").is_some_and(Value::is_object) {
        out["next"] = json!({});
    }
    out["next"]["includeBots"] = super::pr_menu::menu_read(nq);
}

/// `next.readDiscussion`: an issue read without its discussion offers it
/// when GitHub counts any comment (the same issue, `sections:["comments"]`).
fn attach_comments_read(out: &mut Value, query: &HistoryItemRequest, raw: &Value) {
    if super::util::nonzero(raw.get("comments")).is_none() {
        return;
    }
    let mut nq = super::pr_menu::base_public_query(query, super::ItemOperation::Issue);
    for key in ["offset", "length", "commentPage"] {
        super::pr_menu::remove_key(&mut nq, key);
    }
    nq["content"] = json!({"comments": {"discussion": true}});
    if !out.get("next").is_some_and(Value::is_object) {
        out["next"] = json!({});
    }
    out["next"]["readDiscussion"] = super::pr_menu::menu_read(nq);
}

/// The `(commentPage, pageSize)` pair whose page starts at comment index
/// `start`: the largest page size (at most one provider batch) dividing it.
/// The size only places the start; the page fills its budget.
fn resume_page(start: usize) -> (usize, usize) {
    let size = (1..=start.clamp(1, MAX_COLLECTION_PAGE))
        .rev()
        .find(|size| start.is_multiple_of(*size))
        .unwrap_or(1);
    (start / size + 1, size)
}

/// The closing-reference set ends at a bound: no cursor continues it, and the
/// fix it names is a candidate among the listed references only.
fn mark_bounded_closing(out: &mut Value, listed: usize, total: u64, first_window: bool) {
    out["isPartial"] = json!(true);
    out["terminalLimit"] = json!(true);
    let mut reasons = out["partialReasons"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    reasons.push(json!("closingReferenceLimit"));
    out["partialReasons"] = Value::Array(reasons);
    out["warnings"] = json!([if first_window {
        format!(
            "closedBy lists {listed} of {total} linked pull requests; readFixPullRequest is a candidate, not the complete fix set."
        )
    } else {
        format!(
            "The first window's closedBy lists {listed} of {total} linked pull requests; the rest are not listed."
        )
    }]);
}

/// Closing references one issue read lists. Past it the set is bounded and
/// the response says so.
const MAX_CLOSING_REFERENCES: usize = 25;

/// The closing-reference lookup; `first` is [`MAX_CLOSING_REFERENCES`].
pub(super) const CLOSING_REFERENCES_DOCUMENT: &str = "query($owner:String!,$repo:String!,$number:Int!,$first:Int!){ repository(owner:$owner,name:$repo){ issue(number:$number){ closedByPullRequestsReferences(first:$first,includeClosedPrs:true){ totalCount pageInfo{ hasNextPage } nodes{ number state mergedAt additions deletions changedFiles } } } } }";

/// The closing-reference count alone, for windows past the first.
pub(super) const CLOSING_REFERENCE_COUNT_DOCUMENT: &str = "query($owner:String!,$repo:String!,$number:Int!){ repository(owner:$owner,name:$repo){ issue(number:$number){ closedByPullRequestsReferences(first:1,includeClosedPrs:true){ totalCount } } } }";

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
async fn closing_pull_requests<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Option<ClosingReferences> {
    let references = closed_by_references(
        transport,
        query,
        context,
        CLOSING_REFERENCES_DOCUMENT,
        Some(MAX_CLOSING_REFERENCES),
    )
    .await?;
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

/// The issue's `closedByPullRequestsReferences` object that `document`
/// selects (`first` sets its window size). `None` when GraphQL is
/// unavailable or failed.
async fn closed_by_references<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
    document: &str,
    first: Option<usize>,
) -> Option<Value> {
    if !transport.graphql_enabled || !transport.graphql_available(context).await {
        return None;
    }
    let mut variables = json!({
        "owner": query.owner(),
        "repo": query.repo(),
        "number": query.number()?,
    });
    if let Some(first) = first {
        variables["first"] = json!(first);
    }
    let mut page = transport
        .execute_graphql(document, variables, context)
        .await
        .ok()?;
    page.data
        .as_mut()?
        .pointer_mut("/repository/issue/closedByPullRequestsReferences")
        .map(Value::take)
}

/// The closing-reference total for a window past the first: no rows, only
/// whether the set the first window lists is bounded.
async fn closing_reference_count<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Option<ClosingReferences> {
    let total = closed_by_references(
        transport,
        query,
        context,
        CLOSING_REFERENCE_COUNT_DOCUMENT,
        None,
    )
    .await?
    .get("totalCount")?
    .as_u64()?;
    let limit = u64::try_from(MAX_CLOSING_REFERENCES).unwrap_or(u64::MAX);
    Some(ClosingReferences {
        prs: Vec::new(),
        listed: MAX_CLOSING_REFERENCES.min(usize::try_from(total).unwrap_or(usize::MAX)),
        bounded_total: (total > limit).then_some(total),
    })
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

/// `next.readFixPullRequest` reads the first linked (merged-first) pull request;
/// with no link information a closed issue offers the keyword search hop.
fn attach_fix_pr(
    out: &mut Value,
    query: &HistoryItemRequest,
    closed_by: Option<&[ClosingPr]>,
    closed: bool,
    bounded: bool,
) {
    let confidence = if bounded { "medium" } else { "high" };
    let next = match closed_by.and_then(<[ClosingPr]>::first) {
        // A small fix reads whole in one call: its patches are the evidence
        // (`closedBy` already links it to this issue). A larger one names
        // its files, beside its description, so the review can pick patches.
        // Files the caller's goal names: their patches are the evidence, and
        // the filter rides the copied query, so the narrowing is visible.
        Some(pr) => {
            let named = goal_file_globs(query);
            let mut read = json!({
                "operation":"pullRequest","owner":query.owner(),"repo":query.repo(),
                "number":pr.row["number"],
                "sections": if pr.small || !named.is_empty() { json!(["patches"]) } else { json!(["body", "files"]) }});
            if !named.is_empty() {
                read["include"] = json!(named);
            }
            (
                "readFixPullRequest",
                Continuation::new(ToolId::GhGetHistoryItem, read)
                    .confidence(confidence)
                    .build(),
            )
        }
        None if closed && closed_by.is_none() => (
            "findFixPullRequest",
            Continuation::new(
                ToolId::GhSearchHistory,
                json!({"operation":"pullRequest","owner":query.owner(),"repo":query.repo(),
                    "keywords":[query.number().unwrap_or_default().to_string()]}),
            )
            .confidence("medium")
            .build(),
        ),
        None => return,
    };
    if !out.get("next").is_some_and(Value::is_object) {
        out["next"] = json!({});
    }
    out["next"][next.0] = next.1;
}

/// Most goal-named files one fix-PR read narrows to.
const MAX_GOAL_FILES: usize = 5;

/// `include` globs for file names the query's goal mentions
/// (`merge.go`, `pkg/cmd/merge.go`): a word counts only when its name has a
/// known code, config, doc, or lock file type, so dotted identifiers
/// (`res.redirect`) and versions never narrow a read.
fn goal_file_globs(query: &HistoryItemRequest) -> Vec<String> {
    use crate::content::classify_file_type;
    let goal = serde_json::to_value(&query.query)
        .ok()
        .and_then(|value| {
            value
                .get("mainGoal")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
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
            out["next"]["readFixPullRequest"]["query"]["queries"][0]["number"],
            8
        );
        assert_eq!(
            out["next"]["readFixPullRequest"]["query"]["queries"][0]["sections"],
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
                out["next"]["readFixPullRequest"]["query"]["queries"][0]["sections"],
                json!(["body", "files"])
            );
        }
        assert!(
            out_for(&query, &prs)["next"]["readFixPullRequest"]["query"]["queries"][0]
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
        let read = &out["next"]["readFixPullRequest"]["query"]["queries"][0];
        assert_eq!(read["include"], json!(["**/merge.go"]), "{read}");
        assert_eq!(read["sections"], json!(["patches"]), "{read}");
        let out = out_for(
            &issue("Check pkg/cmd/pr/merge/merge.go and README.md (v2.31.0)."),
            &large,
        );
        assert_eq!(
            out["next"]["readFixPullRequest"]["query"]["queries"][0]["include"],
            json!(["**/pkg/cmd/pr/merge/merge.go", "**/README.md"])
        );
        for goal in [
            "What does res.redirect default to in express@4.21.2?",
            "Which PR fixed this issue?",
        ] {
            let read =
                &out_for(&issue(goal), &large)["next"]["readFixPullRequest"]["query"]["queries"][0];
            assert!(read.get("include").is_none(), "{goal}: {read}");
            assert_eq!(read["sections"], json!(["body", "files"]), "{goal}");
        }
    }
}

#[cfg(test)]
mod resume_tests {
    use super::resume_page;

    /// E18: a cut page resumes exactly at its first unshown comment: the
    /// page size only places the start (the largest one dividing it, at
    /// most one provider batch); the next page fills its own budget.
    #[test]
    fn a_cut_comment_page_resumes_at_its_first_unshown_comment() {
        assert_eq!(resume_page(12), (2, 12));
        assert_eq!(resume_page(97), (2, 97));
        assert_eq!(resume_page(190), (3, 95));
        assert_eq!(resume_page(200), (3, 100));
        for start in [1, 17, 97, 101, 250, 1009] {
            let (page, size) = resume_page(start);
            assert!(size <= 100);
            assert_eq!((page - 1) * size, start, "{start}");
        }
    }
}
