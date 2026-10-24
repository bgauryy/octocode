//! Pull-request comment, review and commit sections.
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
    assert_eq!(read["kind"], "related", "{read}");
    let q = &read["input"]["queries"][0];
    assert_eq!(q["ref"], "head1", "{read}");
    assert_eq!(
        q["ranges"],
        json!([{"startLine":2770,"endLine":2779}]),
        "{read}"
    );
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
    let q = &read["input"]["queries"][0];
    assert_eq!(q["path"], "src/a.ts", "{read}");
    assert_eq!(q["ref"], "h", "{read}");
    assert_eq!(
        q["ranges"],
        json!([{"startLine":1,"endLine":5},{"startLine":37,"endLine":48}]),
        "{read}"
    );
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
        "sections":["comments"],"offset":12_000
    }))
    .expect("pull request row");
    let mut row = json!({});
    let mut pagination = Map::new();
    let mut windows = BodyWindows::default();
    let comments = map_comments(vec![
        raw(json!({"id":1,"body":"short"})),
        raw(json!({"id":2,"body":"x".repeat(24_005)})),
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
    assert_eq!(listed[0]["body"], "x".repeat(12_000), "{row}");
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
