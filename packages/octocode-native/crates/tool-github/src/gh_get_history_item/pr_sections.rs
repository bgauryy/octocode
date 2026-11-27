//! Pull-request discussion, review and commit sections: one public page of
//! each, with per-item body windows.
use super::axis::Axis;
use super::filter::{FileFilter, FileScope};
use super::inventory::compact_inventory;
use super::remaining::BodyWindows;
use super::text::{delivered_earlier, shape_comment, window_body};
use super::util::{str_at, string};
use super::window::{WindowState, lift_page_limits};
use super::{HistoryItemRequest, validation};
use crate::gh_get_history_item::Source;
use crate::gh_get_history_item::gh_get_history_item_query;
use crate::gh_get_history_item::sections::Section;
use futures_util::{StreamExt as _, TryStreamExt as _};
use octocode_contract_types::GhGetFileContentQuery;
use octocode_github::MAX_PER_PAGE;
use octocode_github::{ProviderError, RequestContext};
use octocode_runtime_base::response::result::wire_query;
use octocode_runtime_base::response::result::{Continuation, remove_nulls};
use octocode_tools_shared::gh_shared::commit;
use octocode_tools_shared::gh_shared::dates::utc_value;
use serde_json::{Map, Value, json};

mod commit_files;

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
        .filter_map(|spans| {
            let ranges = spans
                .iter()
                .map(|(start, end)| json!({"startLine":start,"endLine":end}))
                .collect::<Vec<_>>();
            let read = wire_query::<GhGetFileContentQuery>(json!({
                "owner": gh_get_history_item_query::owner(query),
                "repo": gh_get_history_item_query::repo(query),
                "path": path,
                "ref": sha,
                "ranges": ranges,
            }))?;
            Some(Continuation::related_query(read).build())
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
    /// The row key and `sectionPagination` axis of the section.
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
    // A comment names its review only where that review's row folds into
    // it ([`shape_pr_reviews`]), which takes the reviews in the same read.
    if !query.sections().has(Section::Reviews) {
        for comment in &mut shaped {
            drop_review_id(comment);
        }
    }
    // A section the caller asked for is always stated: an empty list says
    // zero, where an absent one would read as not fetched. A body hop
    // lists only the bodies it continues, so it states none.
    let body_hop = gh_get_history_item_query::char_offset(query).is_some_and(|offset| offset > 0);
    if !shaped.is_empty() || !body_hop {
        row[section.key()] = Value::Array(shaped);
    }
    pagination.insert(section.key().into(), page);
    code_reads
}

/// A comment row without its `reviewId`.
fn drop_review_id(comment: &mut Value) {
    if let Some(fields) = comment.as_object_mut() {
        fields.shift_remove("reviewId");
    }
}

/// Whether a review is the batch GitHub made of inline comments this read
/// shows: a COMMENTED review with no body, one of whose comments (same
/// review id, same author) is on the row's `reviewComments` page. Those
/// comments state its author, time and commit, and name it (`reviewId`).
fn folds_into_comments(review: &Value, raw_body: &str, row: &Value) -> Option<String> {
    if review.get("state").and_then(Value::as_str) != Some("COMMENTED")
        || !raw_body.trim().is_empty()
    {
        return None;
    }
    let id = match &review["id"] {
        Value::Number(id) => id.to_string(),
        Value::String(id) if !id.is_empty() => id.clone(),
        _ => return None,
    };
    let author = str_at(review, "/user/login").unwrap_or("unknown");
    row.get("reviewComments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|comment| {
            comment.get("reviewId").and_then(Value::as_str) == Some(id.as_str())
                && comment.get("author").and_then(Value::as_str) == Some(author)
        })
        .then_some(id)
}

/// Shape one page of PR reviews. Their cut bodies join `windows`. A review
/// that only batched inline comments shown in this read is not listed
/// again ([`folds_into_comments`]): its comments name it, and every other
/// comment drops its `reviewId`. Returns how many reviews were folded.
pub(super) fn shape_pr_reviews(
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    reviews: Vec<Value>,
    state: WindowState,
    query: &HistoryItemRequest,
    windows: &mut BodyWindows,
) -> usize {
    let (slice, page) = state.paginate(
        reviews,
        gh_get_history_item_query::page(query),
        Some(gh_get_history_item_query::collection_page_size(query)),
    );
    let mut shaped = Vec::new();
    let mut folded = Vec::new();
    for review in slice {
        let raw_body = string(review.get("body"));
        if let Some(id) = folds_into_comments(&review, &raw_body, row) {
            folded.push(id);
            continue;
        }
        // A `matchString` read shows a review body's hit lines.
        let (view, full_chars) = super::body_match::matched_view(&raw_body, query);
        let (body, body_page) = window_body(
            &view,
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
            "bot": super::util::is_bot(&review).then_some(true),
            "state": string(review.get("state")),
            "body": (!body.is_empty()).then_some(body),
            "fullBodyChars": full_chars,
            "bodyPagination": (!raw_body.is_empty() && (gh_get_history_item_query::char_offset(query).unwrap_or(0)>0 || body_page["hasMore"]==true)).then_some(body_page),
            "submittedAt": utc_value(review.get("submitted_at")), "commitSha": review.get("commit_id")
        });
        remove_nulls(&mut item);
        shaped.push(item);
    }
    if let Some(Value::Array(comments)) = row.get_mut("reviewComments") {
        for comment in comments {
            let named = comment
                .get("reviewId")
                .and_then(Value::as_str)
                .is_some_and(|id| folded.iter().any(|folded| folded == id));
            if !named {
                drop_review_id(comment);
            }
        }
    }
    row["reviews"] = Value::Array(shaped);
    pagination.insert(Axis::Reviews.key().into(), page);
    folded.len()
}

/// The disclosure of `folded` reviews left out of `reviews`: each one's
/// inline comments name it, so the list never reads as all the reviews.
pub(super) fn folded_reviews_note(folded: usize) -> Option<String> {
    (folded > 0).then(|| {
        format!(
            "{} with no body (COMMENTED) not listed in reviews: the reviewComments naming each one (reviewId) hold what it said.",
            octocode_engine::text::counted(
                u64::try_from(folded).unwrap_or(u64::MAX),
                "review",
                "reviews"
            )
        )
    })
}

/// Shape one page of PR commits. With `commitFiles` each commit lists its
/// changed paths in the caller's file scope (inventory rows, no patches).
/// A commit with more matching files than one page continues on the row's `next`
/// (`continueCommitFiles`, `continueCommitFiles2`, …: that commit's next file
/// page), never in a list nested in the commit. Returns those
/// continuations.
pub(super) async fn shape_pr_commits(
    source: &Source<'_>,
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    commits: Vec<Value>,
    state: WindowState,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<Vec<(String, Value)>, ProviderError> {
    let include_files = query.sections().has(Section::CommitFiles);
    let scope = FileScope::from_query(query).map_err(|message| validation(&message))?;
    let filter = FileFilter {
        needle: None,
        scope: scope.as_ref(),
    };
    let (slice, page) = state.paginate(
        commits,
        gh_get_history_item_query::page(query),
        Some(gh_get_history_item_query::collection_page_size(query)),
    );
    // Each commit's file window runs through shared admission and deadline;
    // a few commits load at once, preserving the commit page's order.
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
                let per_page = MAX_PER_PAGE;
                let (files, mut files_page) =
                    commit_files::first_page(source, query, &sha, filter, per_page, context)
                        .await?;
                commit["files"] = Value::Array(compact_inventory(&files));
                // The cursor rides the continuation, never the block.
                if let Some(next_page) = files_page
                    .as_object_mut()
                    .and_then(|page| page.remove("nextPage"))
                    .and_then(|page| page.as_u64())
                {
                    more_files = commit_files::next_page(query, &sha, next_page, per_page);
                }
                if more_files.is_some() || files_page["terminalLimit"] == true {
                    commit["sectionPagination"][Axis::Files.key()] = files_page;
                }
            }
            Ok::<_, ProviderError>((commit, more_files))
        })
        .buffered(COMMIT_DETAIL_CONCURRENCY)
        .try_collect()
        .await?;
    let mut continuations = Vec::new();
    let mut commits = Vec::with_capacity(shaped.len());
    for (mut commit, more_files) in shaped {
        if let Some(page) = commit.pointer_mut("/sectionPagination/files") {
            lift_page_limits(row, page);
        }
        // A commit with nothing beside its row (no body, no files) is one line.
        commits.push(commit::line(commit));
        if let Some(read) = more_files {
            let name =
                octocode_runtime_base::id::numbered("continueCommitFiles", continuations.len());
            continuations.push((name, read));
        }
    }
    row["commits"] = Value::Array(commits);
    pagination.insert(Axis::Commits.key().into(), page);
    Ok(continuations)
}

#[cfg(test)]
mod tests;
