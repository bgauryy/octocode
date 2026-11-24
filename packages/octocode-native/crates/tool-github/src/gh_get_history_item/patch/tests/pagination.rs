//! Patch pagination: one char budget per page, clamped lengths, per-file offsets beside the page cursor.
use super::*;
use octocode_test_support::json_fixture;

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
        let _ = shape_pr_files(
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
        // The last window states itself (it is not the whole stream) and
        // names no continuation.
        let Some(patches) = pagination
            .get("patches")
            .filter(|patches| patches["hasMore"] == true)
        else {
            break;
        };
        assert!(patches.get("files").is_none(), "{patches}");
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
        json_fixture(r#"{"filename": "a.rs", "status": "modified", "patch": "ABCD"}"#),
        json_fixture(r#"{"filename": "next.rs", "status": "modified", "patch": "x"}"#),
        json_fixture(
            r#"{
                "filename": "b.rs",
                "status": "modified",
                "patch": "abcdef"
            }"#,
        ),
    ];
    let _ = shape_pr_files(
        &mut row,
        &mut pagination,
        files,
        WindowState::COMPLETE,
        &query,
        true,
        None,
    );
    // The page's patch stream: the window and where it continues.
    assert_eq!(
        pagination["patches"],
        json_fixture(
            r#"{
                "unit": "chars",
                "offset": 0,
                "length": 2,
                "total": 11,
                "hasMore": true,
                "nextOffset": 2
            }"#
        )
    );
    let request: HistoryItemRequest = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "number": 5,
            "sections": ["patches"],
            "page": 2
        }"#,
    ))
    .expect("query");
    let mut out = json!({"pullRequests":[{"sectionPagination":{
        "patches": pagination["patches"].clone()
    }}]});
    promote_pr_continuations(&mut out, &request);
    let next = &out["pullRequests"][0]["sectionPagination"]["patches"]["next"]["nextPage"]["input"]
        ["queries"][0];
    assert_eq!(next["sections"], json_fixture(r#"["patches"]"#), "{out}");
    assert!(next.get("include").is_none(), "{next}");
    assert_eq!(next["page"], 2, "{next}");
    assert_eq!(next["offset"], 2);
    assert_eq!(
        out["pullRequests"][0]["sectionPagination"]["patches"]["next"]
            .as_object()
            .map(serde_json::Map::len),
        Some(1),
        "{out}"
    );
    octocode_runtime_base::contracts::validate_built_query("ghGetHistoryItem", next.clone())
        .expect("the patch continuation is a valid ghGetHistoryItem query");
}

/// A pull request's last patch window states itself on the section, and its
/// one file (whose own window equals the page's) repeats none of it.
#[test]
fn a_last_pull_request_window_is_stated_once_on_the_section() {
    let mut row = json!({});
    let mut pagination = Map::new();
    let files = vec![json_fixture(
        r#"{
            "filename": "b.rs",
            "status": "modified",
            "patch": "abcdef"
        }"#,
    )];
    let _ = shape_pr_files(
        &mut row,
        &mut pagination,
        files,
        WindowState::COMPLETE,
        &patch_query(4),
        true,
        None,
    );
    assert_eq!(pagination["patches"]["offset"], 4, "{pagination:?}");
    assert_eq!(pagination["patches"]["hasMore"], false, "{pagination:?}");
    assert!(row["files"][0].get("patchPagination").is_none(), "{row}");
}

/// A patch window fills most of its response page and never more: a
/// 100k-char patch reads in three calls at a 50k page, not thirteen.
#[test]
fn patch_window_is_one_budget_for_the_whole_page() {
    let default = octocode_runtime_base::execution::RuntimeLimits::DEFAULT.auto_page_chars;
    assert_eq!(patch_window(None), patch_window(Some(default)));
    for page in [1_000, default / 3, default, 50_000] {
        let window = patch_window(Some(page));
        assert!(window > page / 2 && window < page, "{page}: {window}");
    }
    assert!(100_071usize.div_ceil(patch_window(Some(50_000))) <= 3);
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
    for page_chars in [5_000usize, 20_000, 40_000, 50_000] {
        let mut offset = 0usize;
        let mut read = HashMap::<String, String>::new();
        let mut calls = 0;
        loop {
            calls += 1;
            assert!(
                calls <= total.div_ceil(patch_window(Some(page_chars))) + files.len(),
                "cursor did not advance"
            );
            let mut query = window(json!({"offset":offset}));
            query.auto_page_chars = Some(page_chars);
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
                taken <= patch_window(Some(page_chars)),
                "{page_chars}: {taken}"
            );
            match page.cursor {
                Some(next) => {
                    assert_eq!(next, offset + taken, "{page_chars}");
                    offset = next;
                }
                None => break,
            }
        }
        // Windows end at whole files or line ends: at most one extra
        // call per file over the bare stream.
        assert!(
            calls <= total.div_ceil(patch_window(Some(page_chars))) + files.len(),
            "{page_chars}: {calls}"
        );
        for source in &files {
            let name = source["filename"].as_str().unwrap_or("");
            assert_eq!(
                read.get(name).map(String::as_str),
                source["patch"].as_str(),
                "{page_chars} {name}"
            );
        }
    }
}

/// A patch row's `patchPagination` speaks per-file coordinates only; the
/// page-stream cursor that `offset` continues rides the page.
#[test]
fn patch_rows_report_per_file_offsets_and_the_page_cursor_separately() {
    let files = vec![file("a.rs", &"a".repeat(3)), file("b.rs", &"b".repeat(20))];
    let first = shape_patch_page(files.clone(), &with_window(window(json!({})), 10));
    let b = &first.rows[1]["patchPagination"];
    assert_eq!(b["offset"], 0);
    assert_eq!(b["length"], 7);
    assert!(b.get("nextOffset").is_none(), "{b}");
    assert_eq!(first.cursor, Some(10));
    let second = shape_patch_page(
        files,
        &with_window(window(json_fixture(r#"{"offset": 10}"#)), 10),
    );
    let b = &second.rows[0]["patchPagination"];
    assert_eq!(b["offset"], 7);
    assert_eq!(b["length"], 10);
    assert_eq!(second.cursor, Some(20));
    let page = shape_patch_page(
        vec![file("a.rs", "aaa"), file("b.rs", &"b".repeat(20))],
        &with_window(window(json!({})), 10),
    )
    .compact();
    assert_eq!(page.rows[1]["patchPagination"]["length"], 7);
    assert_eq!(page.cursor, Some(10));
}

/// A file whose patch spans pages names itself by `path` on every page; its
/// change `stat` and rename `previousPath` are evidence the page that starts
/// its patch carries once, so a later page repeats neither. The one file
/// holds the commit page's whole stream, so its window is the page's
/// (`sectionPagination.patches`), not restated on the row.
#[test]
fn a_later_patch_page_names_its_file_by_path_only() {
    let files = vec![json!({"filename":"new.rs","previous_filename":"old.rs",
        "status":"renamed","additions":20,"deletions":0,"patch":"n".repeat(40)})];
    let mut offset = 0;
    let mut pages = 0;
    loop {
        let page = shape_patch_page(
            files.clone(),
            &with_window(window(json!({ "offset": offset })), 10),
        )
        .compact();
        let row = &page.rows[0];
        assert_eq!(row["path"], "new.rs", "page {pages}: {row}");
        octocode_runtime_base::test_support::assert_row_contract(
            octocode_runtime_base::id::ToolId::GhGetHistoryItem,
            &json!({"pullRequests":[{"number":1,"title":"t","state":"open",
                "author":"a","createdAt":"2026-11-07T10:00:00Z","files":[row]}]}),
        );
        assert!(row.get("patchPagination").is_none(), "page {pages}: {row}");
        if offset == 0 {
            assert_eq!(
                (&row["status"], &row["additions"], &row["deletions"]),
                (&json!("renamed"), &json!(20), &json!(0)),
                "{row}"
            );
            assert_eq!(row["previousPath"], "old.rs", "{row}");
        } else {
            assert!(row.get("status").is_none(), "page {pages}: {row}");
            assert!(row.get("previousPath").is_none(), "page {pages}: {row}");
        }
        pages += 1;
        match page.cursor {
            Some(next) => offset = next,
            None => break,
        }
    }
    assert!(pages > 1, "the patch spans pages");
}

/// At an automatic page of 1000 the patch window shrinks with the page,
/// so a page of patches still fits one automatic response page.
#[test]
fn patch_window_derives_from_the_effective_auto_page() {
    let window = patch_window(Some(1_000));
    assert!(window < 1_000, "{window}");
    let mut query = patch_request(json_fixture(r#"{"offset": 0}"#));
    query.auto_page_chars = Some(1_000);
    let patch = "+x\n".repeat(2_000);
    let page = shape_patch_page(
        vec![json!({"filename":"a.rs","status":"modified","patch":patch})],
        &query,
    );
    let shaped = &page.rows[0];
    let text = shaped["patch"].as_str().expect("patch");
    assert!(text.encode_utf16().count() <= window, "{}", text.len());
    assert_eq!(shaped["patchPagination"]["hasMore"], true);
}

/// A one-file patch of `lines` added code lines.
fn added_lines(lines: usize) -> String {
    let body = (1..=lines)
        .map(|i| format!("+    let value_{i} = compute(input_{i}, &config);\n"))
        .collect::<String>();
    format!("@@ -0,0 +1,{lines} @@\n{body}")
}

/// The patch view chars of `patch` in one window wide enough to hold it.
fn view_chars(patch: &str) -> usize {
    let page = shape_patch_page(
        vec![file("a.rs", patch)],
        &with_budget(patch_request(json!({"sections": ["patches"]})), 1_000_000),
    );
    page.stream.to_value()["total"].as_u64().unwrap_or(0) as usize
}

/// A patch that fits its response page beside its row arrives whole, so
/// no row-level `next.patches` is left for an agent to skip. Benchmarks
/// (2026-11-23/24, v20) cut 14 of 119 patch reads that fit their page and
/// agents followed none of the 14 continuations: vuejs/core#15035 sent
/// 14,752 of 15,153 patch chars (a 248-char file summary beside it) in a
/// 16.5k reply of a 20k page; a three-row batch sent 2,626 of 4,540 in
/// its 20k/3 share. Each case here is that stream at that page, and the
/// whole published row still fits its page.
#[test]
fn a_patch_that_fits_its_page_arrives_whole() {
    let page = octocode_runtime_base::execution::RuntimeLimits::DEFAULT.auto_page_chars;
    for (share, stream, reserve) in [(page, 15_153, 248), (page / 3, 4_540, 0)] {
        let lines = (1..)
            .find(|lines| view_chars(&added_lines(*lines)) >= stream)
            .expect("a patch of that size");
        let patch = added_lines(lines);
        let query = with_budget(patch_request(json!({"sections": ["patches"]})), share);
        let shaped = shape_patch_page_reserving(vec![file("a.rs", &patch)], &query, reserve);
        assert_eq!(shaped.cursor, None, "{share}: cut at {:?}", shaped.stream);
        let published = octocode_engine::json_size::utf16_units(&shaped.rows).unwrap_or(0);
        assert!(
            published + reserve <= share,
            "{share}: {published} + {reserve}"
        );
    }
}
