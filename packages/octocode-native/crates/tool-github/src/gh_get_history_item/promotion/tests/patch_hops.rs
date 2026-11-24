//! Patch continuations: `patch continuation` and file-page hops follow the page-stream cursor and offer whole patches a narrowed view hid.
use super::*;
use octocode_test_support::json_fixture;

fn diff_page<'a>(out: &'a Value, axis: &str) -> &'a Value {
    &out["next"][axis]
}

fn pr_page<'a>(out: &'a Value, axis: &str) -> &'a Value {
    &out["pullRequests"][0]["sectionPagination"][axis]["next"]["nextPage"]
}

/// A commit's `patch continuation` copies the page-stream cursor named on
/// `sectionPagination.files`, never a file's own per-file `nextOffset`.
#[test]
fn commit_continue_patch_copies_the_page_stream_cursor() {
    let query = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "commit",
            "owner": "a",
            "repo": "b",
            "ref": "abc",
            "sections": ["patches"]
        }"#,
    ))
    .expect("commit query");
    let mut out = json_fixture(
        r#"{
            "files": [
                {"filename": "a.rs", "patch": "aaa"},
                {
                    "filename": "b.rs",
                    "patch": "bbbbbbb",
                    "patchPagination": {
                        "unit": "chars",
                        "offset": 0,
                        "length": 7,
                        "total": 20,
                        "hasMore": true,
                        "nextOffset": 7
                    }
                }
            ],
            "sectionPagination": {"files": {"page": 1, "hasMore": false}}
        }"#,
    );
    let cursors = DiffCursors::of_file_page(&mut out["sectionPagination"]["files"], Some(10));
    attach_diff_continuations(
        &mut out,
        &query,
        ItemOperation::Commit,
        Some("abc"),
        cursors,
    );
    let next = &diff_page(&out, "patches")["input"]["queries"][0];
    assert_eq!(next["offset"], 10, "{out}");
    assert!(next.get("length").is_none());
}

/// A patch-walk hop (`patch continuation`, the next file page) runs
/// at the same automatic page as the first call: it carries no
/// `responseChars`/`responseScope` knob (automatic paging already
/// splits an oversized row by whole rows), and needs no offset marker.
#[test]
fn history_b_patch_walk_hops_carry_no_response_page_knobs() {
    let lead = |fields: Value, page: Option<usize>| {
        let mut row = json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "a",
                "repo": "b",
                "number": 1,
                "sections": ["patches"]
            }"#,
        );
        for (key, value) in fields.as_object().into_iter().flatten() {
            row[key] = value.clone();
        }
        let mut query = HistoryItemRequest::from_row(row).expect("query");
        query.auto_page_chars = page;
        let mut out = json_fixture(
            r#"{
                "pullRequests": [
                    {
                        "sectionPagination": {"patches": {"hasMore": true, "nextOffset": 900}}
                    }
                ]
            }"#,
        );
        promote_pr_continuations(&mut out, &query);
        pr_page(&out, "patches")["input"].clone()
    };
    for hop in [
        lead(json!({}), Some(20_000)),
        lead(json_fixture(r#"{"offset": 5}"#), Some(40_000)),
        lead(json!({}), None),
    ] {
        assert!(hop.get("responseLength").is_none(), "{hop}");
        assert!(hop.get("responseScope").is_none(), "{hop}");
        assert_eq!(hop["queries"][0]["offset"], 900, "{hop}");
    }
    let hop = lead(json!({}), Some(5_000));
    assert!(hop["queries"][0].get("length").is_none(), "{hop}");
    octocode_runtime_base::contracts::validate_saved("ghGetHistoryItem", hop).expect("a valid hop");

    let mut commit = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "commit",
            "owner": "a",
            "repo": "b",
            "ref": "abc",
            "sections": ["patches"]
        }"#,
    ))
    .expect("commit query");
    commit.auto_page_chars = Some(20_000);
    let mut out = json_fixture(
        r#"{
            "sectionPagination": {"files": {"page": 1, "hasMore": false}}
        }"#,
    );
    let cursors = DiffCursors::of_file_page(&mut out["sectionPagination"]["files"], Some(10));
    attach_diff_continuations(
        &mut out,
        &commit,
        ItemOperation::Commit,
        Some("abc"),
        cursors,
    );
    let hop = &diff_page(&out, "patches")["input"];
    assert!(hop.get("responseLength").is_none(), "{out}");
    assert!(hop.get("responseScope").is_none(), "{out}");

    let file_page = |sections: Value, offset: Option<u64>| {
        let mut row = json!({
            "operation":"pullRequest",
            "owner":"a","repo":"b","number":1,"sections":sections
        });
        if let Some(offset) = offset {
            row["offset"] = json!(offset);
        }
        let mut query = HistoryItemRequest::from_row(row).expect("query");
        query.auto_page_chars = Some(40_000);
        let mut out = json_fixture(
            r#"{
                "pullRequests": [
                    {
                        "sectionPagination": {"files": {"hasMore": true, "nextPage": 2}}
                    }
                ]
            }"#,
        );
        promote_pr_continuations(&mut out, &query);
        pr_page(&out, "files")["input"].clone()
    };
    let walk = file_page(json_fixture(r#"["patches"]"#), Some(31_000));
    assert!(walk.get("responseLength").is_none(), "{walk}");
    assert!(walk.get("responseScope").is_none(), "{walk}");
    assert!(walk["queries"][0].get("offset").is_none(), "{walk}");
    assert_eq!(walk["queries"][0]["page"], 2, "{walk}");
    octocode_runtime_base::contracts::validate_built_query(
        "ghGetHistoryItem",
        walk["queries"][0].clone(),
    )
    .expect("a valid hop");
    let inventory = file_page(json_fixture(r#"["files"]"#), None);
    assert!(inventory.get("responseLength").is_none(), "{inventory}");
    assert!(inventory.get("responseScope").is_none(), "{inventory}");
    assert!(
        inventory["queries"][0].get("offset").is_none(),
        "{inventory}"
    );
}

/// A patch hop is one row query (the documented `{queries:[next.query]}`
/// form), in the published spelling: the same file page at the
/// page-stream cursor. It carries no path list and no hidden window, so
/// every hop has the page budget of the first window.
#[test]
fn patch_hops_are_the_same_file_page_at_the_stream_cursor() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "number": 5,
            "sections": ["patches"],
            "page": 2,
            "include": ["src/**"]
        }"#,
    ))
    .expect("query");
    let mut out = json_fixture(
        r#"{
            "pullRequests": [
                {
                    "sectionPagination": {"patches": {"hasMore": true, "nextOffset": 900}}
                }
            ]
        }"#,
    );
    promote_pr_continuations(&mut out, &query);
    let hop = &pr_page(&out, "patches")["input"]["queries"][0];
    assert!(hop.get("queries").is_none(), "{hop}");
    assert!(hop.get("content").is_none(), "{hop}");
    assert!(hop.get("minify").is_none(), "minify: {hop}");
    assert!(hop.get("length").is_none(), "{hop}");
    assert_eq!(hop["operation"], "pullRequest");
    assert_eq!(hop["sections"], json_fixture(r#"["patches"]"#), "{hop}");
    assert_eq!(hop["include"], json_fixture(r#"["src/**"]"#), "{hop}");
    assert_eq!(hop["page"], 2, "{hop}");
    assert_eq!(hop["offset"], 900, "{hop}");
    octocode_runtime_base::contracts::validate_built_query("ghGetHistoryItem", hop.clone())
        .expect("the patch continuation is a valid ghGetHistoryItem query");
}

/// One canonical query per page: a file page with unread patches offers
/// only `patch continuation`; the window that finishes the page's patches
/// offers the next file page at the patch stream's start.
#[test]
fn the_next_file_page_follows_the_last_patch_window() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "number": 5,
            "sections": ["patches"]
        }"#,
    ))
    .expect("query");
    let mut open = json_fixture(
        r#"{
            "pullRequests": [
                {
                    "sectionPagination": {
                        "patches": {"hasMore": true, "nextOffset": 900},
                        "files": {"hasMore": true, "nextPage": 2}
                    }
                }
            ]
        }"#,
    );
    promote_pr_continuations(&mut open, &query);
    assert!(pr_page(&open, "patches").is_object(), "{open}");
    assert!(pr_page(&open, "files").is_null(), "{open}");

    let hop: HistoryItemRequest = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "number": 5,
            "sections": ["patches"],
            "offset": 900
        }"#,
    ))
    .expect("hop");
    let mut last = json_fixture(
        r#"{
            "pullRequests": [
                {
                    "sectionPagination": {"files": {"hasMore": true, "nextPage": 2}}
                }
            ]
        }"#,
    );
    promote_pr_continuations(&mut last, &hop);
    assert!(pr_page(&last, "files").is_object(), "{last}");
    assert!(pr_page(&last, "patches").is_null(), "{last}");
    let page = &pr_page(&last, "files")["input"]["queries"][0];
    assert_eq!(page["page"], 2, "{page}");
    // The walk's next file page opens at the stream start, never at the
    // old cursor (no offset marker keeps the walk's page).
    assert!(page.get("offset").is_none(), "{page}");
    // `patches` alone already lists the page's files: the continuation
    // keeps the caller's one spelling, never `["files","patches"]`.
    assert_eq!(page["sections"], json_fixture(r#"["patches"]"#), "{page}");
}
