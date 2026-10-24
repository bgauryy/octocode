//! Pull-request discussion, review and commit sections: one public page of
//! each, with per-item body windows.
use super::axis::Axis;
use super::inventory::compact_inventory;
use super::remaining::BodyWindows;
use super::text::{body_matches, delivered_earlier, needle, shape_comment, window_body};
use super::util::{array, str_at, string};
use super::window::{WindowState, paginate_window};
use super::{HistoryItemRequest, fetch};
use crate::id::ToolId;
use crate::response::result::remove_nulls;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_get_history_item::sections::Section;
use crate::tools::gh_shared::commit;
use crate::tools::gh_shared::dates::utc_value;
use futures_util::{StreamExt as _, TryStreamExt as _};
use octocode_github::MAX_PER_PAGE;
use octocode_github::{GitHubTransport, ProviderError, RequestContext};
use serde_json::{Map, Value, json};

/// Commit-detail requests in flight at once for `commits.includeFiles`.
const COMMIT_DETAIL_CONCURRENCY: usize = 4;

/// Lines a review-comment read shows on each side of its anchor.
const COMMENT_CODE_PAD: u64 = 3;

/// Ranges one `ghGetFileContent` read takes (the contract's `maxItems`).
const COMMENT_CODE_RANGES: usize = 10;

/// A review comment's anchor: path, the commit the lines belong to, and its
/// `startLine..=line` span. A comment on the old side (`LEFT`) names base
/// lines, which that commit does not hold.
fn comment_anchor(comment: &Value) -> Option<(&str, &str, u64, u64)> {
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
    Some((path, sha, start, line))
}

/// The ghGetFileContent reads of the code the page's anchored review
/// comments discuss: the first anchor's file at its commit, with every
/// anchor of the page on that `(path, commitSha)` as one range each
/// (`startLine-line` padded by [`COMMENT_CODE_PAD`] lines, touching ranges
/// merged), [`COMMENT_CODE_RANGES`] ranges per read.
fn comment_code_reads(query: &HistoryItemRequest, comments: &[Value]) -> Vec<Value> {
    let anchors = comments
        .iter()
        .filter_map(comment_anchor)
        .collect::<Vec<_>>();
    let Some(&(path, sha, _, _)) = anchors.first() else {
        return Vec::new();
    };
    let mut spans = anchors
        .iter()
        .filter(|(p, c, _, _)| *p == path && *c == sha)
        .map(|(_, _, start, line)| {
            (
                start.saturating_sub(COMMENT_CODE_PAD).max(1),
                line + COMMENT_CODE_PAD,
            )
        })
        .collect::<Vec<_>>();
    spans.sort_unstable();
    let mut merged: Vec<(u64, u64)> = Vec::new();
    for span in spans {
        match merged.last_mut() {
            Some(last) if span.0 <= last.1 + 1 => last.1 = last.1.max(span.1),
            _ => merged.push(span),
        }
    }
    merged
        .chunks(COMMENT_CODE_RANGES)
        .map(|spans| {
            let ranges = spans
                .iter()
                .map(|(start, end)| json!({"startLine":start,"endLine":end}))
                .collect::<Vec<_>>();
            crate::response::result::Continuation::related(
                ToolId::GhGetFileContent,
                json!({
                    "owner": gh_get_history_item_query::owner(query),
                    "repo": gh_get_history_item_query::repo(query),
                    "path": path,
                    "ref": sha,
                    "ranges": ranges,
                }),
            )
            .build()
        })
        .collect()
}

/// A pull request's comment section: its discussion (`comments`) or its
/// inline review comments (`reviewComments`), each its own page axis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CommentSection {
    Discussion,
    ReviewInline,
}

impl CommentSection {
    /// The row key and `contentPagination` axis of the section.
    pub(super) fn key(self) -> &'static str {
        self.axis().key()
    }

    fn axis(self) -> Axis {
        match self {
            Self::Discussion => Axis::Comments,
            Self::ReviewInline => Axis::ReviewComments,
        }
    }
}

/// Shape one page of a PR comment `section`; every comment body window of
/// the page joins `windows` (one shared `offset` continues them all).
/// Returns `readCommentCode`, `readCommentCode2`, …: the code the page's
/// anchored review comments on the first anchor's file discuss, ten ranges
/// per read.
pub(super) fn shape_pr_comments(
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    section: CommentSection,
    comments: Vec<Value>,
    state: WindowState,
    query: &HistoryItemRequest,
    windows: &mut BodyWindows,
) -> Vec<Value> {
    let needle = needle(query);
    let comments = comments
        .into_iter()
        .filter(|v| body_matches(v, needle.as_deref()))
        .collect::<Vec<_>>();
    let (slice, page) = state.paginate(
        comments,
        gh_get_history_item_query::page(query),
        Some(gh_get_history_item_query::collection_page_size(query)),
    );
    let mut shaped = Vec::new();
    for comment in slice {
        // The mapped row keeps the review anchor and thread link: path,
        // line range, side, the commit the lines belong to, and the comment
        // this one replies to.
        let (item, body_page) = shape_comment(
            &comment,
            gh_get_history_item_query::char_offset(query),
            query,
            windows,
        );
        // A body continuation lists only the bodies it continues: a whole
        // body an earlier window delivered is not repeated as an empty row.
        if delivered_earlier(gh_get_history_item_query::char_offset(query), &body_page) {
            continue;
        }
        shaped.push(item);
    }
    let code_reads = match section {
        CommentSection::ReviewInline => comment_code_reads(query, &shaped),
        CommentSection::Discussion => Vec::new(),
    };
    super::util::humans_first(&mut shaped);
    if !shaped.is_empty() {
        row[section.key()] = Value::Array(shaped);
    }
    pagination.insert(section.key().into(), page);
    code_reads
}

/// Shape one page of PR reviews. Their cut bodies join `windows`.
pub(super) fn shape_pr_reviews(
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    reviews: Vec<Value>,
    state: WindowState,
    query: &HistoryItemRequest,
    windows: &mut BodyWindows,
) {
    let needle = needle(query);
    let reviews = reviews
        .into_iter()
        .filter(|v| body_matches(v, needle.as_deref()))
        .collect::<Vec<_>>();
    let (slice, page) = state.paginate(
        reviews,
        gh_get_history_item_query::page(query),
        Some(gh_get_history_item_query::collection_page_size(query)),
    );
    let mut shaped = Vec::new();
    for review in slice {
        let raw_body = string(review.get("body"));
        let (body, body_page) = window_body(
            &raw_body,
            gh_get_history_item_query::char_offset(query),
            query,
            windows,
        );
        if delivered_earlier(gh_get_history_item_query::char_offset(query), &body_page) {
            continue;
        }
        let mut item = json!({
            // A review without an identity has none: never the text "null".
            "id": match &review["id"] {
                Value::Number(id) => Some(id.to_string()),
                Value::String(id) if !id.is_empty() => Some(id.clone()),
                _ => None,
            },
            "author": str_at(&review,"/user/login").unwrap_or("unknown"),
            "state": string(review.get("state")),
            "body": (!body.is_empty()).then_some(body),
            "bodyPagination": (!raw_body.is_empty() && (gh_get_history_item_query::char_offset(query).unwrap_or(0)>0 || body_page["hasMore"]==true)).then_some(body_page),
            "submittedAt": utc_value(review.get("submitted_at")), "commitSha": review.get("commit_id")
        });
        remove_nulls(&mut item);
        shaped.push(item);
    }
    row["reviews"] = Value::Array(shaped);
    pagination.insert(Axis::Reviews.key().into(), page);
}

/// Shape one page of PR commits. With `commitFiles` each commit lists its
/// changed paths (the inventory rows, no patches: a commit read returns
/// those) from one commit-detail read; a commit with more files than one
/// file page shows continues on the row's `next`
/// (`continueCommitFiles`, `continueCommitFiles2`, …: that commit's next file
/// page), never in a list nested in the commit. Returns those
/// continuations.
pub(super) async fn shape_pr_commits(
    transport: &GitHubTransport,
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    commits: Vec<Value>,
    state: WindowState,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<Vec<(String, Value)>, ProviderError> {
    let include_files = query.sections().has(Section::CommitFiles);
    let (slice, page) = state.paginate(
        commits,
        gh_get_history_item_query::page(query),
        Some(gh_get_history_item_query::collection_page_size(query)),
    );
    // Each commit's files are one detail request: a few run at once, in
    // page order, through the transport's shared admission and deadline.
    let shaped: Vec<(Value, Option<Value>)> = futures_util::stream::iter(slice)
        .map(|item| async move {
            let sha = string(item.get("sha"));
            // The shared commit row; a pull request's commit list keeps
            // each message whole: the body follows its headline.
            let mut commit = commit::row(&item);
            if let Some(body) = commit::message_body(&item) {
                commit["messageBody"] = json!(body);
            }
            let mut more_files = None;
            if include_files {
                // One provider batch of paths per commit.
                let per_page = MAX_PER_PAGE;
                let (detail, more) = fetch(
                    transport,
                    &[
                        "repos",
                        gh_get_history_item_query::owner(query),
                        gh_get_history_item_query::repo(query),
                        "commits",
                        &sha,
                    ],
                    &[("per_page", per_page.to_string()), ("page", "1".into())],
                    context,
                )
                .await?;
                let files = array(detail.get("files").cloned().unwrap_or(json!([])));
                let (files, mut files_page) =
                    paginate_window(files, 0, !more, Some(1), Some(per_page));
                commit["files"] = Value::Array(compact_inventory(&files));
                // The cursor rides the continuation, never the block.
                if let Some(next_page) = files_page
                    .as_object_mut()
                    .and_then(|page| page.remove("nextPage"))
                    .and_then(|page| page.as_u64())
                {
                    commit["contentPagination"][Axis::Files.key()] = files_page;
                    more_files = Some(commit_files_page(query, &sha, next_page, per_page));
                }
            }
            Ok::<_, ProviderError>((commit, more_files))
        })
        .buffered(COMMIT_DETAIL_CONCURRENCY)
        .try_collect()
        .await?;
    let mut continuations = Vec::new();
    let mut commits = Vec::with_capacity(shaped.len());
    for (commit, more_files) in shaped {
        commits.push(commit);
        if let Some(read) = more_files {
            let name = crate::id::numbered("continueCommitFiles", continuations.len());
            continuations.push((name, read));
        }
    }
    row["commits"] = Value::Array(commits);
    pagination.insert(Axis::Commits.key().into(), page);
    Ok(continuations)
}

/// The next changed-file page of one PR commit: the commit read of its
/// paths at `page`.
fn commit_files_page(query: &HistoryItemRequest, sha: &str, page: u64, page_size: usize) -> Value {
    crate::response::result::Continuation::continued(
        ToolId::GhGetHistoryItem,
        json!({"operation":"commit","owner":gh_get_history_item_query::owner(query),"repo":gh_get_history_item_query::repo(query),"ref":sha,
            "sections":["files"],"page":page,"pageSize":page_size}),
    )
    .build()
}

#[cfg(test)]
mod tests;
