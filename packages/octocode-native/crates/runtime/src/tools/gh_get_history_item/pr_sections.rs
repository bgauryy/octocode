//! Pull-request discussion, review and commit sections: one public page of
//! each, with per-item body windows.
use super::continuations::{attach_diff_continuations, take_reshaped_paths};
use super::files::{attach_patch_cursor, shape_files};
use super::util::{array, body_matches, needle, str_at, string, window_body};
use super::window::{WindowState, commit_files_pagination, paginate_window};
use super::{HistoryItemRequest, ItemOperation, fetch};
use crate::providers::github::{
    CredentialResolver, GitHubTransport, ProviderError, RequestContext,
};
use crate::tools::id::ToolId;
use crate::tools::result::remove_nulls;
use futures_util::{StreamExt as _, TryStreamExt as _};
use serde_json::{Map, Value, json};

/// Commit-detail requests in flight at once for `commits.includeFiles`.
const COMMIT_DETAIL_CONCURRENCY: usize = 4;

/// Shape one page of PR comments; true when a minified body view dropped
/// text from any of them.
pub(super) fn shape_pr_comments(
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    comments: Vec<Value>,
    state: WindowState,
    query: &HistoryItemRequest,
) -> bool {
    let needle = needle(query);
    let mut comments = comments
        .into_iter()
        .filter(|v| body_matches(v, needle.as_deref()))
        .collect::<Vec<_>>();
    let is_inline = |v: &Value| str_at(v, "/commentType") == Some("review_inline");
    comments.sort_by_key(|v| if is_inline(v) { 0 } else { 1 });
    let total_comments = comments.len();
    let inline_comments = comments.iter().filter(|v| is_inline(v)).count();
    let mut commenters = Vec::<String>::new();
    for author in comments.iter().filter_map(|v| str_at(v, "/author")) {
        if !commenters.iter().any(|v| v == author) {
            commenters.push(author.to_owned());
        }
    }
    let latest = comments
        .iter()
        .filter_map(|v| str_at(v, "/updatedAt").or_else(|| str_at(v, "/createdAt")))
        .max()
        .map(str::to_owned);
    let (slice, page) = state.paginate(
        comments,
        query.comment_page(),
        Some(query.collection_page_size()),
    );
    let mut shaped = Vec::new();
    let mut first_body_page = None;
    let mut dropped = false;
    for comment in slice {
        let (body, body_page) = window_body(
            &string(comment.get("body")),
            query.comment_body_offset(),
            query,
            &mut first_body_page,
            &mut dropped,
        );
        let mut item = json!({
            "id": comment["id"], "author": comment["author"],
            "commentType": comment.get("commentType").cloned().unwrap_or(json!("discussion")),
            "path": comment.get("path"), "line": comment.get("line"),
            "body": body, "bodyPagination": body_page,
            "createdAt": comment.get("createdAt"), "updatedAt": comment.get("updatedAt")
        });
        remove_nulls(&mut item);
        shaped.push(item);
    }
    if !shaped.is_empty() {
        row["comments"] = Value::Array(shaped);
        row["reviewSummary"] = json!({"totalComments":total_comments,"inlineComments":inline_comments,"discussionComments":total_comments-inline_comments,"commenters":commenters.iter().take(8).collect::<Vec<_>>(),"commenterCount":commenters.len(),"latestCommentAt":latest,"themes":["discussion"],"countScope":"providerBatch"});
    }
    pagination.insert("comments".into(), page);
    if let Some(page) = first_body_page {
        pagination.insert("commentBody".into(), page);
    }
    dropped
}

/// Shape one page of PR reviews; true when a minified body view dropped
/// text from any of them.
pub(super) fn shape_pr_reviews(
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    reviews: Vec<Value>,
    state: WindowState,
    query: &HistoryItemRequest,
) -> bool {
    let needle = needle(query);
    let reviews = reviews
        .into_iter()
        .filter(|v| body_matches(v, needle.as_deref()))
        .collect::<Vec<_>>();
    let (slice, page) = state.paginate(
        reviews,
        query.review_page(),
        Some(query.collection_page_size()),
    );
    let mut shaped = Vec::new();
    let mut first_body_page = None;
    let mut dropped = false;
    for review in slice {
        let raw_body = string(review.get("body"));
        let (body, body_page) = window_body(
            &raw_body,
            query.char_offset(),
            query,
            &mut first_body_page,
            &mut dropped,
        );
        let mut item = json!({
            // A review without an identity has none: never the text "null".
            "id": match &review["id"] {
                Value::Number(id) => Some(id.to_string()),
                Value::String(id) if !id.is_empty() => Some(id.clone()),
                _ => None,
            },
            "user": str_at(&review,"/user/login").unwrap_or("unknown"),
            "state": string(review.get("state")),
            "body": (!body.is_empty()).then_some(body),
            "bodyPagination": (!raw_body.is_empty() && (query.char_offset().unwrap_or(0)>0 || body_page["hasMore"]==true)).then_some(body_page),
            "submittedAt": review.get("submitted_at"), "commitId": review.get("commit_id")
        });
        remove_nulls(&mut item);
        shaped.push(item);
    }
    row["reviews"] = Value::Array(shaped);
    pagination.insert("reviews".into(), page);
    if let Some(page) = first_body_page {
        pagination.insert("reviewBody".into(), page);
    }
    dropped
}

pub(super) async fn shape_pr_commits<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    commits: Vec<Value>,
    state: WindowState,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<(), ProviderError> {
    let include_files = query
        .content_value()
        .as_ref()
        .and_then(|content| content.pointer("/commits/includeFiles"))
        .and_then(Value::as_bool)
        == Some(true);
    let (slice, page) = state.paginate(
        commits,
        query.commit_page(),
        Some(query.collection_page_size()),
    );
    // Each commit's files are one detail request: a few run at once, in
    // page order, through the transport's shared admission and deadline.
    let shaped: Vec<Value> = futures_util::stream::iter(slice)
        .map(|item| async move {
            let sha = string(item.get("sha"));
            let mut commit = json!({
                "sha":sha,
                "message":str_at(&item,"/commit/message").unwrap_or(""),
                "author":str_at(&item,"/commit/author/name").unwrap_or("unknown"),
                "date":str_at(&item,"/commit/author/date").unwrap_or("")
            });
            if include_files {
                let (detail, more) = fetch(
                    transport,
                    &["repos", query.owner(), query.repo(), "commits", &sha],
                    &[("per_page", "100".into()), ("page", "1".into())],
                    context,
                )
                .await?;
                let files = array(detail.get("files").cloned().unwrap_or(json!([])));
                let (files, files_page) =
                    paginate_window(files, 0, !more, Some(1), Some(query.collection_page_size()));
                let (mut files, cursor) = shape_files(files, true, query);
                let reshaped = take_reshaped_paths(Some(&mut files), query.debug());
                let mut files_page = commit_files_pagination(files_page);
                attach_patch_cursor(&mut files_page, cursor);
                commit["files"] = files;
                commit["filesPagination"] = files_page;
                attach_diff_continuations(
                    &mut commit,
                    query,
                    ItemOperation::Commit,
                    Some(&sha),
                    true,
                );
                if !reshaped.is_empty() {
                    attach_raw_commit_read(&mut commit, query, &sha);
                }
            }
            Ok::<_, ProviderError>(commit)
        })
        .buffered(COMMIT_DETAIL_CONCURRENCY)
        .try_collect()
        .await?;
    row["commits"] = Value::Array(shaped);
    pagination.insert("commits".into(), page);
    Ok(())
}

/// A PR commit's file rows show the PR's patch view (minified or narrowed to
/// `matchString` hits); a commit read is never reshaped, so it returns every
/// patch of that commit whole, paged.
fn attach_raw_commit_read(commit: &mut Value, query: &HistoryItemRequest, sha: &str) {
    if !commit.get("next").is_some_and(Value::is_object) {
        commit["next"] = json!({});
    }
    commit["next"]["readUntrimmed"] = json!({
        "tool": ToolId::GhGetHistoryItem.as_str(),
        "query": {"operation":"commit","owner":query.owner(),"repo":query.repo(),"ref":sha,"includeDiff":true},
        "why": "Read this commit's raw patches; the rows above are a reshaped view.",
        "confidence": "exact"
    });
}
