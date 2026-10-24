//! Patch continuations: `continuePatch` and file-page hops follow the page-stream cursor and offer whole patches a narrowed view hid.
use super::*;

/// A commit's `continuePatch` copies the page-stream cursor named on
/// `contentPagination.files`, never a file's own per-file `nextOffset`.
#[test]
fn commit_continue_patch_copies_the_page_stream_cursor() {
    let query = HistoryItemRequest::from_row(json!({
        "operation":"commit","owner":"a","repo":"b",
        "ref":"abc","sections":["patches"]
    }))
    .expect("commit query");
    let mut out = json!({
        "files":[{"filename":"a.rs","patch":"aaa"},{"filename":"b.rs","patch":"bbbbbbb",
            "patchPagination":{"unit":"chars","offset":0,"length":7,"total":20,"hasMore":true,"nextOffset":7}}],
        "contentPagination":{"files":{"page":1,"hasMore":false}}
    });
    let cursors = DiffCursors::of_file_page(&mut out["contentPagination"]["files"], Some(10), true);
    attach_diff_continuations(
        &mut out,
        &query,
        ItemOperation::Commit,
        Some("abc"),
        cursors,
    );
    let next = &out["calls"]["continuePatch"]["input"]["queries"][0];
    assert_eq!(next["offset"], 10, "{out}");
    assert!(next.get("length").is_none());
}

/// A patch-walk hop (`continuePatch`, the next file page) runs
/// at the same automatic page as the first call: it carries no
/// `responseChars`/`responseScope` knob (automatic paging already
/// splits an oversized row by whole rows), and needs no offset marker.
#[test]
fn history_b_patch_walk_hops_carry_no_response_page_knobs() {
    let lead = |fields: Value, page: Option<usize>| {
        let mut row = json!({
            "operation":"pullRequest",
            "owner":"a","repo":"b","number":1,"sections":["patches"]
        });
        for (key, value) in fields.as_object().into_iter().flatten() {
            row[key] = value.clone();
        }
        let mut query = HistoryItemRequest::from_row(row).expect("query");
        query.auto_page_chars = page;
        let mut out = json!({"pullRequests":[{"contentPagination":{
            "patches":{"hasMore":true,"nextOffset":900}
        }}]});
        promote_pr_continuations(&mut out, &query, &Remainder::default());
        out["calls"]["continuePatch"]["input"].clone()
    };
    for hop in [
        lead(json!({}), Some(20_000)),
        lead(json!({"offset":5}), Some(40_000)),
        lead(json!({}), None),
    ] {
        assert!(hop.get("responseLength").is_none(), "{hop}");
        assert!(hop.get("responseScope").is_none(), "{hop}");
        assert_eq!(hop["queries"][0]["offset"], 900, "{hop}");
    }
    let hop = lead(json!({}), Some(5_000));
    assert!(hop["queries"][0].get("length").is_none(), "{hop}");
    crate::contracts::validate_built("ghGetHistoryItem", hop).expect("a valid hop");

    let mut commit = HistoryItemRequest::from_row(json!({
        "operation":"commit","owner":"a","repo":"b",
        "ref":"abc","sections":["patches"]
    }))
    .expect("commit query");
    commit.auto_page_chars = Some(20_000);
    let mut out = json!({"contentPagination":{"files":{"page":1,"hasMore":false}}});
    let cursors = DiffCursors::of_file_page(&mut out["contentPagination"]["files"], Some(10), true);
    attach_diff_continuations(
        &mut out,
        &commit,
        ItemOperation::Commit,
        Some("abc"),
        cursors,
    );
    let hop = &out["calls"]["continuePatch"]["input"];
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
        let mut out = json!({"pullRequests":[{"contentPagination":{
            "files":{"hasMore":true,"nextPage":2}
        }}]});
        promote_pr_continuations(&mut out, &query, &Remainder::default());
        out["calls"]["nextFilePage"]["input"].clone()
    };
    let walk = file_page(json!(["patches"]), Some(31_000));
    assert!(walk.get("responseLength").is_none(), "{walk}");
    assert!(walk.get("responseScope").is_none(), "{walk}");
    assert!(walk["queries"][0].get("offset").is_none(), "{walk}");
    assert_eq!(walk["queries"][0]["page"], 2, "{walk}");
    crate::contracts::validate_built_query("ghGetHistoryItem", walk["queries"][0].clone())
        .expect("a valid hop");
    let inventory = file_page(json!(["files"]), None);
    assert!(inventory.get("responseLength").is_none(), "{inventory}");
    assert!(inventory.get("responseScope").is_none(), "{inventory}");
    assert!(
        inventory["queries"][0].get("offset").is_none(),
        "{inventory}"
    );
}

#[test]
fn match_string_views_offer_the_whole_patches_they_narrowed() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest", "owner":"a","repo":"b","number":1,
        "sections":["patches"],"matchString":["needle"],"offset":10,"page":2
    }))
    .expect("match query");
    let mut out = json!({"pullRequests":[{"files":[
        {"path":"src/a.rs","patch":"@@ -1,1 +1,1 @@\n+needle","fullPatchChars":900},
        {"path":"src/b.rs","patch":"+needle"}
    ]}]});
    attach_full_patch_continuation(&mut out, &query);
    assert_eq!(out["calls"]["readFullPatches"]["kind"], "related", "{out}");
    let next = &out["calls"]["readFullPatches"]["input"]["queries"][0];
    // The published spelling; patch hunks are never minified, so no
    // `minify` rides along.
    assert_eq!(next["sections"], json!(["patches"]), "{out}");
    assert_eq!(next["include"], json!(["src/a.rs"]), "{out}");
    for key in ["matchString", "offset", "page", "content", "minify"] {
        assert!(next.get(key).is_none(), "{key} kept: {next}");
    }
    // An explicit contextLines still clips long lines: the whole patch
    // stays reachable.
    let explicit: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest", "owner":"a","repo":"b","number":1,
        "sections":["patches"],"matchString":["needle"],"contextLines":{"before":0,"after":0}
    }))
    .expect("match query");
    let mut out = json!({"pullRequests":[{"files":[
        {"path":"src/a.rs","patch":"+needle","fullPatchChars":900}
    ]}]});
    attach_full_patch_continuation(&mut out, &explicit);
    let next = &out["calls"]["readFullPatches"]["input"]["queries"][0];
    assert_eq!(next["include"], json!(["src/a.rs"]), "{out}");
    assert!(next.get("contextLines").is_none(), "{next}");
    assert!(out["calls"].get("widenContext").is_none(), "{out}");
}

/// More reshaped files than one selection holds continue in numbered
/// reads that reach each file exactly once.
#[test]
fn reshaped_files_past_one_selection_continue_in_numbered_reads() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","owner":"a","repo":"b","number":1,
        "sections":["patches"],"matchString":["x"]
    }))
    .expect("pr query");
    let rows = (0..selected_patch_files() + 3)
        .map(|i| json!({"path":format!("src/{i}.rs"),"patch":"...","fullPatchChars":90}))
        .collect::<Vec<_>>();
    let mut out = json!({"pullRequests":[{"files":rows}]});
    attach_full_patch_continuation(&mut out, &query);
    let files = |name: &str| {
        out["calls"][name]["input"]["queries"][0]["include"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    };
    assert_eq!(
        files("readFullPatches").len(),
        selected_patch_files(),
        "{out}"
    );
    assert_eq!(
        files("readFullPatches2"),
        (selected_patch_files()..selected_patch_files() + 3)
            .map(|i| json!(format!("src/{i}.rs")))
            .collect::<Vec<_>>()
    );
    assert!(out["calls"].get("readFullPatches3").is_none(), "{out}");
}

/// Many narrowed files get the whole-patch re-read; the default
/// context already holds 10 lines, so no narrower `widenContext` view is
/// offered, and the marker never reaches default rows.
#[test]
fn many_narrowed_files_read_full_patches_without_a_narrower_view() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","owner":"a","repo":"b","number":1,
        "matchString":["miri"]
    }))
    .expect("match query");
    let rows = (0..6)
        .map(|i| json!({"path":format!("src/{i}.rs"),"patch":"+miri","fullPatchChars":90}))
        .collect::<Vec<_>>();
    let mut out = json!({"pullRequests":[{"files":rows}]});
    attach_full_patch_continuation(&mut out, &query);
    assert!(out["calls"].get("widenContext").is_none(), "{out}");
    assert_eq!(
        out["calls"]["readFullPatches"]["input"]["queries"][0]["include"]
            .as_array()
            .map(Vec::len),
        Some(6),
        "{out}"
    );
    assert!(
        out["pullRequests"][0]["files"]
            .as_array()
            .expect("files")
            .iter()
            .all(|file| file["fullPatchChars"] == 90)
    );
}

/// A patch hop is one row query (the documented `{queries:[next.query]}`
/// form), in the published spelling: the same file page at the
/// page-stream cursor. It carries no path list and no hidden window, so
/// every hop has the page budget of the first window.
#[test]
fn patch_hops_are_the_same_file_page_at_the_stream_cursor() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","owner":"o","repo":"r","number":5,"sections":["patches"],
        "page":2,"include":["src/**"]
    }))
    .expect("query");
    let mut out = json!({"pullRequests":[{"contentPagination":{
        "patches":{"hasMore":true,"nextOffset":900}
    }}]});
    let mut remainder = Remainder::default();
    remainder.unfinished_patches(2);
    let notes = promote_pr_continuations(&mut out, &query, &remainder);
    let hop = &out["calls"]["continuePatch"]["input"]["queries"][0];
    assert!(hop.get("queries").is_none(), "{hop}");
    assert!(hop.get("content").is_none(), "{hop}");
    assert!(hop.get("minify").is_none(), "minify: {hop}");
    assert!(hop.get("length").is_none(), "{hop}");
    assert_eq!(hop["operation"], "pullRequest");
    assert_eq!(hop["sections"], json!(["patches"]), "{hop}");
    assert_eq!(hop["include"], json!(["src/**"]), "{hop}");
    assert_eq!(hop["page"], 2, "{hop}");
    assert_eq!(hop["offset"], 900, "{hop}");
    assert_eq!(
        notes,
        [crate::response::result::Remaining {
            page: "continuePatch".into(),
            left: "2 unfinished patches".into()
        }]
    );
    crate::contracts::validate_built_query("ghGetHistoryItem", hop.clone())
        .expect("continuePatch is a valid ghGetHistoryItem query");
}

/// One canonical query per page: a file page with unread patches offers
/// only `continuePatch`; the window that finishes the page's patches
/// offers the next file page at the patch stream's start.
#[test]
fn the_next_file_page_follows_the_last_patch_window() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","owner":"o","repo":"r","number":5,"sections":["patches"]
    }))
    .expect("query");
    let mut open = json!({"pullRequests":[{"contentPagination":{
        "patches":{"hasMore":true,"nextOffset":900},
        "files":{"hasMore":true,"nextPage":2}
    }}]});
    promote_pr_continuations(&mut open, &query, &Remainder::default());
    let names = open["calls"]
        .as_object()
        .map(|next| next.keys().cloned().collect::<Vec<_>>());
    assert_eq!(names, Some(vec!["continuePatch".to_owned()]), "{open}");

    let hop: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","owner":"o","repo":"r","number":5,"sections":["patches"],
        "offset":900
    }))
    .expect("hop");
    let mut last = json!({"pullRequests":[{"contentPagination":{
        "files":{"hasMore":true,"nextPage":2}
    }}]});
    promote_pr_continuations(&mut last, &hop, &Remainder::default());
    let names = last["calls"]
        .as_object()
        .map(|next| next.keys().cloned().collect::<Vec<_>>());
    assert_eq!(names, Some(vec!["nextFilePage".to_owned()]), "{last}");
    let page = &last["calls"]["nextFilePage"]["input"]["queries"][0];
    assert_eq!(page["page"], 2, "{page}");
    // The walk's next file page opens at the stream start, never at the
    // old cursor (no offset marker keeps the walk's page).
    assert!(page.get("offset").is_none(), "{page}");
    // `patches` alone already lists the page's files: the continuation
    // keeps the caller's one spelling, never `["files","patches"]`.
    assert_eq!(page["sections"], json!(["patches"]), "{page}");
}
