//! Pull-request discussion, review and commit sections: one public page of
//! each, with per-item body windows.
use super::patch::{attach_patch_cursor, shape_files};
use super::patch_hop::{DiffCursors, attach_diff_continuations, take_reshaped_paths};
use super::util::{array, body_matches, needle, str_at, string, window_body};
use super::window::{WindowState, paginate_window};
use super::{HistoryItemRequest, ItemOperation, fetch};
use crate::providers::github::{
    CredentialResolver, GitHubTransport, ProviderError, RequestContext,
};
use crate::tools::id::ToolId;
use crate::tools::result::remove_nulls;
use futures_util::{StreamExt as _, TryStreamExt as _};
use serde_json::{Map, Value, json};

/// Fields of a mapped comment that anchor it in the diff and its thread.
const COMMENT_ANCHOR_KEYS: [&str; 8] = [
    "path",
    "side",
    "startLine",
    "line",
    "commitSha",
    "outdated",
    "subjectType",
    "inReplyToId",
];

/// Commit-detail requests in flight at once for `commits.includeFiles`.
const COMMIT_DETAIL_CONCURRENCY: usize = 4;

/// What one page of PR comments adds beside its rows.
pub(super) struct CommentShape {
    /// A minified body view dropped text from a comment.
    pub(super) dropped: bool,
    /// `hints.readCommentCode`: the code the page's first anchored review
    /// comment discusses.
    pub(super) code_read: Option<Value>,
}

/// The ghGetFileContent read of the lines a review comment anchors: its
/// path at the commit the lines belong to, `startLine-line`. A comment on
/// the old side (`LEFT`) names base lines, which that commit does not hold.
fn comment_code_read(query: &HistoryItemRequest, comment: &Value) -> Option<Value> {
    if str_at(comment, "/side") == Some("LEFT") {
        return None;
    }
    let path = str_at(comment, "/path")?;
    let sha = str_at(comment, "/commitSha")?;
    let line = comment.get("line").and_then(Value::as_u64)?;
    let start = comment
        .get("startLine")
        .and_then(Value::as_u64)
        .filter(|start| *start <= line)
        .unwrap_or(line);
    Some(
        crate::tools::result::Continuation::new(
            ToolId::GhGetFileContent,
            json!({
                "owner": query.owner(),
                "repo": query.repo(),
                "path": path,
                "ref": sha,
                "ranges": [format!("{start}-{line}")],
            }),
        )
        .why("Read the code the review comment discusses.")
        .confidence("exact")
        .build(),
    )
}

/// Shape one page of PR comments.
pub(super) fn shape_pr_comments(
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    comments: Vec<Value>,
    state: WindowState,
    query: &HistoryItemRequest,
) -> CommentShape {
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
            query.char_offset(),
            query,
            &mut first_body_page,
            &mut dropped,
        );
        // A whole body restates no window, and an unedited comment no
        // second timestamp.
        let windowed = body_page["offset"] != 0 || body_page["hasMore"] == true;
        let updated = comment
            .get("updatedAt")
            .filter(|updated| comment.get("createdAt") != Some(*updated));
        let mut item = json!({
            "id": comment["id"], "author": comment["author"],
            "commentType": comment.get("commentType").cloned().unwrap_or(json!("discussion")),
            "body": body, "bodyPagination": windowed.then_some(body_page),
            "createdAt": comment.get("createdAt"), "updatedAt": updated
        });
        // The review anchor and thread link: path, line range, side, the
        // commit the lines belong to, and the comment this one replies to.
        for key in COMMENT_ANCHOR_KEYS {
            item[key] = comment.get(key).cloned().unwrap_or(Value::Null);
        }
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
    let code_read = row["comments"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|comment| comment_code_read(query, comment));
    CommentShape { dropped, code_read }
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
                "date":super::util::utc_date(str_at(&item,"/commit/author/date"))
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
                let (files, mut files_page) =
                    paginate_window(files, 0, !more, Some(1), Some(query.collection_page_size()));
                let (mut files, cursor) = shape_files(files, true, query);
                let reshaped = take_reshaped_paths(Some(&mut files), query.debug());
                attach_patch_cursor(&mut files_page, cursor);
                let cursors = DiffCursors::of_file_page(
                    &files_page,
                    files.as_array().is_some_and(|files| !files.is_empty()),
                );
                commit["files"] = files;
                commit["filePagination"] = files_page;
                attach_diff_continuations(
                    &mut commit,
                    query,
                    ItemOperation::Commit,
                    Some(&sha),
                    true,
                    cursors,
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

/// A PR commit's file rows show the PR's patch view (narrowed to
/// `matchString` hits); a commit read is never narrowed, so it returns every
/// patch of that commit whole, paged.
fn attach_raw_commit_read(commit: &mut Value, query: &HistoryItemRequest, sha: &str) {
    if !commit.get("next").is_some_and(Value::is_object) {
        commit["next"] = json!({});
    }
    commit["next"]["readUntrimmed"] = crate::tools::result::Continuation::new(
        ToolId::GhGetHistoryItem,
        json!({"operation":"commit","owner":query.owner(),"repo":query.repo(),"ref":sha,"sections":["patches"]}),
    )
    .why("Read this commit's raw patches; the rows above are a reshaped view.")
    .confidence("exact")
    .build();
}

#[cfg(test)]
mod tests {
    use super::super::util::map_comments;
    use super::*;

    fn shaped(raw: Vec<Value>) -> Vec<Value> {
        let query = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["reviewComments"]
        }))
        .expect("pull request row");
        let mut row = json!({});
        let mut pagination = Map::new();
        shape_pr_comments(
            &mut row,
            &mut pagination,
            map_comments(raw, "review_inline", true),
            WindowState::COMPLETE,
            &query,
        );
        array(row["comments"].clone())
    }

    fn raw(fields: Value) -> Value {
        super::super::util::merge(
            json!({"id":1,"user":{"login":"rev"},"body":"b","created_at":"2025-11-13T20:12:39Z",
                "path":"src/a.ts","side":"RIGHT","subject_type":"line","in_reply_to_id":null}),
            fields,
        )
    }

    /// react#35129: an outdated comment has `line: null`; its anchor is the
    /// original line range at the original commit, flagged outdated.
    #[test]
    fn outdated_review_comment_keeps_its_original_anchor() {
        let comments = shaped(vec![raw(json!({
            "id":2524805000_u64,"line":null,"start_line":null,"commit_id":"cd4d",
            "original_line":1765,"original_start_line":1758,"original_commit_id":"cd4d9e9"
        }))]);
        let c = &comments[0];
        assert_eq!(c["line"], 1765, "{c}");
        assert_eq!(c["startLine"], 1758, "{c}");
        assert_eq!(c["commitSha"], "cd4d9e9", "{c}");
        assert_eq!(c["outdated"], true, "{c}");
        assert_eq!(c["side"], "RIGHT", "{c}");
        assert_eq!(c["path"], "src/a.ts", "{c}");
    }

    /// react#37621: a current multi-line comment anchors at its live range.
    #[test]
    fn current_multi_line_comment_anchors_at_its_live_range() {
        let comments = shaped(vec![raw(json!({
            "line":2776,"start_line":2773,"commit_id":"head1",
            "original_line":2770,"original_start_line":2767,"original_commit_id":"old1"
        }))]);
        let c = &comments[0];
        assert_eq!(
            (c["startLine"].clone(), c["line"].clone()),
            (json!(2773), json!(2776)),
            "{c}"
        );
        assert_eq!(c["commitSha"], "head1", "{c}");
        assert!(c.get("outdated").is_none(), "{c}");
    }

    /// FIX §0 #2: an anchored review comment leads to its code at the
    /// commit the lines belong to; an old-side comment does not.
    #[test]
    fn review_comments_lead_to_the_code_they_discuss() {
        let query = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["reviewComments"]
        }))
        .expect("pull request row");
        let mut row = json!({});
        let comments = map_comments(
            vec![
                raw(json!({"id":1,"line":5,"start_line":null,"commit_id":"c0","side":"LEFT"})),
                raw(json!({"id":2,"line":2776,"start_line":2773,"commit_id":"head1"})),
            ],
            "review_inline",
            true,
        );
        let shape = shape_pr_comments(
            &mut row,
            &mut Map::new(),
            comments,
            WindowState::COMPLETE,
            &query,
        );
        let read = shape.code_read.expect("a code read");
        assert_eq!(read["tool"], "ghGetFileContent", "{read}");
        let q = &read["query"]["queries"][0];
        assert_eq!(q["ref"], "head1", "{read}");
        assert_eq!(q["ranges"], json!(["2773-2776"]), "{read}");
        assert_eq!(q["path"], "src/a.ts", "{read}");
    }

    /// A single-line comment omits startLine; a reply keeps its thread link
    /// (react#37543 replies lost it); a LEFT-side comment keeps its side.
    #[test]
    fn replies_sides_and_single_lines_survive_shaping() {
        let comments = shaped(vec![
            raw(json!({"id":10,"line":108,"start_line":null,"commit_id":"c1"})),
            raw(json!({"id":11,"line":108,"commit_id":"c1","in_reply_to_id":10,"side":"LEFT"})),
        ]);
        assert!(comments[0].get("startLine").is_none(), "{}", comments[0]);
        assert!(comments[0].get("inReplyToId").is_none(), "{}", comments[0]);
        assert_eq!(comments[1]["inReplyToId"], "10", "{}", comments[1]);
        assert_eq!(comments[1]["side"], "LEFT", "{}", comments[1]);
    }

    /// A file-level comment has no line anchor and says so.
    #[test]
    fn file_level_comment_has_no_line() {
        let comments = shaped(vec![raw(json!({
            "subject_type":"file","line":null,"original_line":null,"commit_id":"c1","original_commit_id":"c0"
        }))]);
        let c = &comments[0];
        assert_eq!(c["subjectType"], "file", "{c}");
        assert!(
            c.get("line").is_none() && c.get("outdated").is_none(),
            "{c}"
        );
        assert_eq!(c["commitSha"], "c1", "{c}");
    }
}
