//! Page continuations: comment, compare, issue and file pages carry their own cursor and pinned grid, and only unreached axes are partial.
use super::*;

/// A cut comment body continues with the one text offset (`offset`),
/// on the same comment page, reading only comments: every pull-request
/// text surface windows with `offset`, as an issue's do.
#[test]
fn a_cut_comment_body_continues_with_offset_on_its_comment_page() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","owner":"a","repo":"b","number":1,
        "sections":["body","comments"],"page":2
    }))
    .expect("pr query");
    let mut out = json!({"pullRequests":[{"contentPagination":{
        "commentBody":{"hasMore":true,"nextOffset":500}
    }}]});
    promote_pr_continuations(&mut out, &query, &Remainder::default());
    let next = &out["calls"]["continueCommentBody"]["input"]["queries"][0];
    assert_eq!(next["offset"], 500, "{out}");
    assert_eq!(next["page"], 2, "{out}");
    assert_eq!(next["sections"], json!(["comments"]), "{next}");
    assert!(next.get("commentOffset").is_none(), "{next}");
}

/// Compare: each page has one canonical query. The commit page drops
/// every file cursor; the file page restarts the patch offset and follows
/// the last patch window; a patch hop re-offers neither.
#[test]
fn compare_pages_are_offered_once_with_their_own_cursor_only() {
    let compare = |fields: Value| {
        HistoryItemRequest::from_row(crate::tools::gh_get_history_item::util::merge(
            json!({"operation":"compare","owner":"a","repo":"b","baseRef":"x","headRef":"y",
                "sections":["patches"]}),
            fields,
        ))
        .expect("compare query")
    };
    let mut first = json!({
        "commits":[{"sha":"1"}],
        "files":[{"path":"a.rs","patch":"x"}],
        "contentPagination":{"commits":{"page":1,"hasMore":true,"nextPage":2},
            "files":{"page":1,"hasMore":true,"nextPage":2}}
    });
    let cursors = DiffCursors {
        commit_page: Some(2),
        ..DiffCursors::of_file_page(&mut first["contentPagination"]["files"], Some(5000), true)
    };
    attach_diff_continuations(
        &mut first,
        &compare(json!({})),
        ItemOperation::Compare,
        None,
        cursors,
    );
    let next = first["calls"].as_object().expect("next");
    assert!(next.get("nextFilePage").is_none(), "{first}");
    let commits = &first["calls"]["nextPage"]["input"]["queries"][0];
    for key in ["page", "offset", "length"] {
        assert!(commits.get(key).is_none(), "{key}: {commits}");
    }
    assert_eq!(
        first["calls"]["continuePatch"]["input"]["queries"][0]["offset"],
        5000
    );

    let mut last = json!({
        "files":[{"path":"b.rs","patch":"y"}],
        "contentPagination":{"files":{"page":1,"hasMore":true,"nextPage":2}}
    });
    let cursors = DiffCursors::of_file_page(&mut last["contentPagination"]["files"], None, true);
    attach_diff_continuations(
        &mut last,
        &compare(json!({"offset":5000})),
        ItemOperation::Compare,
        None,
        cursors,
    );
    let names = last["calls"]
        .as_object()
        .map(|next| next.keys().cloned().collect::<Vec<_>>());
    assert_eq!(names, Some(vec!["nextFilePage".to_owned()]), "{last}");
    let files = &last["calls"]["nextFilePage"]["input"]["queries"][0];
    assert_eq!(files["page"], 2, "{files}");
    assert!(files.get("offset").is_none(), "{files}");
}

#[test]
fn char_offset_continuations_narrow_content_to_their_own_surface() {
    // `offset` is one shared field: continuing the body must not skew
    // review bodies or patches by the body offset (and vice versa).
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest", "owner":"o","repo":"r","number":5,
        "sections":["body","patches","comments","reviews"],
        "offset":0
    }))
    .expect("query");
    let mut out = json!({"pullRequests":[{"contentPagination":{
        "body":{"hasMore":true,"nextOffset":12000},
        "reviewBody":{"hasMore":true,"nextOffset":300},
        "patches":{"hasMore":true,"nextOffset":900},
        "comments":{"hasMore":true,"nextPage":2}
    }}]});
    promote_pr_continuations(&mut out, &query, &Remainder::default());
    let next = &out["calls"];
    assert_eq!(
        next["continueBody"]["input"]["queries"][0]["sections"],
        json!(["body"])
    );
    assert_eq!(next["continueBody"]["input"]["queries"][0]["offset"], 12000);
    assert_eq!(
        next["continueReviewBody"]["input"]["queries"][0]["sections"],
        json!(["reviews"])
    );
    assert_eq!(
        next["continuePatch"]["input"]["queries"][0]["sections"],
        json!(["patches"])
    );
    assert!(
        next["continuePatch"]["input"]["queries"][0]
            .get("content")
            .is_none()
    );
    let comments = &next["nextCommentPage"]["input"]["queries"][0];
    assert!(comments.get("offset").is_none(), "{comments}");
    assert_eq!(comments["page"], 2);
}

/// Issue pages use the published `include` spelling.
#[test]
fn issue_pages_use_the_published_include_spelling() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"issue","owner":"o","repo":"r","number":3,
        "sections":["body","comments"]
    }))
    .expect("issue query");
    let mut out = json!({"issues":[{"number":3,"contentPagination":{
        "body":{"hasMore":true,"nextOffset":12000},
        "comments":{"hasMore":true,"nextPage":2}
    }}]});
    promote_issue_continuations(&mut out, &query, &Remainder::default());
    let body = &out["calls"]["continueBody"]["input"]["queries"][0];
    assert_eq!(body["sections"], json!(["body"]), "{out}");
    assert!(body.get("content").is_none(), "{out}");
    let comments = &out["calls"]["nextCommentPage"]["input"]["queries"][0];
    assert_eq!(comments["sections"], json!(["comments"]), "{out}");
    assert!(comments.get("content").is_none(), "{out}");
}

/// A budget-sized file page (no `pageSize`, no patches) carries the page
/// size its grid was cut on in `nextFilePage`, so the continuation reads
/// the same grid under any batch share; a caller's `pageSize` and a patch
/// page (fixed batch) carry nothing new. `readPatches` from an inventory
/// page opens the patch page that holds the page's first file.
#[test]
fn budget_sized_file_pages_pin_their_grid_on_the_continuation() {
    let request = |row: Value, share: usize| {
        let mut query = HistoryItemRequest::from_row(row).expect("query");
        query.auto_page_chars = Some(share);
        query
    };
    let pr_next = |query: &HistoryItemRequest| {
        let mut out = json!({"pullRequests":[{"contentPagination":{
            "files":{"hasMore":true,"nextPage":2}
        }}]});
        promote_pr_continuations(&mut out, query, &Remainder::default());
        out["calls"]["nextFilePage"]["input"]["queries"][0].clone()
    };
    let pr = |fields: Value| {
        crate::tools::gh_get_history_item::util::merge(
            json!({"operation":"pullRequest","owner":"a","repo":"b","number":1}),
            fields,
        )
    };
    let alone = pr_next(&request(pr(json!({"sections":["files"]})), 20_000));
    let batched = pr_next(&request(pr(json!({"sections":["files"]})), 10_000));
    assert_eq!(alone["pageSize"], 333, "{alone}");
    assert_eq!(batched["pageSize"], 166, "{batched}");
    // Replayed in another batch, the pinned size is the caller's now.
    let replay = request(batched.clone(), 20_000);
    assert_eq!(
        crate::tools::gh_get_history_item::inventory::file_page_size(&replay, false),
        166
    );
    assert_eq!(pr_next(&replay)["pageSize"], 166);
    let patches = pr_next(&request(pr(json!({"sections":["patches"]})), 10_000));
    assert!(patches.get("pageSize").is_none(), "{patches}");
    let sized = pr_next(&request(
        pr(json!({"sections":["files"],"pageSize":50})),
        10_000,
    ));
    assert_eq!(sized["pageSize"], 50, "{sized}");

    let compare = |page: u64, share: usize| {
        let query = request(
            json!({"operation":"compare","owner":"a","repo":"b","baseRef":"x","headRef":"y","page":page}),
            share,
        );
        let mut out = json!({"files":["M +1 -1 a.rs"],
            "contentPagination":{"files":{"page":page,"hasMore":true,"nextPage":page + 1}}});
        let cursors = DiffCursors::of_file_page(&mut out["contentPagination"]["files"], None, true);
        attach_diff_continuations(&mut out, &query, ItemOperation::Compare, None, cursors);
        out
    };
    let out = compare(2, 10_000);
    let files = &out["calls"]["nextFilePage"]["input"]["queries"][0];
    assert_eq!(
        (&files["page"], &files["pageSize"]),
        (&json!(3), &json!(166)),
        "{out}"
    );
    // Optional patches start a fresh request; the inventory page grid is private.
    let patches = &out["calls"]["readPatches"]["input"]["queries"][0];
    assert!(patches.get("page").is_none(), "{out}");
    assert_eq!(out["calls"]["readPatches"]["kind"], "related");
    assert!(patches.get("pageSize").is_none(), "{out}");
    let first = compare(1, 10_000);
    let patches = &first["calls"]["readPatches"]["input"]["queries"][0];
    assert!(patches.get("page").is_none(), "{first}");
}

/// A page block's cursor rides the `next` continuation it was read into,
/// never the block, and `hasMore` beside that continuation already says the
/// rest is unread: `isPartial` is set only for an axis no continuation
/// reaches.
#[test]
fn cursors_ride_next_and_is_partial_marks_only_unreached_axes() {
    let pr = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","owner":"a","repo":"b","number":1,
        "sections":["body","files"]
    }))
    .expect("query");
    let mut out = json!({"pullRequests":[{"contentPagination":{
        "body":{"offset":0,"length":10,"total":30,"hasMore":true,"nextOffset":10},
        "files":{"page":1,"hasMore":true,"nextPage":2}
    }}]});
    promote_pr_continuations(&mut out, &pr, &Remainder::default());
    assert!(out.get("isPartial").is_none(), "{out}");
    assert_eq!(
        out["calls"]["nextFilePage"]["input"]["queries"][0]["page"],
        2
    );
    assert_eq!(
        out["calls"]["continueBody"]["input"]["queries"][0]["offset"],
        10
    );
    let pages = &out["pullRequests"][0]["contentPagination"];
    assert!(pages["body"].get("nextOffset").is_none(), "{out}");
    assert!(pages["files"].get("nextPage").is_none(), "{out}");
    assert_eq!(pages["files"]["hasMore"], true, "{out}");
    // More to read and no cursor: nothing reaches it.
    let mut out = json!({"pullRequests":[{"contentPagination":{
        "files":{"page":1,"hasMore":true}
    }}]});
    promote_pr_continuations(&mut out, &pr, &Remainder::default());
    assert_eq!(out["isPartial"], true, "{out}");
    assert!(out.get("calls").is_none(), "{out}");

    let issue = HistoryItemRequest::from_row(json!({
        "operation":"issue","owner":"a","repo":"b","number":1,"sections":["comments"]
    }))
    .expect("query");
    let mut out = json!({"issues":[{"contentPagination":{
        "comments":{"page":1,"hasMore":true,"nextPage":2}
    }}]});
    promote_issue_continuations(&mut out, &issue, &Remainder::default());
    assert!(out.get("isPartial").is_none(), "{out}");
    assert_eq!(
        out["calls"]["nextCommentPage"]["input"]["queries"][0]["page"],
        2
    );
    assert!(
        out["issues"][0]["contentPagination"]["comments"]
            .get("nextPage")
            .is_none(),
        "{out}"
    );
}
