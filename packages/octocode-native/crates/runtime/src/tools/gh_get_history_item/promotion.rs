//! Promotion of a shaped page's cursors (`contentPagination`) to `next.*`
//! for pull requests and issues.
use super::patch_hop::*;
use super::pr_menu::*;
use super::{HistoryItemRequest, ItemOperation};
use serde_json::{Map, Value, json};
use std::collections::HashSet;

/// A pull-request `contentPagination` axis: its `next.*` name, the page field
/// holding the cursor, and the query key the cursor continues.
pub(super) fn pr_axis(axis: &str) -> Option<(&'static str, &'static str, &'static str)> {
    Some(match axis {
        "body" => ("continueBody", "nextOffset", "offset"),
        "reviewBody" => ("continueReviewBody", "nextOffset", "offset"),
        "patches" => ("continuePatch", "nextOffset", "offset"),
        "commentBody" => ("continueCommentBody", "nextOffset", "offset"),
        "files" => ("nextFilePage", "nextPage", "filePage"),
        "comments" => ("nextCommentPage", "nextPage", "commentPage"),
        "reviews" => ("nextReviewPage", "nextPage", "reviewPage"),
        "commits" => ("nextCommitPage", "nextPage", "commitPage"),
        _ => return None,
    })
}

pub(super) fn promote_pr_continuations(out: &mut Value, q: &HistoryItemRequest) {
    let unresolved_selected_paths = selected_patch_paths(q).map(|requested| {
        let returned = out
            .pointer("/pullRequests/0/files")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|file| file.get("path").and_then(Value::as_str))
            .collect::<HashSet<_>>();
        requested
            .into_iter()
            .filter(|path| !returned.contains(path.as_str()))
            .collect::<Vec<_>>()
    });
    let Some(pages) = out
        .pointer_mut("/pullRequests/0/contentPagination")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    // A file page with unread patches continues them first: its window
    // that finishes the patches offers the next file page.
    let patches_open = pages
        .get("patches")
        .is_some_and(|entry| entry.get("hasMore").and_then(Value::as_bool) == Some(true));
    let mut next = Map::new();
    let mut partial = false;
    for (axis, entry) in pages.iter_mut() {
        if entry.get("hasMore").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        if axis == "files" && patches_open {
            partial = true;
            continue;
        }
        if axis == "files"
            && unresolved_selected_paths
                .as_ref()
                .is_some_and(Vec::is_empty)
        {
            if let Some(entry) = entry.as_object_mut() {
                entry.insert("hasMore".into(), json!(false));
                entry.remove("nextPage");
            }
            continue;
        }
        partial = true;
        let Some((name, field, key)) = pr_axis(axis) else {
            continue;
        };
        let Some(cursor) = entry.get(field).cloned() else {
            continue;
        };
        let mut nq = base_public_query(q, ItemOperation::PullRequest);
        nq[key] = cursor;
        if axis == "files"
            && let Some(unresolved) = unresolved_selected_paths.as_deref()
        {
            retain_unresolved_patch_selection(&mut nq, unresolved);
            nq["filePage"] = json!(1);
        }
        // `offset` is a single field shared by the body, review-body,
        // comment-body and patch windows. A char-window continuation
        // therefore narrows content to the surface it continues, and page
        // continuations restart every char window, so one surface's offset
        // never skews another's.
        match axis.as_str() {
            "body" => nq["content"] = json!({"body":true}),
            "reviewBody" => nq["content"] = json!({"reviews":true}),
            "commentBody" => {
                nq["content"] = json!({"comments": q
                    .content_value()
                    .and_then(|content| content.get("comments").cloned())
                    .unwrap_or(json!({"discussion":true}))});
            }
            "patches" => {
                next.insert(name.into(), patch_continuation(nq, entry, q));
                continue;
            }
            _ => {
                remove_key(&mut nq, "offset");
                // A collection page continues only its own surface; the other
                // surfaces were already delivered on this call.
                let keep: &[&str] = match axis.as_str() {
                    "files" => &["files", "patches"],
                    "comments" => &["comments"],
                    "reviews" => &["reviews"],
                    "commits" => &["commits"],
                    _ => &[],
                };
                if !keep.is_empty()
                    && let Some(content) = nq.get_mut("content").and_then(Value::as_object_mut)
                {
                    content.retain(|key, _| keep.contains(&key.as_str()));
                }
            }
        }
        let lead = menu_read(nq);
        let patch_read = q
            .content_value()
            .is_some_and(|content| content.get("patches").is_some());
        let lead = if axis == "files" && patch_read {
            patch_walk_file_page(lead, q)
        } else {
            lead
        };
        next.insert(name.into(), lead);
    }
    if partial {
        out["isPartial"] = json!(true);
        out["partialReasons"] = json!(["contentPagination"]);
        if !next.is_empty() {
            out["next"] = Value::Object(next);
        }
    }
}

/// `next.readRawBody`: the PR text surfaces whose minified view dropped text
/// (the row says `bodyView:"minified"`), re-read with `minify:"none"` from
/// their first character, on the same comment and review pages.
pub(super) fn attach_raw_body_read(out: &mut Value, q: &HistoryItemRequest, surfaces: &[&str]) {
    if surfaces.is_empty() {
        return;
    }
    let mut nq = base_public_query(q, ItemOperation::PullRequest);
    for key in [
        "offset",
        "filePage",
        "commitPage",
        "include",
        "status",
        "minChanges",
    ] {
        remove_key(&mut nq, key);
    }
    let requested = q.content_value().unwrap_or_default();
    let mut content = Map::new();
    for surface in surfaces {
        let value = match *surface {
            "comments" => requested
                .get("comments")
                .cloned()
                .unwrap_or(json!({"discussion":true})),
            _ => json!(true),
        };
        content.insert((*surface).to_owned(), value);
    }
    nq["content"] = Value::Object(content);
    nq["minify"] = json!("none");
    if !out.get("next").is_some_and(Value::is_object) {
        out["next"] = json!({});
    }
    out["next"]["readRawBody"] = menu_read(nq);
}

pub(super) fn promote_issue_continuations(out: &mut Value, q: &HistoryItemRequest) {
    let Some(pages) = out
        .pointer("/issues/0/contentPagination")
        .and_then(Value::as_object)
    else {
        return;
    };
    let content_value = q.content_value();
    let comments = || json!({"comments":content_value.as_ref().and_then(|v|v.get("comments").cloned()).unwrap_or(json!({"discussion":true}))});
    // A comment-body hop keeps the body section its first window read: the
    // hop does not show the body again, but sizes its page like that window.
    let comment_bodies = || {
        let mut content = comments();
        if content_value
            .as_ref()
            .and_then(|v| v.get("body"))
            .and_then(Value::as_bool)
            == Some(true)
        {
            content["body"] = json!(true);
        }
        content
    };
    let mut next = Map::new();
    for (axis, entry) in pages {
        if entry.get("hasMore").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let (name, key, field, content) = match axis.as_str() {
            "body" => ("continueBody", "offset", "nextOffset", json!({"body":true})),
            "commentBody" => (
                "continueCommentBody",
                "offset",
                "nextOffset",
                comment_bodies(),
            ),
            "comments" => ("nextCommentPage", "commentPage", "nextPage", comments()),
            _ => continue,
        };
        let Some(cursor) = entry.get(field) else {
            continue;
        };
        let mut nq = base_public_query(q, ItemOperation::Issue);
        nq[key] = cursor.clone();
        nq["content"] = content;
        // A comment page cut to the response budget resumes with the page
        // size that starts a page at its first unshown comment.
        if let Some(size) = entry.get("nextPageSize") {
            nq["pageSize"] = size.clone();
        }
        if axis == "comments" {
            nq["offset"] = json!(0);
        }
        next.insert(name.into(), menu_read(nq));
    }
    if !next.is_empty() {
        out["isPartial"] = json!(true);
        out["partialReasons"] = json!(["contentPagination"]);
        out["next"] = Value::Object(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squash_headline_names_its_pull_request() {
        assert_eq!(
            headline_pull_request("io: use `spawn_mandatory_blocking` (#8506)"),
            Some(8506)
        );
        assert_eq!(headline_pull_request("Merge branch main"), None);
        assert_eq!(headline_pull_request("fix (#abc)"), None);
        assert_eq!(headline_pull_request("revert (#12) partially"), None);
    }

    /// D2: a commit's `continuePatch` copies the page-stream cursor named on
    /// `filePagination`, never a file's own per-file `nextOffset`.
    #[test]
    fn commit_continue_patch_copies_the_page_stream_cursor() {
        let query = HistoryItemRequest::from_row(json!({
            "operation":"commit","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
            "ref":"abc","sections":["patches"],"length":10
        }))
        .expect("commit query");
        let mut out = json!({
            "files":[{"filename":"a.rs","patch":"aaa"},{"filename":"b.rs","patch":"bbbbbbb",
                "patchPagination":{"offset":0,"length":7,"totalChars":20,"hasMore":true,"nextOffset":7}}],
            "filePagination":{"currentPage":1,"hasMore":false,"nextPatchOffset":10}
        });
        let cursors = DiffCursors::of_file_page(&out["filePagination"], true);
        attach_diff_continuations(
            &mut out,
            &query,
            ItemOperation::Commit,
            Some("abc"),
            false,
            cursors,
        );
        let next = &out["next"]["continuePatch"]["query"]["queries"][0];
        assert_eq!(next["offset"], 10, "{out}");
        assert_eq!(next["length"], 10);
    }

    /// HI9b: every patch-walk hop asks for twice the configured page
    /// (`output.pagination.defaultCharLength`, capped at the contract
    /// maximum), whatever page the hop itself ran with, so no hop needs an
    /// `offset` marker to avoid doubling again; a caller's explicit `length`
    /// keeps the call's page.
    #[test]
    fn history_b_continue_patch_hops_double_the_first_page_only() {
        let lead_with = |fields: Value, page: Option<usize>, configured: Option<usize>| {
            let mut row = json!({
                "operation":"pullRequest","mainGoal":"test","reasoning":"test",
                "owner":"a","repo":"b","number":1,"sections":["patches"]
            });
            for (key, value) in fields.as_object().into_iter().flatten() {
                row[key] = value.clone();
            }
            let mut query = HistoryItemRequest::from_row(row).expect("query");
            query.auto_page_chars = page;
            query.configured_page_chars = configured;
            let mut out = json!({"pullRequests":[{"contentPagination":{
                "patches":{"hasMore":true,"nextOffset":900}
            }}]});
            promote_pr_continuations(&mut out, &query);
            out["next"]["continuePatch"]["query"].clone()
        };
        let lead = |fields: Value, page: Option<usize>| lead_with(fields, page, Some(20_000));
        assert_eq!(lead(json!({}), Some(20_000))["responseLength"], 40_000);
        // A hop pages its response by whole rows: an overflow splits the row
        // into structured parts instead of text windows that hide the rows.
        assert_eq!(lead(json!({}), Some(20_000))["responseScope"], "rows");
        // A hop ran at the doubled page; its next hop asks for the same page.
        assert_eq!(lead(json!({}), Some(40_000))["responseLength"], 40_000);
        assert_eq!(
            lead(json!({"offset":5}), Some(40_000))["responseLength"],
            40_000
        );
        assert_eq!(
            lead_with(json!({}), Some(40_000), Some(40_000))["responseLength"],
            50_000,
            "capped at the contract maximum"
        );
        assert_eq!(
            lead(json!({"length":900}), Some(20_000))["responseLength"],
            20_000
        );
        assert!(
            lead_with(json!({}), None, None)
                .get("responseLength")
                .is_none()
        );
        let hop = lead_with(json!({}), Some(5_000), Some(5_000));
        assert_eq!(hop["responseLength"], 10_000);
        assert!(hop["queries"][0].get("length").is_none(), "{hop}");
        crate::contracts::validate("ghGetHistoryItem", hop).expect("a valid hop");

        let mut commit = HistoryItemRequest::from_row(json!({
            "operation":"commit","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
            "ref":"abc","sections":["patches"]
        }))
        .expect("commit query");
        commit.auto_page_chars = Some(20_000);
        commit.configured_page_chars = Some(20_000);
        let mut out =
            json!({"filePagination":{"currentPage":1,"hasMore":false,"nextPatchOffset":10}});
        let cursors = DiffCursors::of_file_page(&out["filePagination"], true);
        attach_diff_continuations(
            &mut out,
            &commit,
            ItemOperation::Commit,
            Some("abc"),
            false,
            cursors,
        );
        assert_eq!(
            out["next"]["continuePatch"]["query"]["responseLength"], 40_000,
            "{out}"
        );
        assert_eq!(
            out["next"]["continuePatch"]["query"]["responseScope"], "rows",
            "{out}"
        );

        // The next file page of a patch walk asks for the walk's page and
        // needs no offset marker; an inventory's next file page asks for
        // neither.
        let file_page = |sections: Value, offset: Option<u64>| {
            let mut row = json!({
                "operation":"pullRequest","mainGoal":"test","reasoning":"test",
                "owner":"a","repo":"b","number":1,"sections":sections
            });
            if let Some(offset) = offset {
                row["offset"] = json!(offset);
            }
            let mut query = HistoryItemRequest::from_row(row).expect("query");
            query.auto_page_chars = Some(40_000);
            query.configured_page_chars = Some(20_000);
            let mut out = json!({"pullRequests":[{"contentPagination":{
                "files":{"hasMore":true,"nextPage":2}
            }}]});
            promote_pr_continuations(&mut out, &query);
            out["next"]["nextFilePage"]["query"].clone()
        };
        let walk = file_page(json!(["patches"]), Some(31_000));
        assert_eq!(walk["responseLength"], 40_000, "{walk}");
        assert_eq!(walk["responseScope"], "rows", "{walk}");
        assert!(walk["queries"][0].get("offset").is_none(), "{walk}");
        assert_eq!(walk["queries"][0]["filePage"], 2, "{walk}");
        crate::contracts::validate("ghGetHistoryItem", walk).expect("a valid hop");
        let inventory = file_page(json!(["files"]), None);
        assert!(inventory.get("responseLength").is_none(), "{inventory}");
        assert!(inventory.get("responseScope").is_none(), "{inventory}");
        assert!(
            inventory["queries"][0].get("offset").is_none(),
            "{inventory}"
        );
    }

    #[test]
    fn selected_patch_continuation_stops_after_every_requested_path_is_returned() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
            "sections":["patches"],"patchRanges":[{"file":"src/lib.rs","additions":[1]}]
        }))
        .expect("selected patch query");
        let mut output = json!({

            "pullRequests":[{
                "files":[{"path":"src/lib.rs","patch":"diff"}],
                "contentPagination":{"files":{
                    "hasMore":true,
                    "nextPage":1
                }}
            }]
        });

        promote_pr_continuations(&mut output, &query);

        assert_eq!(
            output["pullRequests"][0]["contentPagination"]["files"]["hasMore"], false,
            "{output}"
        );
        assert!(output.pointer("/next/nextFilePage").is_none(), "{output}");
    }

    #[test]
    fn selected_patch_continuation_carries_only_unresolved_paths() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
            "sections":["patches"],"include":["src/a.rs","src/b.rs"],"patchRanges":[{"file":"src/a.rs","additions":[1]}]
        }))
        .expect("selected patch query");
        let mut output = json!({

            "pullRequests":[{
                "files":[{"path":"src/a.rs","patch":"diff"}],
                "contentPagination":{"files":{
                    "hasMore":true,
                    "nextPage":1
                }}
            }]
        });

        promote_pr_continuations(&mut output, &query);

        let next_query = &output["next"]["nextFilePage"]["query"]["queries"][0];
        assert_eq!(next_query["include"], json!(["src/b.rs"]), "{output}");
        assert_eq!(next_query["filePage"], 1, "{output}");
    }

    #[test]
    fn selected_patch_continuation_filters_resolved_range_selectors() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
            "sections":["patches"],"patchRanges":[{"file":"src/a.rs","additions":[1]},{"file":"src/b.rs","deletions":[2]}]
        }))
        .expect("selected patch range query");
        let mut output = json!({

            "pullRequests":[{
                "files":[{"path":"src/a.rs","patch":"diff"}],
                "contentPagination":{"files":{
                    "hasMore":true,
                    "nextPage":1
                }}
            }]
        });

        promote_pr_continuations(&mut output, &query);

        let ranges = &output["next"]["nextFilePage"]["query"]["queries"][0]["patchRanges"];
        assert_eq!(
            ranges,
            &json!([{"file":"src/b.rs","deletions":[2]}]),
            "{output}"
        );
    }

    #[test]
    fn match_string_views_offer_the_whole_patches_they_narrowed() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
            "sections":["patches"],"matchString":"needle","offset":10,"filePage":2
        }))
        .expect("match query");
        let mut out = json!({"pullRequests":[{"files":[
            {"path":"src/a.rs","patch":"@@ -1,1 +1,1 @@\n+needle","fullPatchChars":900},
            {"path":"src/b.rs","patch":"+needle"}
        ]}]});
        attach_full_patch_continuation(&mut out, &query);
        let next = &out["next"]["readFullPatches"]["query"]["queries"][0];
        // The published spelling; patch hunks are never minified, so no
        // `minify` rides along.
        assert_eq!(next["sections"], json!(["patches"]), "{out}");
        assert_eq!(next["include"], json!(["src/a.rs"]), "{out}");
        for key in ["matchString", "offset", "filePage", "content", "minify"] {
            assert!(next.get(key).is_none(), "{key} kept: {next}");
        }
        // An explicit contextLines still clips long lines: the whole patch
        // stays reachable.
        let explicit: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
            "sections":["patches"],"matchString":"needle","contextLines":0
        }))
        .expect("match query");
        let mut out = json!({"pullRequests":[{"files":[
            {"path":"src/a.rs","patch":"+needle","fullPatchChars":900}
        ]}]});
        attach_full_patch_continuation(&mut out, &explicit);
        let next = &out["next"]["readFullPatches"]["query"]["queries"][0];
        assert_eq!(next["include"], json!(["src/a.rs"]), "{out}");
        assert!(next.get("contextLines").is_none(), "{next}");
        assert!(out["next"].get("widenContext").is_none(), "{out}");
    }

    /// A cut comment body continues with the one text offset (`offset`),
    /// on the same comment page, reading only comments: every pull-request
    /// text surface windows with `offset`, as an issue's do.
    #[test]
    fn a_cut_comment_body_continues_with_offset_on_its_comment_page() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"a","repo":"b","number":1,
            "sections":["body","comments"],"commentPage":2
        }))
        .expect("pr query");
        let mut out = json!({"pullRequests":[{"contentPagination":{
            "commentBody":{"hasMore":true,"nextOffset":500}
        }}]});
        promote_pr_continuations(&mut out, &query);
        let next = &out["next"]["continueCommentBody"]["query"]["queries"][0];
        assert_eq!(next["offset"], 500, "{out}");
        assert_eq!(next["commentPage"], 2, "{out}");
        assert_eq!(next["sections"], json!(["comments"]), "{next}");
        assert!(next.get("commentOffset").is_none(), "{next}");
    }

    /// `next.readRawBody` re-reads only the minified surfaces, raw, from
    /// their first character, on the same comment page.
    #[test]
    fn raw_body_read_targets_the_minified_surfaces_on_the_same_page() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"a","repo":"b","number":1,
            "sections":["body","patches","comments"],
            "commentPage":2,"offset":10
        }))
        .expect("pr query");
        let mut out = json!({"pullRequests":[{"number":1}]});
        attach_raw_body_read(&mut out, &query, &[]);
        assert!(out.get("next").is_none(), "{out}");
        attach_raw_body_read(&mut out, &query, &["body", "comments"]);
        let next = &out["next"]["readRawBody"]["query"]["queries"][0];
        assert_eq!(next["minify"], "none", "{out}");
        assert_eq!(next["commentPage"], 2, "{out}");
        for key in ["offset", "content"] {
            assert!(next.get(key).is_none(), "{key} kept: {next}");
        }
        assert_eq!(next["sections"], json!(["body", "comments"]), "{next}");
    }

    /// More reshaped files than one selection holds continue in numbered
    /// reads that reach each file exactly once.
    #[test]
    fn reshaped_files_past_one_selection_continue_in_numbered_reads() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"a","repo":"b","number":1,
            "sections":["patches"],"matchString":"x"
        }))
        .expect("pr query");
        let rows = (0..SELECTED_PATCH_FILES + 3)
            .map(|i| json!({"path":format!("src/{i}.rs"),"patch":"...","fullPatchChars":90}))
            .collect::<Vec<_>>();
        let mut out = json!({"pullRequests":[{"files":rows}]});
        attach_full_patch_continuation(&mut out, &query);
        let files = |name: &str| {
            out["next"][name]["query"]["queries"][0]["include"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        };
        assert_eq!(
            files("readFullPatches").len(),
            SELECTED_PATCH_FILES,
            "{out}"
        );
        assert_eq!(
            files("readFullPatches2"),
            (SELECTED_PATCH_FILES..SELECTED_PATCH_FILES + 3)
                .map(|i| json!(format!("src/{i}.rs")))
                .collect::<Vec<_>>()
        );
        assert!(out["next"].get("readFullPatches3").is_none(), "{out}");
    }

    /// Many narrowed files get the whole-patch re-read; the default
    /// context already holds 10 lines, so no narrower `widenContext` view is
    /// offered, and the marker never reaches default rows.
    #[test]
    fn many_narrowed_files_read_full_patches_without_a_narrower_view() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"a","repo":"b","number":1,
            "matchString":"miri"
        }))
        .expect("match query");
        let rows = (0..6)
            .map(|i| json!({"path":format!("src/{i}.rs"),"patch":"+miri","fullPatchChars":90}))
            .collect::<Vec<_>>();
        let mut out = json!({"pullRequests":[{"files":rows}]});
        attach_full_patch_continuation(&mut out, &query);
        assert!(out["next"].get("widenContext").is_none(), "{out}");
        assert_eq!(
            out["next"]["readFullPatches"]["query"]["queries"][0]["include"]
                .as_array()
                .map(Vec::len),
            Some(6),
            "{out}"
        );
        assert!(!out.to_string().contains("fullPatchChars"), "{out}");
    }

    /// The row already shows a merged PR's `mergeCommitSha`, so the menu
    /// offers no separate merge-commit read beside it.
    #[test]
    fn merged_pull_requests_offer_no_merge_commit_read_beside_the_shown_sha() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"o","repo":"r","number":5
        }))
        .expect("pr query");
        for raw in [
            json!({"merged_at":"2026-09-26T15:24:18Z","merge_commit_sha":"facc6fc"}),
            json!({"merged_at":"2026-09-26T15:24:18Z","merge_commit_sha":"facc6fc",
                "changed_files":3,"additions":63,"deletions":4}),
            json!({"merged_at":null,"merge_commit_sha":"deadbee"}),
        ] {
            let menu = pr_next_menu(&query, None, "none", &[], &raw);
            assert!(menu.get("readMergeCommit").is_none(), "{menu}");
        }
    }

    /// A merged PR whose patches were read offers its most-changed source
    /// file at the merge commit: one read of every hunk's new-side lines,
    /// padded and merged.
    #[test]
    fn merged_patch_reads_offer_the_changed_source_at_the_merge_commit() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"cli","repo":"cli","number":14429
        }))
        .expect("pr query");
        let merged = json!({"merged_at":"2026-09-11T15:55:40Z","merge_commit_sha":"8fcd6a6"});
        let file = |name: &str, additions: u64, deletions: u64, patch: &str| json!({"filename":name,"additions":additions,"deletions":deletions,"patch":patch});
        let files = vec![
            file(
                "acceptance/testdata/pr/merge.txtar",
                6,
                2,
                "@@ -33,2 +33,2 @@\n-# Merge\n+# Merge and delete",
            ),
            file(
                "pkg/cmd/pr/view.go",
                1,
                0,
                "@@ -9,1 +9,2 @@\n x\n+count := countItems(pr)",
            ),
            file(
                "pkg/cmd/pr/merge/merge.go",
                2,
                2,
                "@@ -589,7 +589,7 @@ func New() {\n \t\tdeleteBranch: opts.DeleteBranch,\n-\t\tcrossRepoPR: x,\n+\t\tcrossRepoPR:        pr.IsCrossRepository,\n@@ -600,3 +600,3 @@\n+\tok := true\n@@ -900,1 +900,1 @@\n+\tdone()",
            ),
            file(
                "pkg/cmd/pr/merge/merge_test.go",
                55,
                0,
                "@@ -1 +1 @@\n+func TestX() {}",
            ),
        ];
        let all = |_: &Value| true;
        let read = read_at_merge(&query, &merged, &files, all).expect("offer");
        assert_eq!(read["tool"], "ghGetFileContent");
        // HI11: the diff numbers `sourceSha`; the merge commit is searched
        // for one distinctive added line per hunk, never read by number.
        assert_eq!(
            read["query"],
            json!({"queries":[{"owner":"cli","repo":"cli","ref":"8fcd6a6",
                "path":"pkg/cmd/pr/merge/merge.go",
                "matchString":["crossRepoPR:        pr.IsCrossRepository,"]}]})
        );
        // An `include` read offers only a file it selected.
        let view = |file: &Value| file["filename"] == "pkg/cmd/pr/view.go";
        let read = read_at_merge(&query, &merged, &files, view).expect("selected offer");
        assert_eq!(read["query"]["queries"][0]["path"], "pkg/cmd/pr/view.go");
        let docs = |file: &Value| {
            file["filename"]
                .as_str()
                .is_some_and(|name| name.ends_with(".txtar"))
        };
        assert!(read_at_merge(&query, &merged, &files, docs).is_none());
        // Open PRs, files without patches, and patches with nothing added
        // offer nothing.
        let open = json!({"merged_at":null,"merge_commit_sha":"deadbee"});
        assert!(read_at_merge(&query, &open, &files, all).is_none());
        let patchless =
            [json!({"filename":"pkg/cmd/pr/merge/merge.go","additions":2,"deletions":2})];
        assert!(read_at_merge(&query, &merged, &patchless, all).is_none());
        let removed = [file("a.go", 0, 1, "@@ -1 +0,0 @@\n-x := 1")];
        assert!(read_at_merge(&query, &merged, &removed, all).is_none());
        // A fix of short lines only still locates them.
        let short = [file("a.go", 1, 0, "@@ -1 +1 @@\n+\tok := true")];
        let read = read_at_merge(&query, &merged, &short, all).expect("short offer");
        assert_eq!(
            read["query"]["queries"][0]["matchString"],
            json!(["ok := true"])
        );
        // Docs and tests are not the shipped fix: no code file, no offer.
        let no_code = [
            file(
                "acceptance/README.md",
                1,
                0,
                "@@ -1 +1 @@\n+Run the acceptance suite",
            ),
            file(
                "pkg/cmd/pr/merge/merge_test.go",
                1,
                0,
                "@@ -1 +1 @@\n+func TestX() {}",
            ),
        ];
        assert!(read_at_merge(&query, &merged, &no_code, all).is_none());
    }

    /// A change's top file reads whole at its new side (`readAtCommit`)
    /// and at its old side over the hunks' old-side windows
    /// (`readParent`): gutters number `-` lines on the old side, so parent
    /// ranges come from `-a,b`, never from `+c,d`.
    #[test]
    fn change_reads_number_each_side_from_its_own_hunk_side() {
        let file = |name: &str, status: &str, patch: &str| json!({"filename":name,"status":status,"additions":2,"deletions":1,"patch":patch});
        let modified = file(
            "src/lib.rs",
            "modified",
            "@@ -40,3 +60,4 @@ fn f\n a\n-b\n+c\n+d\n e",
        );
        let sides = ChangeSides {
            new_ref: Some("c0ffee"),
            old_ref: Some("parent1"),
            old_confidence: "high",
        };
        let reads = change_reads("o", "r", std::slice::from_ref(&modified), &sides, |_| true);
        let names = reads.iter().map(|(name, _)| *name).collect::<Vec<_>>();
        assert_eq!(names, ["readAtCommit", "readParent"]);
        assert_eq!(
            reads[0].1["query"],
            json!({"queries":[{"owner":"o","repo":"r","path":"src/lib.rs","ref":"c0ffee"}]})
        );
        assert_eq!(
            reads[1].1["query"],
            json!({"queries":[{"owner":"o","repo":"r","path":"src/lib.rs","ref":"parent1","ranges":["30-52"]}]})
        );
        // A rename reads its parent under the old path; an added file has
        // no parent side, a removed one no new side.
        let mut renamed = modified.clone();
        renamed["status"] = json!("renamed");
        renamed["previous_filename"] = json!("src/old.rs");
        let reads = change_reads("o", "r", &[renamed], &sides, |_| true);
        assert_eq!(reads[1].1["query"]["queries"][0]["path"], "src/old.rs");
        let added = file("src/new.rs", "added", "@@ -0,0 +1,2 @@\n+x\n+y");
        let reads = change_reads("o", "r", &[added], &sides, |_| true);
        assert_eq!(
            reads.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            ["readAtCommit"]
        );
        let removed = file("src/gone.rs", "removed", "@@ -1,2 +0,0 @@\n-x\n-y");
        let reads = change_reads("o", "r", &[removed], &sides, |_| true);
        assert_eq!(
            reads.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            ["readParent"]
        );
        assert_eq!(reads[0].1["query"]["queries"][0]["ranges"], json!(["1-2"]));
        // Without a parent ref only the new side is offered.
        let no_parent = ChangeSides {
            old_ref: None,
            ..sides
        };
        let reads = change_reads("o", "r", &[modified], &no_parent, |_| true);
        assert_eq!(
            reads.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            ["readAtCommit"]
        );
    }

    /// Old-side hunk spans merge where their padded windows touch, start at
    /// line 1, skip pure additions, and fit one read's range cap.
    #[test]
    fn parent_windows_fit_one_ranged_read() {
        assert_eq!(parent_windows("@@ -1,4 +1,3 @@ fn a\n"), ["1-14"]);
        assert_eq!(parent_windows("@@ -5 +5 @@\n@@ -38,0 +40,2 @@\n"), ["1-15"]);
        // A hunk with fewer trailing context lines than the diff keeps ends
        // at the end of the file: its span stops at its last line.
        assert_eq!(
            parent_windows("@@ -10,5 +10,4 @@\n a\n b\n c\n-d\n e\n"),
            ["1-14"]
        );
        assert_eq!(
            parent_windows("@@ -10,7 +10,6 @@\n a\n b\n c\n-d\n e\n f\n g\n"),
            ["1-26"]
        );
        let many = (0..15)
            .map(|i| format!("@@ -{0},1 +{0},1 @@\n", 100 * (i + 1) + i))
            .collect::<String>();
        let ranges = parent_windows(&many);
        assert_eq!(ranges.len(), PARENT_WINDOW_RANGES, "{ranges:?}");
        assert!(ranges[0].starts_with("90-"), "{ranges:?}");
        assert!(
            ranges[PARENT_WINDOW_RANGES - 1].ends_with("-1524"),
            "{ranges:?}"
        );
    }

    /// HI11: one distinctive added line per hunk (the longest of at most
    /// 200 chars, not punctuation, not a redaction; under 12 chars only when
    /// no hunk has a longer one), duplicates once.
    #[test]
    fn distinctive_added_lines_pick_one_locator_per_hunk() {
        let patch = "@@ -1,3 +1,4 @@\n a\n+  }\n+  const total = items.length;\n+  let x = 1;\n@@ -40 +41,2 @@\n+});\n+[REDACTED secret value here]\n@@ -90 +92 @@\n+  const total = items.length;\n@@ -99 +100 @@\n+    return computeTotal(items, options);";
        assert_eq!(
            distinctive_added_lines(patch),
            [
                "const total = items.length;",
                "return computeTotal(items, options);"
            ]
        );
    }

    #[test]
    fn pr_next_menu_carries_required_defaults_and_drops_cursors() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"o","repo":"r","number":5,
            "sections":["body"],"offset":100,"commentPage":2,
            "reasoning":"r"
        }))
        .expect("query");
        let content_value = query.content_value();
        let content = content_value.as_ref().and_then(Value::as_object);
        let menu = pr_next_menu(&query, content, "none", &["src/a.rs".into()], &json!({}));
        let reviews = &menu["readFiles"]["query"]["queries"][0];
        // pageSize has no contract default; an omitted one stays omitted.
        assert!(reviews.get("pageSize").is_none(), "{reviews}");
        // Menu reads use the flat spelling; the default minify stays implicit.
        assert!(reviews.get("minify").is_none(), "{reviews}");
        assert_eq!(reviews["sections"], json!(["files"]));
        assert!(reviews.get("content").is_none(), "{reviews}");
        for key in ["offset", "commentPage"] {
            assert!(reviews.get(key).is_none(), "{key} leaked: {reviews}");
        }
        assert_eq!(reviews["mainGoal"], "test");
        assert_eq!(reviews["reasoning"], "r");
        assert!(menu.get("readBody").is_none());
    }

    #[test]
    fn pr_next_menu_omits_entries_the_row_already_answers() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal": "test", "reasoning":"r","owner":"o","repo":"r","number":1
        }))
        .expect("query");
        let names = |menu: &Value| {
            menu.as_object()
                .map(|m| m.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default()
        };
        // Empty body, one small file, comments present: every patch reads in
        // one call, so no separate file list.
        let small = json!({"body":"","changed_files":1,"additions":6,"deletions":1,"comments":3});
        let menu = pr_next_menu(&query, None, "none", &[], &small);
        assert_eq!(names(&menu), ["readPatches", "readDiscussion"]);
        assert_eq!(
            menu["readPatches"]["query"]["queries"][0]["sections"],
            json!(["patches"])
        );
        // A body rides the file list of a large diff.
        let large = json!({"body":"x","changed_files":40,
            "additions":900,"deletions":50,"comments":0,"review_comments":0});
        let menu = pr_next_menu(&query, None, "none", &[], &large);
        assert_eq!(names(&menu), ["readFiles", "readDiscussion"]);
        assert_eq!(
            menu["readFiles"]["query"]["queries"][0]["sections"],
            json!(["body", "files"])
        );
        // Provably no comments: the discussion read asks for reviews only.
        assert_eq!(
            menu["readDiscussion"]["query"]["queries"][0]["sections"],
            json!(["reviews"])
        );
        // No changed files and a body: the body read stands alone.
        let empty = json!({"body":"y","changed_files":0,
            "comments":0,"review_comments":0});
        assert_eq!(
            names(&pr_next_menu(&query, None, "none", &[], &empty)),
            ["readBody", "readDiscussion"]
        );
    }

    /// A menu holds at most [`MENU_CAP`] entries; an inventory read turns its
    /// review pick into `readSelectedPatches` over several files.
    #[test]
    fn menus_hold_at_most_two_entries_and_inventory_reviews_many_files() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal": "test", "reasoning":"r","owner":"o","repo":"r","number":1
        }))
        .expect("query");
        let merged = json!({"body":"x".repeat(900),"changed_files":656,"additions":282_700,
            "deletions":284_842,"comments":5,"merged_at":"2026-01-01T00:00:00Z","merge_commit_sha":"abc"});
        let menu = pr_next_menu(&query, None, "none", &[], &merged);
        let keys = menu
            .as_object()
            .map(|m| m.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        assert_eq!(keys, ["readFiles", "readDiscussion"], "{menu}");
        assert!(!menu.to_string().contains("<literal"), "{menu}");
        let review = vec!["src/a.rs".to_owned(), "src/b.rs".to_owned()];
        let inventory = json!({"changed_files":30,"additions":900,"deletions":10});
        let content = json!({"files":true});
        let menu = pr_next_menu(&query, content.as_object(), "none", &review, &inventory);
        assert_eq!(
            menu["readSelectedPatches"]["query"]["queries"][0]["sections"],
            json!(["patches"])
        );
        assert_eq!(
            menu["readSelectedPatches"]["query"]["queries"][0]["include"],
            json!(["src/a.rs", "src/b.rs"])
        );
        assert_eq!(menu["readSelectedPatches"]["confidence"], "high");
        assert!(menu.get("readPatches").is_some(), "{menu}");
        // The discussion read is the third candidate: it stays reachable
        // through `include`, not the menu.
        assert_eq!(menu.as_object().map(Map::len), Some(MENU_CAP), "{menu}");
        assert!(menu.get("readDiscussion").is_none(), "{menu}");
        // A pick covering every changed file is the every-patch read.
        let two = json!({"body":"","changed_files":2,"additions":900,"deletions":10});
        let menu = pr_next_menu(&query, content.as_object(), "none", &review, &two);
        assert_eq!(
            menu["readPatches"]["query"]["queries"][0]["sections"],
            json!(["patches"]),
            "{menu}"
        );
        assert!(
            menu["readPatches"]["query"]["queries"][0]
                .get("include")
                .is_none(),
            "{menu}"
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
            "filePage":2,"include":["src/**"]
        }))
        .expect("query");
        let mut out = json!({"pullRequests":[{"contentPagination":{
            "patches":{"hasMore":true,"nextOffset":900,"unfinishedFiles":2}
        }}]});
        promote_pr_continuations(&mut out, &query);
        let hop = &out["next"]["continuePatch"]["query"]["queries"][0];
        assert!(hop.get("queries").is_none(), "{hop}");
        assert!(hop.get("content").is_none(), "{hop}");
        assert!(hop.get("minify").is_none(), "default minify: {hop}");
        assert!(hop.get("length").is_none(), "{hop}");
        assert_eq!(hop["operation"], "pullRequest");
        assert_eq!(hop["sections"], json!(["patches"]), "{hop}");
        assert_eq!(hop["include"], json!(["src/**"]), "{hop}");
        assert_eq!(hop["filePage"], 2, "{hop}");
        assert_eq!(hop["offset"], 900, "{hop}");
        assert_eq!(
            out["pullRequests"][0]["contentPagination"]["patches"]["unfinishedFiles"],
            2
        );
        crate::contracts::validate_query("ghGetHistoryItem", hop.clone())
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
            "patches":{"hasMore":true,"nextOffset":900,"unfinishedFiles":2},
            "files":{"hasMore":true,"nextPage":2}
        }}]});
        promote_pr_continuations(&mut open, &query);
        let names = open["next"]
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
        promote_pr_continuations(&mut last, &hop);
        let names = last["next"]
            .as_object()
            .map(|next| next.keys().cloned().collect::<Vec<_>>());
        assert_eq!(names, Some(vec!["nextFilePage".to_owned()]), "{last}");
        let page = &last["next"]["nextFilePage"]["query"]["queries"][0];
        assert_eq!(page["filePage"], 2, "{page}");
        // The walk's next file page opens at the stream start, never at the
        // old cursor (HI9b: no offset marker keeps the walk's page).
        assert!(page.get("offset").is_none(), "{page}");
    }

    /// Compare: each page has one canonical query. The commit page drops
    /// every file cursor; the file page restarts the patch offset and follows
    /// the last patch window; a patch hop re-offers neither.
    #[test]
    fn compare_pages_are_offered_once_with_their_own_cursor_only() {
        let compare = |fields: Value| {
            HistoryItemRequest::from_row(super::super::util::merge(
                json!({"operation":"compare","owner":"a","repo":"b","base":"x","head":"y",
                    "sections":["patches"],"length":5000}),
                fields,
            ))
            .expect("compare query")
        };
        let mut first = json!({
            "commits":[{"sha":"1"}],
            "pagination":{"currentPage":1,"hasMore":true,"nextPage":2},
            "files":[{"path":"a.rs","patch":"x"}],
            "filePagination":{"currentPage":1,"hasMore":true,"nextPage":2,"nextPatchOffset":5000}
        });
        let cursors = DiffCursors {
            commit_page: Some(2),
            ..DiffCursors::of_file_page(&first["filePagination"], true)
        };
        attach_diff_continuations(
            &mut first,
            &compare(json!({})),
            ItemOperation::Compare,
            None,
            false,
            cursors,
        );
        let next = first["next"].as_object().expect("next");
        assert!(next.get("nextFilePage").is_none(), "{first}");
        let commits = &first["next"]["nextPage"]["query"]["queries"][0];
        for key in ["filePage", "offset", "length"] {
            assert!(commits.get(key).is_none(), "{key}: {commits}");
        }
        assert_eq!(
            first["next"]["continuePatch"]["query"]["queries"][0]["offset"],
            5000
        );

        let mut last = json!({
            "files":[{"path":"b.rs","patch":"y"}],
            "filePagination":{"currentPage":1,"hasMore":true,"nextPage":2}
        });
        let cursors = DiffCursors::of_file_page(&last["filePagination"], true);
        attach_diff_continuations(
            &mut last,
            &compare(json!({"offset":5000})),
            ItemOperation::Compare,
            None,
            false,
            cursors,
        );
        let names = last["next"]
            .as_object()
            .map(|next| next.keys().cloned().collect::<Vec<_>>());
        assert_eq!(names, Some(vec!["nextFilePage".to_owned()]), "{last}");
        let files = &last["next"]["nextFilePage"]["query"]["queries"][0];
        assert_eq!(files["filePage"], 2, "{files}");
        assert!(files.get("offset").is_none(), "{files}");
    }

    #[test]
    fn char_offset_continuations_narrow_content_to_their_own_surface() {
        // `offset` is one shared field: continuing the body must not skew
        // review bodies or patches by the body offset (and vice versa).
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"o","repo":"r","number":5,
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
        promote_pr_continuations(&mut out, &query);
        let next = &out["next"];
        assert_eq!(
            next["continueBody"]["query"]["queries"][0]["sections"],
            json!(["body"])
        );
        assert_eq!(next["continueBody"]["query"]["queries"][0]["offset"], 12000);
        assert_eq!(
            next["continueReviewBody"]["query"]["queries"][0]["sections"],
            json!(["reviews"])
        );
        assert_eq!(
            next["continuePatch"]["query"]["queries"][0]["sections"],
            json!(["patches"])
        );
        assert!(
            next["continuePatch"]["query"]["queries"][0]
                .get("content")
                .is_none()
        );
        let comments = &next["nextCommentPage"]["query"]["queries"][0];
        assert!(comments.get("offset").is_none(), "{comments}");
        assert_eq!(comments["commentPage"], 2);
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
        promote_issue_continuations(&mut out, &query);
        let body = &out["next"]["continueBody"]["query"]["queries"][0];
        assert_eq!(body["sections"], json!(["body"]), "{out}");
        assert!(body.get("content").is_none(), "{out}");
        let comments = &out["next"]["nextCommentPage"]["query"]["queries"][0];
        assert_eq!(comments["sections"], json!(["comments"]), "{out}");
        assert!(comments.get("content").is_none(), "{out}");
    }
}
