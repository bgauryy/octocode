//! `operation: "issue"`: issue metadata, body window, and one page of
//! discussion comments.
use super::continuations::promote_issue_continuations;
use super::util::{
    array, content_flag, history_body_view, map_comments, merge, paginate_text, str_at, string,
    window_body,
};
use super::{DEFAULT_PAGE_SIZE, GhGetHistoryItemQuery, fetch, validation};
use crate::providers::github::{
    CredentialResolver, GitHubTransport, ProviderError, ProviderErrorReason, RequestContext,
};
use crate::tools::result::remove_nulls;
use serde_json::{Map, Value, json};

pub(super) async fn issue<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &GhGetHistoryItemQuery,
    context: &RequestContext,
) -> Result<Value, ProviderError> {
    let number = query
        .number
        .ok_or_else(|| validation("number is required"))?
        .to_string();
    let issue_path = ["repos", &query.owner, &query.repo, "issues", &number];
    let (raw, _) = fetch(transport, &issue_path, &[], context).await?;
    if raw.get("pull_request").is_some_and(|v| !v.is_null()) {
        return Err(validation(&format!(
            "Issue #{number} is a pull request; use ghGetHistoryItem operation:\"pullRequest\" with number:{number}."
        ))
        .with_reason(ProviderErrorReason::IssueIsPullRequest));
    }
    let content = query.content.as_ref().and_then(Value::as_object);
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
        let (body, page) = paginate_text(&body_view, query.char_offset, query.char_length);
        row["body"] = json!(body);
        if query.char_offset.is_some() || query.char_length.is_some() || page["hasMore"] == true {
            pagination.insert("body".into(), page);
        }
    }
    if want_comments {
        let page_no = query.comment_page.unwrap_or(1);
        let per = query.page_size.unwrap_or(DEFAULT_PAGE_SIZE);
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
                query.char_offset,
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
    let mut out = json!({"type":"issues","owner":query.owner,"repo":query.repo,"issues":[row],"totalCount":1});
    promote_issue_continuations(&mut out, query);
    Ok(out)
}
