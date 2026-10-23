//! Pull-request discussion, review and commit sections: one public page of
//! each, with per-item body windows.
use super::axis::Axis;
use super::inventory::compact_inventory;
use super::remaining::BodyWindows;
use super::text::{body_matches, delivered_earlier, needle, shape_comment, window_body};
use super::util::{array, str_at, string};
use super::window::{WindowState, paginate_window};
use super::{HistoryItemRequest, Section, fetch};
use crate::id::ToolId;
use crate::response::result::Confidence;
use crate::response::result::remove_nulls;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
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
                .map(|(start, end)| format!("{start}-{end}"))
                .collect::<Vec<_>>();
            crate::response::result::Continuation::new(
                ToolId::GhGetFileContent,
                json!({
                    "owner": gh_get_history_item_query::owner(query),
                    "repo": gh_get_history_item_query::repo(query),
                    "path": path,
                    "ref": sha,
                    "ranges": ranges,
                }),
            )
            .confidence(Confidence::Exact)
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
    crate::response::result::Continuation::new(
        ToolId::GhGetHistoryItem,
        json!({"operation":"commit","owner":gh_get_history_item_query::owner(query),"repo":gh_get_history_item_query::repo(query),"ref":sha,
            "sections":["files"],"page":page,"pageSize":page_size}),
    )
    .confidence(Confidence::Exact)
    .build()
}

#[cfg(test)]
mod tests {
    use super::super::util::map_comments;
    use super::*;

    fn review_comments() -> HistoryItemRequest {
        HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["reviewComments"]
        }))
        .expect("pull request row")
    }

    /// One page of `section` shaped from `raw` provider comments: the row
    /// and the page's code reads.
    fn shape(
        query: &HistoryItemRequest,
        section: CommentSection,
        raw: Vec<Value>,
        pagination: &mut Map<String, Value>,
    ) -> (Value, Vec<Value>) {
        let mut row = json!({});
        let shape = shape_pr_comments(
            &mut row,
            pagination,
            section,
            map_comments(raw),
            WindowState::COMPLETE,
            query,
            &mut BodyWindows::default(),
        );
        (row, shape)
    }

    fn shaped(raw: Vec<Value>) -> Vec<Value> {
        let (row, _) = shape(
            &review_comments(),
            CommentSection::ReviewInline,
            raw,
            &mut Map::new(),
        );
        array(row["reviewComments"].clone())
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
        let (_, shape) = shape(
            &review_comments(),
            CommentSection::ReviewInline,
            vec![
                raw(json!({"id":1,"line":5,"start_line":null,"commit_id":"c0","side":"LEFT"})),
                raw(json!({"id":2,"line":2776,"start_line":2773,"commit_id":"head1"})),
            ],
            &mut Map::new(),
        );
        let [read] = shape.as_slice() else {
            panic!("one code read: {shape:?}");
        };
        assert_eq!(read["tool"], "ghGetFileContent", "{read}");
        let q = &read["query"]["queries"][0];
        assert_eq!(q["ref"], "head1", "{read}");
        assert_eq!(q["ranges"], json!(["2770-2779"]), "{read}");
        assert_eq!(q["path"], "src/a.ts", "{read}");
    }

    /// One read covers every anchor of the page on the first
    /// anchor's file and commit; another file or commit stays out.
    #[test]
    fn review_comment_read_covers_every_anchor_on_its_file() {
        let (row, shape) = shape(
            &review_comments(),
            CommentSection::ReviewInline,
            vec![
                raw(json!({"id":1,"line":40,"start_line":null,"commit_id":"h"})),
                raw(json!({"id":2,"line":2,"start_line":null,"commit_id":"h"})),
                raw(json!({"id":3,"line":45,"start_line":43,"commit_id":"h"})),
                raw(json!({"id":4,"line":9,"commit_id":"h","path":"src/b.ts"})),
                raw(json!({"id":5,"line":90,"commit_id":"other"})),
            ],
            &mut Map::new(),
        );
        let [read] = shape.as_slice() else {
            panic!("one code read: {shape:?}");
        };
        let q = &read["query"]["queries"][0];
        assert_eq!(q["path"], "src/a.ts", "{read}");
        assert_eq!(q["ref"], "h", "{read}");
        assert_eq!(q["ranges"], json!(["1-5", "37-48"]), "{read}");
        // Rows carry the evidence: no summary restates their counts.
        assert_eq!(row.get("reviewSummary"), None, "{row}");
        assert_eq!(row.pointer("/reviewComments/0/commentType"), None, "{row}");
    }

    /// Inline review comments are their own section: their rows and page
    /// stand under `reviewComments`, beside the discussion's `comments`,
    /// never merged into it; only review comments lead to code.
    #[test]
    fn review_comments_and_the_discussion_are_separate_sections() {
        let query = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["comments","reviewComments"]
        }))
        .expect("pull request row");
        let mut pagination = Map::new();
        let (inline, inline_shape) = shape(
            &query,
            CommentSection::ReviewInline,
            vec![raw(json!({"id":1,"line":4,"commit_id":"h"}))],
            &mut pagination,
        );
        let mut discussion = vec![raw(json!({"id":2}))];
        discussion[0]
            .as_object_mut()
            .map(|comment| comment.remove("path"));
        let (talk, talk_shape) = shape(
            &query,
            CommentSection::Discussion,
            discussion,
            &mut pagination,
        );
        assert_eq!(
            inline.pointer("/reviewComments/0/id"),
            Some(&json!("1")),
            "{inline}"
        );
        assert_eq!(inline.get("comments"), None, "{inline}");
        assert_eq!(talk.pointer("/comments/0/id"), Some(&json!("2")), "{talk}");
        assert_eq!(talk.get("reviewComments"), None, "{talk}");
        assert_eq!(inline_shape.len(), 1);
        assert!(talk_shape.is_empty());
        assert_eq!(
            pagination.keys().collect::<Vec<_>>(),
            ["reviewComments", "comments"]
        );
    }

    /// A body continuation (`offset` > 0) re-reads the comment page
    /// to continue its cut bodies; a comment whose whole body an earlier
    /// window delivered is not listed again as an empty row.
    #[test]
    fn body_continuation_lists_only_unfinished_bodies() {
        let query = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["comments"],"offset":10,"length":10
        }))
        .expect("pull request row");
        let mut row = json!({});
        let mut pagination = Map::new();
        let mut windows = BodyWindows::default();
        let comments = map_comments(vec![
            raw(json!({"id":1,"body":"short"})),
            raw(json!({"id":2,"body":"x".repeat(25)})),
            raw(json!({"id":3,"body":"y".repeat(10)})),
            // An empty body was whole on the first window too.
            raw(json!({"id":4,"body":""})),
        ]);
        shape_pr_comments(
            &mut row,
            &mut pagination,
            CommentSection::Discussion,
            comments,
            WindowState::COMPLETE,
            &query,
            &mut windows,
        );
        let listed = array(row["comments"].clone());
        assert_eq!(listed.len(), 1, "{row}");
        assert_eq!(listed[0]["id"], "2", "{row}");
        assert_eq!(listed[0]["body"], "x".repeat(10), "{row}");
        let mut remainder = super::super::remaining::Remainder::default();
        remainder.bodies(Axis::CommentBody, windows, &mut pagination);
        assert_eq!(
            remainder
                .note(Axis::CommentBody, &pagination["commentBody"])
                .map(|note| note.left)
                .as_deref(),
            Some("5 more comment body chars"),
            "{pagination:?}"
        );
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
