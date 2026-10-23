//! Patch pagination: one char budget per page, clamped lengths, per-file offsets beside the page cursor.
use super::*;

/// A pull-request patch hop is the same file page at the page-stream
/// cursor: the page names no path list, only how many files are
/// unfinished, and following the cursor rebuilds every patch once.
#[test]
fn pull_request_patch_hops_continue_the_page_stream() {
    let files = vec![file("short.rs", "ABCD"), file("long.rs", "abcdefgh")];
    let mut offset = 0;
    let mut rebuilt = HashMap::<String, String>::new();
    for calls in 1.. {
        assert!(calls < 100, "cursor did not advance");
        let mut row = json!({});
        let mut pagination = Map::new();
        let shaped = shape_pr_files(
            &mut row,
            &mut pagination,
            files.clone(),
            WindowState::COMPLETE,
            &patch_query(offset),
            true,
            None,
        );
        for file in row["files"].as_array().into_iter().flatten() {
            let path = file["path"].as_str().unwrap_or("").to_owned();
            rebuilt
                .entry(path)
                .or_default()
                .push_str(file["patch"].as_str().unwrap_or(""));
        }
        let Some(patches) = pagination.get("patches") else {
            break;
        };
        assert!(patches.get("files").is_none(), "{patches}");
        assert!(shaped.unfinished_patches > 0, "{patches}");
        let next = patches["nextOffset"].as_u64().unwrap_or(0) as usize;
        assert!(next > offset, "{patches}");
        offset = next;
    }
    assert_eq!(rebuilt["short.rs"], "ABCD");
    assert_eq!(rebuilt["long.rs"], "abcdefgh");
}

#[test]
fn patch_pagination_counts_unfinished_files_and_continues_the_same_page() {
    let query = patch_query(0);
    let mut row = json!({});
    let mut pagination = Map::new();
    let files = vec![
        json!({"filename":"a.rs","status":"modified","patch":"ABCD"}),
        json!({"filename":"next.rs","status":"modified","patch":"x"}),
        json!({"filename":"b.rs","status":"modified","patch":"abcdef"}),
    ];
    let shaped = shape_pr_files(
        &mut row,
        &mut pagination,
        files,
        WindowState::COMPLETE,
        &query,
        true,
        None,
    );
    assert_eq!(shaped.unfinished_patches, 3, "{pagination:?}");
    // The page's patch stream: the window and where it continues.
    assert_eq!(
        pagination["patches"],
        json!({"unit":"chars","offset":0,"length":2,"total":11,"hasMore":true,"nextOffset":2})
    );
    let request: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"o","repo":"r","number":5,
        "sections":["patches"],"page":2
    }))
    .expect("query");
    let mut out = json!({"pullRequests":[{"contentPagination":{
        "patches": pagination["patches"].clone()
    }}]});
    promote_pr_continuations(&mut out, &request, &Remainder::default());
    let next = &out["next"]["continuePatch"]["query"]["queries"][0];
    assert_eq!(next["sections"], json!(["patches"]), "{out}");
    assert!(next.get("include").is_none(), "{next}");
    assert_eq!(next["page"], 2, "{next}");
    assert_eq!(next["offset"], 2);
    assert_eq!(
        out["next"].as_object().map(|next| next.len()),
        Some(1),
        "{out}"
    );
    crate::contracts::validate_query("ghGetHistoryItem", next.clone())
        .expect("continuePatch is a valid ghGetHistoryItem query");
}

/// A patch window fills most of the automatic response page: a 100k-char
/// patch reads in three calls at the default 50k page, not thirteen.
#[test]
fn patch_window_is_one_budget_for_the_whole_page() {
    assert_eq!(patch_window(None, None), 15_000);
    assert_eq!(patch_window(Some(2), None), 2);
    assert_eq!(patch_window(None, Some(50_000)), 40_000);
    assert_eq!(100_071usize.div_ceil(patch_window(None, Some(50_000))), 3);
    // An explicit length wins up to the budget.
    assert_eq!(patch_window(Some(9_000), None), 9_000);
    assert_eq!(patch_window(Some(90_000), None), 15_000);
}

/// An explicit `length` above one response page is clamped to the
/// page's one-row budget: the row never splits into response parts whose
/// patch cursor would skip the unread parts.
#[test]
fn explicit_char_length_is_clamped_to_one_response_page() {
    assert_eq!(patch_window(Some(80_000), Some(20_000)), 15_000);
    assert_eq!(patch_window(Some(50_000), None), 15_000);
    assert_eq!(patch_window(Some(100_000), Some(50_000)), 40_000);
    assert_eq!(patch_window(Some(9_000), Some(50_000)), 9_000);
    assert_eq!(patch_window(Some(150_000), Some(1_000)), 400);
    let patch = "+x\n".repeat(30_000);
    let mut query = window(json!({"length":80_000}));
    query.auto_page_chars = Some(20_000);
    let page = shape_patch_page(vec![file("big.rs", &patch)], &query);
    let row = &page.rows[0];
    assert_eq!(
        row["patch"].as_str().map(|p| p.chars().count()),
        Some(15_000)
    );
    assert_eq!(row["patchPagination"]["length"], 15_000);
    assert_eq!(page.cursor, Some(15_000));
}

/// A walk with an explicit `length` far above the response page reads
/// every patch exactly once: each window fits one page, and following
/// the cursor never skips or repeats a character of a multi-file stream.
#[test]
fn oversized_char_length_walk_reads_every_patch_exactly_once() {
    let files = (0..6)
        .map(|i| {
            file(
                &format!("f{i}.rs"),
                &format!("+{i}\n").repeat(4_000 + i * 3_000),
            )
        })
        .collect::<Vec<_>>();
    let total = files
        .iter()
        .map(|f| f["patch"].as_str().map_or(0, |p| p.chars().count()))
        .sum::<usize>();
    for length in [25_000usize, 40_000, 80_000, 100_000] {
        let mut offset = 0usize;
        let mut read = HashMap::<String, String>::new();
        let mut calls = 0;
        loop {
            calls += 1;
            assert!(calls < 100, "cursor did not advance");
            let mut query = window(json!({"offset":offset,"length":length}));
            query.auto_page_chars = Some(20_000);
            let page = shape_patch_page(files.clone(), &query);
            let mut taken = 0;
            for row in &page.rows {
                let name = str_at(row, "/filename").unwrap_or("").to_owned();
                let text = row["patch"].as_str().unwrap_or("");
                let have = read.entry(name).or_default();
                let at = row["patchPagination"]["offset"].as_u64().unwrap_or(0);
                assert_eq!(at as usize, have.chars().count(), "gap or repeat: {row}");
                have.push_str(text);
                taken += text.chars().count();
            }
            assert!(
                taken <= patch_window(None, Some(20_000)),
                "{length}: {taken}"
            );
            match page.cursor {
                Some(next) => {
                    assert_eq!(next, offset + taken, "{length}");
                    offset = next;
                }
                None => break,
            }
        }
        // Windows end at whole files or line ends: at most one extra
        // call per file over the bare stream.
        assert!(
            calls <= total.div_ceil(patch_window(None, Some(20_000))) + files.len(),
            "{length}: {calls}"
        );
        for source in &files {
            let name = source["filename"].as_str().unwrap_or("");
            assert_eq!(
                read.get(name).map(String::as_str),
                source["patch"].as_str(),
                "{length} {name}"
            );
        }
    }
}

/// A patch row's `patchPagination` speaks per-file coordinates only; the
/// page-stream cursor that `offset` continues rides the page.
#[test]
fn patch_rows_report_per_file_offsets_and_the_page_cursor_separately() {
    let files = vec![file("a.rs", &"a".repeat(3)), file("b.rs", &"b".repeat(20))];
    let first = shape_patch_page(files.clone(), &window(json!({"length":10})));
    let b = &first.rows[1]["patchPagination"];
    assert_eq!(b["offset"], 0);
    assert_eq!(b["length"], 7);
    assert!(b.get("nextOffset").is_none(), "{b}");
    assert_eq!(first.cursor, Some(10));
    let second = shape_patch_page(files, &window(json!({"offset":10,"length":10})));
    let b = &second.rows[0]["patchPagination"];
    assert_eq!(b["offset"], 7);
    assert_eq!(b["length"], 10);
    assert_eq!(second.cursor, Some(20));
    let page = shape_patch_page(
        vec![file("a.rs", "aaa"), file("b.rs", &"b".repeat(20))],
        &window(json!({"length":10})),
    )
    .compact();
    assert_eq!(page.rows[1]["patchPagination"]["length"], 7);
    assert_eq!(page.cursor, Some(10));
}

/// At an automatic page of 1000 the patch window shrinks with the page,
/// so a page of patches still fits one automatic response page.
#[test]
fn patch_window_derives_from_the_effective_auto_page() {
    assert_eq!(patch_window(None, Some(1_000)), 400);
    let mut query = patch_request(json!({"offset":0}));
    query.auto_page_chars = Some(1_000);
    let patch = "+x\n".repeat(2_000);
    let page = shape_patch_page(
        vec![json!({"filename":"a.rs","status":"modified","patch":patch})],
        &query,
    );
    let shaped = &page.rows[0];
    let text = shaped["patch"].as_str().expect("patch");
    assert!(text.encode_utf16().count() <= 400, "{}", text.len());
    assert_eq!(shaped["patchPagination"]["hasMore"], true);
}
