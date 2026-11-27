//! Page continuations: comment, compare, issue and file pages carry their own cursor and pinned grid, and only unreached axes are partial.
use super::*;
use octocode_test_support::json_fixture;

fn diff_page<'a>(out: &'a Value, axis: &str) -> &'a Value {
    &out["next"][axis]
}

fn row_page<'a>(out: &'a Value, row: &str, axis: &str) -> &'a Value {
    &out[row][0]["sectionPagination"][axis]["next"]["nextPage"]
}

fn pr_page<'a>(out: &'a Value, axis: &str) -> &'a Value {
    row_page(out, "pullRequests", axis)
}

/// A cut comment body continues with the one text offset (`offset`),
/// on the same comment page, reading only comments: every pull-request
/// text surface windows with `offset`, as an issue's do.
#[test]
fn a_cut_comment_body_continues_with_offset_on_its_comment_page() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "a",
            "repo": "b",
            "number": 1,
            "sections": ["body", "comments"],
            "page": 2
        }"#,
    ))
    .expect("pr query");
    let mut out = json_fixture(
        r#"{
            "pullRequests": [
                {
                    "sectionPagination": {"commentBody": {"hasMore": true, "nextOffset": 500}}
                }
            ]
        }"#,
    );
    promote_pr_continuations(&mut out, &query);
    let next = &pr_page(&out, "commentBody")["input"]["queries"][0];
    assert_eq!(next["offset"], 500, "{out}");
    assert_eq!(next["page"], 2, "{out}");
    assert_eq!(next["sections"], json_fixture(r#"["comments"]"#), "{next}");
    assert!(next.get("commentOffset").is_none(), "{next}");
}

/// Compare: each page has one canonical query. The commit page drops
/// every file cursor; the file page restarts the patch offset and follows
/// the last patch window; a patch hop re-offers neither.
#[test]
fn compare_pages_are_offered_once_with_their_own_cursor_only() {
    let compare = |fields: Value| {
        HistoryItemRequest::from_row(crate::gh_get_history_item::util::merge(
            json_fixture(
                r#"{
                    "operation": "compare",
                    "owner": "a",
                    "repo": "b",
                    "baseRef": "x",
                    "headRef": "y",
                    "sections": ["patches"]
                }"#,
            ),
            fields,
        ))
        .expect("compare query")
    };
    let mut first = json_fixture(
        r#"{
            "commits": [{"sha": "1"}],
            "files": [{"path": "a.rs", "patch": "x"}],
            "sectionPagination": {
                "commits": {"page": 1, "hasMore": true, "nextPage": 2},
                "files": {"page": 1, "hasMore": true, "nextPage": 2}
            }
        }"#,
    );
    let cursors = DiffCursors {
        commit_page: Some(2),
        ..DiffCursors::of_file_page(&mut first["sectionPagination"]["files"], Some(5000))
    };
    attach_diff_continuations(
        &mut first,
        &compare(json!({})),
        ItemOperation::Compare,
        None,
        cursors,
    );
    assert!(diff_page(&first, "files").is_null(), "{first}");
    let commits = &diff_page(&first, "commits")["input"]["queries"][0];
    assert_eq!(commits["page"], 2, "{commits}");
    assert_eq!(
        commits["sections"],
        json_fixture(r#"["commits"]"#),
        "{commits}"
    );
    for key in ["offset", "length"] {
        assert!(commits.get(key).is_none(), "{key}: {commits}");
    }
    assert_eq!(
        diff_page(&first, "patches")["input"]["queries"][0]["offset"],
        5000
    );

    let mut last = json_fixture(
        r#"{
            "files": [{"path": "b.rs", "patch": "y"}],
            "sectionPagination": {"files": {"page": 1, "hasMore": true, "nextPage": 2}}
        }"#,
    );
    let cursors = DiffCursors::of_file_page(&mut last["sectionPagination"]["files"], None);
    attach_diff_continuations(
        &mut last,
        &compare(json_fixture(r#"{"offset": 5000}"#)),
        ItemOperation::Compare,
        None,
        cursors,
    );
    let files = &diff_page(&last, "files")["input"]["queries"][0];
    assert!(files.is_object(), "{last}");
    assert_eq!(files["page"], 2, "{files}");
    assert!(files.get("offset").is_none(), "{files}");
}

#[test]
fn char_offset_continuations_narrow_content_to_their_own_surface() {
    // `offset` is one shared field: continuing the body must not skew
    // review bodies or patches by the body offset (and vice versa).
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "number": 5,
            "sections": ["body", "patches", "comments", "reviews"],
            "offset": 0
        }"#,
    ))
    .expect("query");
    let mut out = json_fixture(
        r#"{
            "pullRequests": [
                {
                    "sectionPagination": {
                        "body": {"hasMore": true, "nextOffset": 12000},
                        "reviewBody": {"hasMore": true, "nextOffset": 300},
                        "patches": {"hasMore": true, "nextOffset": 900},
                        "comments": {"hasMore": true, "nextPage": 2}
                    }
                }
            ]
        }"#,
    );
    promote_pr_continuations(&mut out, &query);
    let next = &out["pullRequests"][0]["sectionPagination"];
    assert_eq!(
        next["body"]["next"]["nextPage"]["input"]["queries"][0]["sections"],
        json_fixture(r#"["body"]"#)
    );
    assert_eq!(
        next["body"]["next"]["nextPage"]["input"]["queries"][0]["offset"],
        12000
    );
    assert_eq!(
        next["reviewBody"]["next"]["nextPage"]["input"]["queries"][0]["sections"],
        json_fixture(r#"["reviews"]"#)
    );
    assert_eq!(
        next["patches"]["next"]["nextPage"]["input"]["queries"][0]["sections"],
        json_fixture(r#"["patches"]"#)
    );
    assert!(
        next["patches"]["next"]["nextPage"]["input"]["queries"][0]
            .get("content")
            .is_none()
    );
    let comments = &next["comments"]["next"]["nextPage"]["input"]["queries"][0];
    assert!(comments.get("offset").is_none(), "{comments}");
    assert_eq!(comments["page"], 2);
}

/// Issue pages use the published `include` spelling.
#[test]
fn issue_pages_use_the_published_include_spelling() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "issue",
            "owner": "o",
            "repo": "r",
            "number": 3,
            "sections": ["body", "comments"]
        }"#,
    ))
    .expect("issue query");
    let mut out = json_fixture(
        r#"{
            "issues": [
                {
                    "number": 3,
                    "sectionPagination": {
                        "body": {"hasMore": true, "nextOffset": 12000},
                        "comments": {"hasMore": true, "nextPage": 2}
                    }
                }
            ]
        }"#,
    );
    promote_issue_continuations(&mut out, &query, &Remainder::default());
    let body = &row_page(&out, "issues", "body")["input"]["queries"][0];
    assert_eq!(body["sections"], json_fixture(r#"["body"]"#), "{out}");
    assert!(body.get("content").is_none(), "{out}");
    let comments = &row_page(&out, "issues", "comments")["input"]["queries"][0];
    assert_eq!(
        comments["sections"],
        json_fixture(r#"["comments"]"#),
        "{out}"
    );
    assert!(comments.get("content").is_none(), "{out}");
}

/// A budget-sized file page (no `pageSize`, no patches) carries the page
/// size its grid was cut on in the file page continuation, so the continuation reads
/// the same grid under any batch share; a caller's `pageSize` and a patch
/// page (fixed batch) carry nothing new.
#[test]
fn budget_sized_file_pages_pin_their_grid_on_the_continuation() {
    let request = |row: Value, share: usize| {
        let mut query = HistoryItemRequest::from_row(row).expect("query");
        query.auto_page_chars = Some(share);
        query
    };
    let pr_next = |query: &HistoryItemRequest| {
        let mut out = json_fixture(
            r#"{
                "pullRequests": [
                    {
                        "sectionPagination": {"files": {"hasMore": true, "nextPage": 2}}
                    }
                ]
            }"#,
        );
        promote_pr_continuations(&mut out, query);
        pr_page(&out, "files")["input"]["queries"][0].clone()
    };
    let pr = |fields: Value| {
        crate::gh_get_history_item::util::merge(
            json_fixture(
                r#"{
                    "operation": "pullRequest",
                    "owner": "a",
                    "repo": "b",
                    "number": 1
                }"#,
            ),
            fields,
        )
    };
    let alone = pr_next(&request(
        pr(json_fixture(r#"{"sections": ["files"]}"#)),
        20_000,
    ));
    let batched = pr_next(&request(
        pr(json_fixture(r#"{"sections": ["files"]}"#)),
        10_000,
    ));
    assert_eq!(alone["pageSize"], 333, "{alone}");
    assert_eq!(batched["pageSize"], 166, "{batched}");
    // Replayed in another batch, the pinned size is the caller's now.
    let replay = request(batched.clone(), 20_000);
    assert_eq!(
        crate::gh_get_history_item::inventory::file_page_size(&replay, false),
        166
    );
    assert_eq!(pr_next(&replay)["pageSize"], 166);
    let patches = pr_next(&request(
        pr(json_fixture(r#"{"sections": ["patches"]}"#)),
        10_000,
    ));
    assert!(patches.get("pageSize").is_none(), "{patches}");
    let sized = pr_next(&request(
        pr(json_fixture(r#"{"sections": ["files"], "pageSize": 50}"#)),
        10_000,
    ));
    assert_eq!(sized["pageSize"], 50, "{sized}");

    let compare = |page: u64, share: usize| {
        let query = request(
            json!({"operation":"compare","owner":"a","repo":"b","baseRef":"x","headRef":"y","page":page}),
            share,
        );
        let mut out = json!({"files":["M +1 -1 a.rs"],
            "sectionPagination":{"files":{"page":page,"hasMore":true,"nextPage":page + 1}}});
        let cursors = DiffCursors::of_file_page(&mut out["sectionPagination"]["files"], None);
        attach_diff_continuations(&mut out, &query, ItemOperation::Compare, None, cursors);
        out
    };
    let out = compare(2, 10_000);
    let files = &diff_page(&out, "files")["input"]["queries"][0];
    assert_eq!(
        (&files["page"], &files["pageSize"]),
        (&json!(3), &json!(166)),
        "{out}"
    );
    // Patches are a `sections` value away: no static lead restates the query.
    assert!(out["calls"].get("readPatches").is_none(), "{out}");
}

/// A page block's cursor rides the `next` continuation it was read into,
/// never the block, and `hasMore` beside that continuation already says the
/// rest is unread: `isPartial` is set only for an axis no continuation
/// reaches.
#[test]
fn cursors_ride_next_and_is_partial_marks_only_unreached_axes() {
    let pr = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "a",
            "repo": "b",
            "number": 1,
            "sections": ["body", "files"]
        }"#,
    ))
    .expect("query");
    let mut out = json_fixture(
        r#"{
            "pullRequests": [
                {
                    "sectionPagination": {
                        "body": {
                            "offset": 0,
                            "length": 10,
                            "total": 30,
                            "hasMore": true,
                            "nextOffset": 10
                        },
                        "files": {"page": 1, "hasMore": true, "nextPage": 2}
                    }
                }
            ]
        }"#,
    );
    promote_pr_continuations(&mut out, &pr);
    assert!(out.get("isPartial").is_none(), "{out}");
    assert_eq!(pr_page(&out, "files")["input"]["queries"][0]["page"], 2);
    assert_eq!(pr_page(&out, "body")["input"]["queries"][0]["offset"], 10);
    let pages = &out["pullRequests"][0]["sectionPagination"];
    assert!(pages["body"].get("nextOffset").is_none(), "{out}");
    assert!(pages["files"].get("nextPage").is_none(), "{out}");
    assert_eq!(pages["files"]["hasMore"], true, "{out}");
    // More to read and no cursor: nothing reaches it.
    let mut out = json_fixture(
        r#"{
            "pullRequests": [
                {
                    "sectionPagination": {"files": {"page": 1, "hasMore": true}}
                }
            ]
        }"#,
    );
    promote_pr_continuations(&mut out, &pr);
    assert_eq!(out["isPartial"], true, "{out}");
    assert!(pr_page(&out, "files").is_null(), "{out}");

    let issue = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "issue",
            "owner": "a",
            "repo": "b",
            "number": 1,
            "sections": ["comments"]
        }"#,
    ))
    .expect("query");
    let mut out = json_fixture(
        r#"{
            "issues": [
                {
                    "sectionPagination": {"comments": {"page": 1, "hasMore": true, "nextPage": 2}}
                }
            ]
        }"#,
    );
    promote_issue_continuations(&mut out, &issue, &Remainder::default());
    assert!(out.get("isPartial").is_none(), "{out}");
    assert_eq!(
        row_page(&out, "issues", "comments")["input"]["queries"][0]["page"],
        2
    );
    assert!(
        out["issues"][0]["sectionPagination"]["comments"]
            .get("nextPage")
            .is_none(),
        "{out}"
    );
}

/// A list's next page comes with the last window of the text its page cuts:
/// while a comment body or a review body has more windows, its comment,
/// review-comment or review page is not offered (nor marked unreached); the
/// window that finishes it offers that page, once.
#[test]
fn a_comment_page_follows_its_last_body_window() {
    let issue = |offset: Option<u64>| {
        let mut row = json_fixture(
            r#"{
                "operation": "issue",
                "owner": "o",
                "repo": "r",
                "number": 3,
                "sections": ["comments"]
            }"#,
        );
        if let Some(offset) = offset {
            row["offset"] = json!(offset);
        }
        HistoryItemRequest::from_row(row).expect("issue query")
    };
    let issue_page = |body_more: bool| {
        json!({"issues": [{"sectionPagination": {
            "commentBody": if body_more {
                json!({"hasMore": true, "nextOffset": 12000})
            } else {
                json!({"hasMore": false})
            },
            "comments": {"hasMore": true, "nextPage": 2}
        }}]})
    };
    let mut cutting = issue_page(true);
    promote_issue_continuations(&mut cutting, &issue(None), &Remainder::default());
    assert!(
        row_page(&cutting, "issues", "comments").is_null(),
        "{cutting}"
    );
    assert!(
        row_page(&cutting, "issues", "commentBody").is_object(),
        "{cutting}"
    );
    assert!(cutting.get("isPartial").is_none(), "{cutting}");
    let mut finishing = issue_page(false);
    promote_issue_continuations(&mut finishing, &issue(Some(12000)), &Remainder::default());
    let comments = &row_page(&finishing, "issues", "comments")["input"]["queries"][0];
    assert_eq!(comments["page"], 2, "{finishing}");
    assert!(comments.get("offset").is_none(), "{finishing}");

    let pr = |offset: Option<u64>| {
        let mut row = json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "o",
                "repo": "r",
                "number": 5,
                "sections": ["comments", "reviewComments", "reviews"]
            }"#,
        );
        if let Some(offset) = offset {
            row["offset"] = json!(offset);
        }
        HistoryItemRequest::from_row(row).expect("pr query")
    };
    let pr_out = |comment_body_more: bool, review_body_more: bool| {
        let text = |more: bool| {
            if more {
                json!({"hasMore": true, "nextOffset": 12000})
            } else {
                json!({"hasMore": false})
            }
        };
        json!({"pullRequests": [{"sectionPagination": {
            "commentBody": text(comment_body_more),
            "reviewBody": text(review_body_more),
            "comments": {"hasMore": true, "nextPage": 2},
            "reviewComments": {"hasMore": true, "nextPage": 2},
            "reviews": {"hasMore": true, "nextPage": 2}
        }}]})
    };
    let mut cutting = pr_out(true, true);
    promote_pr_continuations(&mut cutting, &pr(None));
    for list in ["comments", "reviewComments", "reviews"] {
        assert!(pr_page(&cutting, list).is_null(), "{list}: {cutting}");
    }
    for text in ["commentBody", "reviewBody"] {
        assert!(pr_page(&cutting, text).is_object(), "{text}: {cutting}");
    }
    assert!(cutting.get("isPartial").is_none(), "{cutting}");
    let mut finishing = pr_out(false, false);
    promote_pr_continuations(&mut finishing, &pr(Some(12000)));
    for list in ["comments", "reviewComments", "reviews"] {
        assert_eq!(
            pr_page(&finishing, list)["input"]["queries"][0]["page"],
            2,
            "{list}: {finishing}"
        );
    }
}
