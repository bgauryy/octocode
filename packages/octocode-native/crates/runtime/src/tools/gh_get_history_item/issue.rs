//! `operation: "issue"`: issue metadata, body window, and one page of
//! discussion comments.
use super::continuations::promote_issue_continuations;
use super::util::{
    array, content_flag, history_body_view, map_comments, merge, paginate_text, str_at, string,
    window_body,
};
use super::{HistoryItemRequest, default_page_size, fetch, validation};
use crate::providers::github::{
    CredentialResolver, GitHubTransport, ProviderError, ProviderErrorKind, ProviderErrorReason,
    RequestContext,
};
use crate::tools::id::ToolId;
use crate::tools::result::remove_nulls;
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
    // The closing PRs load beside the issue on its first window only.
    let first_window = query.comment_page().unwrap_or(1) <= 1
        && query.char_offset().is_none_or(|offset| offset == 0);
    let (fetched, closed_by) = tokio::join!(fetch(transport, &issue_path, &[], context), async {
        if first_window {
            closing_pull_requests(transport, query, context).await
        } else {
            None
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
    let mut row = json!({
        "number":raw["number"],"title":string(raw.get("title")),
        "state":str_at(&raw,"/state").unwrap_or("open"),"author":str_at(&raw,"/user/login").unwrap_or("unknown"),
        "labels":raw.get("labels").and_then(Value::as_array).into_iter().flatten().filter_map(|v|str_at(v,"/name").map(str::to_owned)).collect::<Vec<_>>(),
        "createdAt":string(raw.get("created_at")),"updatedAt":string(raw.get("updated_at")),
        "closedAt":raw.get("closed_at").filter(|v|!v.is_null())
    });
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
    if want_comments {
        let page_no = query.comment_page().unwrap_or(1);
        let per = query.page_size().unwrap_or_else(default_page_size);
        let (raw_comments, more) = if page_no == 0 {
            (json!([]), false)
        } else {
            fetch(
                transport,
                &[issue_path.as_slice(), &["comments"]].concat(),
                &[("per_page", per.to_string()), ("page", page_no.to_string())],
                context,
            )
            .await?
        };
        let mut shaped = Vec::new();
        let mut body_page = None;
        for comment in map_comments(array(raw_comments), "discussion", include_bots) {
            let (body, page) = window_body(
                str_at(&comment, "/body").unwrap_or(""),
                query.char_offset(),
                query,
                &mut body_page,
            );
            let mut c = merge(comment, json!({"body":body,"bodyPagination":page}));
            if let Some(map) = c.as_object_mut()
                && let Some(author) = map.remove("author")
            {
                map.insert("user".into(), author);
            }
            remove_nulls(&mut c);
            shaped.push(c);
        }
        if !shaped.is_empty() {
            row["comments"] = Value::Array(shaped);
        }
        // `raw.comments` is the issue's real comment count; the page count is
        // `returnedComments` (bots may be hidden from it).
        pagination.insert("comments".into(),json!({"currentPage":page_no,"itemsPerPage":per,"totalComments":raw.get("comments").and_then(Value::as_u64),"returnedComments":row["comments"].as_array().map_or(0,Vec::len),"hasMore":more,"nextCommentPage":more.then_some(page_no+1)}));
        if let Some(page) = body_page {
            pagination.insert("commentBody".into(), page);
        }
    }
    if !pagination.is_empty() {
        row["contentPagination"] = Value::Object(pagination);
    }
    let closed = raw.get("state").and_then(Value::as_str) == Some("closed");
    if let Some(references) = closed_by.as_ref().filter(|refs| !refs.prs.is_empty()) {
        row["closedBy"] = json!(references.prs);
    }
    let mut out = json!({"type":"issues","owner":query.owner(),"repo":query.repo(),"issues":[row],"totalCount":1});
    promote_issue_continuations(&mut out, query);
    let bounded = closed_by.as_ref().and_then(|references| {
        references
            .bounded_total
            .map(|total| (references.prs.len(), total))
    });
    if let Some((listed, total)) = bounded {
        // No cursor continues the set: it ends here, and the fix it names
        // is a candidate among the listed references only.
        out["isPartial"] = json!(true);
        out["terminalLimit"] = json!(true);
        out["partialReasons"] = json!(["closingReferenceLimit"]);
        out["warnings"] = json!([format!(
            "closedBy lists {listed} of {total} linked pull requests; readFixPr is a candidate, not the complete fix set."
        )]);
    }
    if first_window {
        let prs = closed_by
            .as_ref()
            .map(|references| references.prs.as_slice());
        attach_fix_pr(&mut out, query, prs, closed, bounded.is_some());
    }
    Ok(out)
}

/// Closing references one issue read lists. Past it the set is bounded and
/// the response says so.
const MAX_CLOSING_REFERENCES: usize = 25;

/// The closing-reference lookup; `first` is [`MAX_CLOSING_REFERENCES`].
pub(super) const CLOSING_REFERENCES_DOCUMENT: &str = "query($owner:String!,$repo:String!,$number:Int!,$first:Int!){ repository(owner:$owner,name:$repo){ issue(number:$number){ closedByPullRequestsReferences(first:$first,includeClosedPrs:true){ totalCount pageInfo{ hasNextPage } nodes{ number state mergedAt } } } } }";

/// The linked pull requests one read returned, merged first, and GitHub's
/// total when more exist than were returned.
struct ClosingReferences {
    prs: Vec<Value>,
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
    if !transport.graphql_enabled || !transport.graphql_available(context).await {
        return None;
    }
    let variables = json!({
        "owner": query.owner(),
        "repo": query.repo(),
        "number": query.number()?,
        "first": MAX_CLOSING_REFERENCES,
    });
    let page = transport
        .execute_graphql(CLOSING_REFERENCES_DOCUMENT, variables, context)
        .await
        .ok()?;
    let references = page
        .data
        .as_ref()?
        .pointer("/repository/issue/closedByPullRequestsReferences")?;
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
        prs,
    })
}

fn map_closing_pull_requests(nodes: &[Value]) -> Vec<Value> {
    let mut prs = nodes
        .iter()
        .filter_map(|node| {
            let number = node.get("number").and_then(Value::as_u64)?;
            let mut pr = json!({
                "number": number,
                "state": str_at(node, "/state").unwrap_or("closed").to_ascii_lowercase(),
                "mergedAt": node.get("mergedAt").filter(|v| !v.is_null()),
            });
            remove_nulls(&mut pr);
            Some(pr)
        })
        .collect::<Vec<_>>();
    // Stable: merged first, otherwise GitHub's order.
    prs.sort_by_key(|pr| pr.get("mergedAt").is_none());
    prs
}

/// `next.readFixPr` reads the first linked (merged-first) pull request;
/// with no link information a closed issue offers the keyword search hop.
fn attach_fix_pr(
    out: &mut Value,
    query: &HistoryItemRequest,
    closed_by: Option<&[Value]>,
    closed: bool,
    bounded: bool,
) {
    let confidence = if bounded { "medium" } else { "high" };
    let next = match closed_by.and_then(<[Value]>::first) {
        Some(pr) => (
            "readFixPr",
            json!({"tool":ToolId::GhGetHistoryItem.as_str(),"confidence":confidence,"query":{
                "operation":"pullRequest","owner":query.owner(),"repo":query.repo(),
                "number":pr["number"],"include":["body","files"]}}),
        ),
        None if closed && closed_by.is_none() => (
            "findFixPr",
            json!({"tool":ToolId::GhSearchHistory.as_str(),"confidence":"medium","query":{
                "operation":"pullRequest","owner":query.owner(),"repo":query.repo(),
                "keywords":[query.number().unwrap_or_default().to_string()]}}),
        ),
        None => return,
    };
    if !out.get("next").is_some_and(Value::is_object) {
        out["next"] = json!({});
    }
    out["next"][next.0] = next.1;
}
