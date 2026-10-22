use super::super::HistoryItemRequest;
use super::super::filter::*;
use super::super::inventory::*;
use super::super::promotion::promote_pr_continuations;
use super::super::remaining::Remainder;
use super::super::util::str_at;
use super::super::window::*;
use super::hunks::*;
use super::locate::*;
use super::page::*;
use octocode_github::MAX_PER_PAGE;
use serde_json::{Map, Value, json};
use std::collections::HashMap;

fn patch_query(offset: usize) -> HistoryItemRequest {
    patch_request(json!({"offset":offset,"length":2}))
}

fn patch_request(fields: serde_json::Value) -> HistoryItemRequest {
    let base = json!({
        "operation":"pullRequest",
        "mainGoal": "test", "reasoning":"test",
        "owner":"a",
        "repo":"b",
        "number":1
    });
    HistoryItemRequest::from_row(super::super::util::merge(base, fields))
        .expect("patch query fixture should be valid")
}

fn file(name: &str, patch: &str) -> Value {
    json!({"filename":name,"status":"modified","patch":patch})
}

fn window(fields: Value) -> HistoryItemRequest {
    let base = json!({
        "operation":"commit","mainGoal":"test","reasoning":"test",
        "owner":"a","repo":"b","ref":"abc","sections":["patches"]
    });
    HistoryItemRequest::from_row(super::super::util::merge(base, fields))
        .expect("commit window fixture should be valid")
}

/// Follow a commit page's `nextOffset` cursor to the end and return
/// (calls, per-file reassembled patches, files reported patchless).
fn follow_commit_page(
    files: &[Value],
    char_length: Option<usize>,
) -> (usize, HashMap<String, String>, Vec<String>) {
    let mut offset = 0usize;
    let mut calls = 0;
    let mut patches = HashMap::<String, String>::new();
    let mut patchless = Vec::new();
    loop {
        calls += 1;
        assert!(calls < 1_000, "cursor did not advance");
        let mut fields = json!({"offset":offset});
        if let Some(length) = char_length {
            fields["length"] = json!(length);
        }
        let page = shape_patch_page(files.to_vec(), &window(fields));
        for row in &page.rows {
            let name = str_at(row, "/filename").unwrap_or("").to_owned();
            if row.get("patchUnavailable").is_some() {
                patchless.push(name.clone());
            }
            if let Some(text) = row.get("patch").and_then(Value::as_str) {
                patches.entry(name).or_default().push_str(text);
            }
        }
        let Some(next) = page.cursor else {
            return (calls, patches, patchless);
        };
        assert!(next > offset, "cursor must advance");
        offset = next;
    }
}

/// A patch identical to an earlier one on the page (one README copied to
/// four packages: 37% of bgauryy/octocode#472's patch stream) is delivered
/// once; each repeat names the row that holds it, and the stream shrinks
/// by the repeats, so a walk still reaches every distinct patch exactly once.
#[test]
fn identical_patches_are_delivered_once_and_referenced() {
    let readme = format!("@@ -0,0 +1,2 @@\n+{}\n+end\n", "r".repeat(40));
    let files = vec![
        file("a/README.md", &readme),
        file("src/x.rs", "@@ -1 +1 @@\n-x\n+y\n"),
        file("b/README.md", &readme),
        file("c/README.md", &readme),
    ];
    let page = shape_patch_page(files.clone(), &window(json!({})));
    let rows = page
        .rows
        .iter()
        .map(|row| {
            (
                row["filename"].clone(),
                row.get("patch").is_some(),
                row["samePatchAs"].clone(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rows,
        [
            (json!("a/README.md"), true, Value::Null),
            (json!("src/x.rs"), true, Value::Null),
            (json!("b/README.md"), false, json!("a/README.md")),
            (json!("c/README.md"), false, json!("a/README.md")),
        ]
    );
    let distinct = page.rows[0]["patch"]
        .as_str()
        .map_or(0, |p| p.chars().count())
        + page.rows[1]["patch"]
            .as_str()
            .map_or(0, |p| p.chars().count());
    assert_eq!(page.stream.to_value()["total"], distinct);
    // A walk in small windows lists each repeat once.
    let (_, patches, _) = follow_commit_page(&files, Some(20));
    assert!(patches.contains_key("a/README.md") && patches.contains_key("src/x.rs"));
    assert!(!patches.contains_key("b/README.md"), "{patches:?}");
}

/// A repeat names only text already delivered: it is listed on the window
/// that finishes its referent, never before (here the referent sits behind
/// a patch larger than the window, so it arrives on a later window).
#[test]
fn a_repeat_is_listed_once_its_referent_is_delivered() {
    let readme = format!("@@ -0,0 +1,2 @@\n+{}\n+end\n", "r".repeat(40));
    let big = format!("@@ -0,0 +1,40 @@\n{}", "+line of the big file\n".repeat(40));
    let files = vec![
        file("big.rs", &big),
        file("a/README.md", &readme),
        file("b/README.md", &readme),
    ];
    let (mut offset, mut calls) = (0usize, 0);
    let mut done = std::collections::HashSet::<String>::new();
    let mut repeats = Vec::new();
    loop {
        calls += 1;
        assert!(calls < 1_000, "cursor did not advance");
        let page = shape_patch_page(
            files.clone(),
            &window(json!({"offset":offset,"length":200})),
        );
        for row in &page.rows {
            let name = str_at(row, "/filename").unwrap_or("").to_owned();
            if let Some(earlier) = row["samePatchAs"].as_str() {
                assert!(
                    done.contains(earlier),
                    "{name} names unread {earlier}: {row}"
                );
                repeats.push((calls, name));
            } else if row["patchPagination"]["hasMore"] != true {
                done.insert(name);
            }
        }
        let Some(next) = page.cursor else { break };
        offset = next;
    }
    assert!(calls > 2, "{calls}");
    assert_eq!(repeats.len(), 1, "{repeats:?}");
    assert!(repeats[0].0 > 1, "listed on the first window: {repeats:?}");
}

/// The page budget is not split evenly across files. Small patches
/// arrive whole, a patch larger than the window continues inside the same
/// file, and the call count is the stream length over the window.
#[test]
fn commit_patches_pack_whole_files_and_continue_inside_a_large_one() {
    // Distinct patches: identical ones are delivered once.
    let small = |i: usize| format!("{i:03}{}", "s".repeat(297));
    let mut files = (0..29)
        .map(|i| file(&format!("small{i}.rs"), &small(i)))
        .collect::<Vec<_>>();
    files.insert(3, file("big.rs", &"b".repeat(35_000)));
    let first = shape_patch_page(files.clone(), &window(json!({})));
    // The first three small files arrive whole, not as 266-char slices.
    for row in &first.rows[..3] {
        assert_eq!(row["patch"].as_str().map(str::len), Some(300), "{row}");
        assert!(row.get("patchPagination").is_none(), "{row}");
    }
    let window_chars = patch_window(None, None);
    let big = &first.rows[3];
    assert_eq!(big["patchPagination"]["offset"], 0);
    assert_eq!(big["patchPagination"]["length"], window_chars - 900);
    assert_eq!(first.cursor, Some(window_chars));
    // Files not reached yet ride the continuation, not placeholder rows.
    assert_eq!(first.rows.len(), 4);
    assert_eq!(first.unfinished.len(), 27);

    let second = shape_patch_page(files.clone(), &window(json!({"offset":window_chars})));
    // Completed files are not re-emitted; the big patch continues in place.
    assert_eq!(second.rows.len(), 1);
    assert_eq!(second.rows[0]["filename"], "big.rs");
    assert_eq!(
        second.rows[0]["patchPagination"]["offset"],
        window_chars - 900
    );

    let total: usize = 29 * 300 + 35_000;
    let (calls, patches, _) = follow_commit_page(&files, None);
    // Windows end at whole files, so the walk takes at most one call
    // more than the stream over the window.
    assert!(calls <= total.div_ceil(window_chars) + 1, "{calls}");
    assert_eq!(patches["big.rs"], "b".repeat(35_000));
    for i in 0..29 {
        assert_eq!(patches[&format!("small{i}.rs")], small(i));
    }
}

#[test]
fn single_file_and_three_hundred_file_commits_stay_lossless() {
    let one = vec![file("only.rs", &"x".repeat(20_000))];
    let (calls, patches, _) = follow_commit_page(&one, None);
    assert_eq!(calls, 20_000usize.div_ceil(patch_window(None, None)));
    assert_eq!(patches["only.rs"], "x".repeat(20_000));

    let many = (0..300)
        .map(|i| file(&format!("f{i}.rs"), &format!("+{i}\n")))
        .collect::<Vec<_>>();
    let page = shape_patch_page(many.clone(), &window(json!({})));
    assert_eq!(page.rows.len(), 300);
    assert!(page.unfinished.is_empty());
    assert!(page.cursor.is_none());
    let (calls, patches, _) = follow_commit_page(&many, Some(50));
    assert!(calls > 1);
    for (i, source) in many.iter().enumerate() {
        assert_eq!(
            patches[&format!("f{i}.rs")],
            source["patch"].as_str().unwrap_or("")
        );
    }
}

#[test]
fn binary_files_are_reported_once_and_never_block_the_cursor() {
    let files = vec![
        file("a.rs", &"a".repeat(10)),
        json!({"filename":"logo.png","status":"added"}),
        file("b.rs", &"b".repeat(10)),
        json!({"filename":"tail.bin","status":"added"}),
    ];
    let (calls, patches, patchless) = follow_commit_page(&files, Some(4));
    assert_eq!(calls, 5);
    assert_eq!(patchless, ["logo.png", "tail.bin"]);
    assert_eq!(patches["a.rs"], "a".repeat(10));
    assert_eq!(patches["b.rs"], "b".repeat(10));
    let only_binary = vec![json!({"filename":"logo.png","status":"added"})];
    let page = shape_patch_page(only_binary, &window(json!({})));
    assert_eq!(page.rows[0]["patchUnavailable"], "binary");
    assert!(page.unfinished.is_empty());
}

#[test]
fn missing_patches_name_why_github_sent_none() {
    let file = |name: &str, status: &str, additions: u64| json!({"filename":name,"status":status,"additions":additions,"deletions":0});
    assert_eq!(
        missing_patch_reason(&file("src/checker.ts", "modified", 9)),
        Some("tooLarge")
    );
    assert_eq!(
        missing_patch_reason(&file("app/favicon.ICO", "modified", 0)),
        Some("binary")
    );
    assert_eq!(
        missing_patch_reason(&file("src/core.ts", "modified", 0)),
        Some("omitted")
    );
    assert_eq!(
        missing_patch_reason(&file("src/new.rs", "renamed", 0)),
        None
    );
}

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
fn windows_end_at_a_file_boundary_unless_one_file_overflows() {
    let files = vec![file("a.rs", "AB"), file("b.rs", "CDEF"), file("c.rs", "G")];
    let query = |offset: usize| window(json!({"offset":offset,"length":4}));
    let shown = |page: &PatchPage| {
        page.rows
            .iter()
            .map(|row| row["patch"].as_str().unwrap_or("").to_owned())
            .collect::<Vec<_>>()
    };
    // a.rs fits; b.rs does not fit the rest of the window, so it starts
    // on the next call instead of being cut.
    let first = shape_patch_page(files.clone(), &query(0));
    assert_eq!(shown(&first), ["AB"]);
    assert_eq!(first.unfinished, ["b.rs", "c.rs"]);
    assert_eq!(first.cursor, Some(2));
    let second = shape_patch_page(files, &query(2));
    assert_eq!(shown(&second), ["CDEF"]);
    assert_eq!(second.cursor, Some(6));
    // A first file larger than the window is cut at the window end.
    let big = vec![file("big.rs", "abcdefgh"), file("c.rs", "G")];
    let cut = shape_patch_page(big, &query(0));
    assert_eq!(shown(&cut), ["abcd"]);
    assert_eq!(cut.unfinished, ["big.rs", "c.rs"]);
    assert_eq!(cut.cursor, Some(4));
    // A file no window holds whole starts at once after the whole ones.
    let early = vec![file("a.rs", "AB"), file("big.rs", "abcdefgh")];
    let started = shape_patch_page(early, &query(0));
    assert_eq!(shown(&started), ["AB", "ab"]);
    assert_eq!(started.unfinished, ["big.rs"]);
    assert_eq!(started.cursor, Some(4));
}

/// A patch cut inside a file ends at the last line end the window holds,
/// so no window splits a diff line; a window with no line end in it is
/// cut at its end.
#[test]
fn a_cut_patch_ends_at_a_line_boundary() {
    let lines = vec![file("a.rs", "+one\n+two\n+three\n")];
    let page = shape_patch_page(lines.clone(), &window(json!({"length":12})));
    assert_eq!(page.rows[0]["patch"], "+one\n+two\n");
    assert_eq!(page.cursor, Some(10));
    let rest = shape_patch_page(lines, &window(json!({"offset":10,"length":12})));
    assert_eq!(rest.rows[0]["patch"], "+three\n");
    assert_eq!(rest.cursor, None);
    let long = shape_patch_page(vec![file("a.rs", "abcdefgh")], &window(json!({"length":4})));
    assert_eq!(long.cursor, Some(4));
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

/// `matchString` narrows a patch to the matching lines plus context under
/// recomputed hunk headers; a path-only match keeps the whole patch.
#[test]
fn match_string_patch_view_keeps_only_matching_hunks() {
    let body = (1..=20)
        .map(|n| format!(" line {n}\r\n"))
        .collect::<String>();
    let patch = format!(
        "@@ -1,22 +1,22 @@ fn main\r\n{body}-old Needle\r\n+new needle\r\n{body}@@ -80,3 +80,3 @@\n x\n-y\n+z"
    );
    let view = matching_hunks(&patch, "needle", 3, None)
        .expect("a line matches")
        .text;
    assert_eq!(
        view,
        "@@ -18,4 +18,4 @@ fn main\n line 18\r\n line 19\r\n line 20\r\n-old Needle\r\n+new needle\r\n line 1\r\n line 2\r\n line 3\r\n"
            .replace("@@ -18,4 +18,4 @@", "@@ -18,7 +18,7 @@")
    );
    assert!(matching_hunks(&patch, "absent", 3, None).is_none());
    // contextLines 0: only the hit lines, under one header per run.
    assert_eq!(
        matching_hunks(&patch, "needle", 0, None)
            .map(|found| found.text)
            .as_deref(),
        Some("@@ -21,1 +21,1 @@ fn main\n-old Needle\r\n+new needle\r\n")
    );
    // A generated one-line diff keeps only the text around each hit.
    let long = format!(
        "@@ -1 +1 @@\n+{}PointerEvent{}\r\n",
        "a".repeat(1_000),
        "b".repeat(1_000)
    );
    let clipped = matching_hunks(&long, "pointerevent", 3, None)
        .expect("hit")
        .text;
    assert_eq!(
        clipped,
        format!(
            "@@ -1,0 +1,1 @@\n+[… 850 chars …]{}PointerEvent{}[… 850 chars …]\r\n",
            "a".repeat(150),
            "b".repeat(150)
        )
    );
    let query = patch_request(json!({"matchString":"NEEDLE","contextLines":3}));
    let page = shape_patch_page(vec![file("src/a.rs", &patch)], &query);
    assert_eq!(page.rows[0]["patch"], number_patch(&view));
    assert_eq!(page.rows[0]["fullPatchChars"], patch.chars().count());
}

/// A run with no line on one side starts that side at its own line before
/// the run, as a unified diff does: an addition after old line 10, a
/// deletion after new line 11, a whole-file delete at new line 0, never
/// the other side's number.
#[test]
fn a_one_sided_run_starts_from_its_own_side() {
    let patch = "@@ -8,6 +8,7 @@ fn f\n a\n b\n c\n+x\n+needle add\n d\n-needle gone\n e\n";
    let header = |needle: &str| {
        matching_hunks(patch, needle, 0, None)
            .expect("hit")
            .text
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned()
    };
    // Old lines 8-10 (a, b, c) precede the additions at new 11-12.
    assert_eq!(header("needle add"), "@@ -10,0 +12,1 @@ fn f");
    // New line 13 (d) precedes the deletion of old line 12.
    assert_eq!(header("needle gone"), "@@ -12,1 +13,0 @@ fn f");
    // A deletion with no new line in its hunk: the header's own start.
    let deleted = "@@ -1,3 +0,0 @@\n-one\n-needle\n-three\n";
    let whole = matching_hunks(deleted, "needle", 0, None)
        .expect("hit")
        .text;
    assert!(whole.starts_with("@@ -2,1 +0,0 @@"), "{whole}");
    // A whole-file addition: old side 0.
    let added = "@@ -0,0 +1,2 @@\n+one\n+needle\n";
    let new_file = matching_hunks(added, "needle", 0, None).expect("hit").text;
    assert!(new_file.starts_with("@@ -0,0 +2,1 @@"), "{new_file}");
}

/// Every patch view numbers its new side: kept and added lines carry
/// their new-file line, removed lines and `\ No newline` markers a bare
/// tab; headers (and their enclosing-symbol heading) stay verbatim, and
/// dropping the gutter restores the raw patch.
#[test]
fn patches_number_the_new_side_and_keep_hunk_headers() {
    let patch = "@@ -84,4 +84,4 @@ cfg_io_util! {\n \n-    old\r\n+    new\r\n     const X: usize = 1;\n@@ -1 +0,0 @@\n-gone\n\\ No newline at end of file\n@@ -0,0 +7,2 @@ impl A\n+a\n+b";
    let numbered = number_patch(patch);
    assert_eq!(
        numbered,
        "@@ -84,4 +84,4 @@ cfg_io_util! {\n84\t \n85\t-    old\r\n85\t+    new\r\n86\t     const X: usize = 1;\n@@ -1 +0,0 @@\n1\t-gone\n\t\\ No newline at end of file\n@@ -0,0 +7,2 @@ impl A\n7\t+a\n8\t+b"
    );
    let raw = numbered
        .split_inclusive('\n')
        .map(|line| match line.split_once('\t') {
            Some((_, text)) if !line.starts_with("@@") => text,
            _ => line,
        })
        .collect::<String>();
    assert_eq!(raw, patch);
    // Not a hunk stream (a line-filtered `+N:` view, empty): unchanged.
    assert_eq!(number_patch("+3: x"), "+3: x");
    assert_eq!(number_patch(""), "");
}

/// A patch-reading page that does not fit one response lists every file
/// of the page first: status, change counts, hunks, and why a file has
/// no patch. A page that fits shows the patches alone.
#[test]
fn unfinished_patch_pages_open_with_a_per_file_summary() {
    let files = vec![
        json!({"filename":"src/a.rs","status":"modified","additions":2,"deletions":1,"sha":"1",
               "patch":"@@ -1,2 +1,3 @@ fn a\n x\n+y\n@@ -9 +10 @@\n-p\n+q"}),
        json!({"filename":"src/b.rs","status":"added","additions":900,"deletions":0,"sha":"2"}),
        json!({"filename":"README.md","status":"renamed","additions":1,"deletions":0,"sha":"3",
               "previous_filename":"OLD.md","patch":"@@ -1 +1,2 @@\n x\n+y"}),
    ];
    let summary = |fields: Value| {
        let mut row = json!({});
        let mut pagination = Map::new();
        shape_pr_files(
            &mut row,
            &mut pagination,
            files.clone(),
            WindowState::COMPLETE,
            &patch_request(fields),
            true,
            None,
        );
        row.get("fileSummary").cloned()
    };
    assert_eq!(
        summary(json!({"length":20})),
        Some(json!([
            {"src/": ["M +2 -1 2 hunks a.rs", "A +900 -0 !tooLarge b.rs"]},
            "R +1 -0 1 hunk README.md <- OLD.md"
        ]))
    );
    assert_eq!(summary(json!({})), None);
    // A continuation window does not repeat it.
    assert_eq!(summary(json!({"offset":5,"length":20})), None);
}

/// (PR microsoft/TypeScript#51387 file page 6 at a 40k page): a
/// first window lists every patch-less file of its page (`omitted`,
/// `tooLarge`, binary) as a metadata row beside the summary. Those rows
/// come out of the patch window too, so the row still fits one response
/// page; before, 61 such rows pushed a 40k page to 42.6k and the
/// response fell back to text windows that hide the structured rows.
#[test]
fn first_window_reserves_patchless_rows_and_summary() {
    let page = 40_000;
    let mut files = (0..90)
        .map(|i| {
            json!({"filename":format!("src/testRunner/unittests/tsserver/someLongDirectory/file{i:03}.ts"),
                   "status":"modified","additions":0,"deletions":0,"sha":"1"})
        })
        .collect::<Vec<_>>();
    files.extend((0..4).map(|i| {
        let patch = format!("@@ -1,900 +1,900 @@\n{}", "-old line of code here\n+new line of code here\n".repeat(450));
        json!({"filename":format!("src/big{i}.ts"),"status":"modified","additions":450,"deletions":450,"sha":"2","patch":patch})
    }));
    let mut query = patch_request(json!({}));
    query.auto_page_chars = Some(page);
    let mut row = json!({});
    let mut pagination = Map::new();
    shape_pr_files(
        &mut row,
        &mut pagination,
        files,
        WindowState::COMPLETE,
        &query,
        true,
        None,
    );
    assert!(row.get("fileSummary").is_some(), "{row}");
    assert!(pagination.contains_key("patches"), "the patches continue");
    // The row may use the page's patch share; the rest of the page holds
    // the envelope (identity, pagination, continuation, warnings).
    // Patch text renders verbatim in the text channel: count its chars,
    // not its JSON escapes.
    let mut meta = row.clone();
    let mut patch_chars = 0;
    for file in meta["files"].as_array_mut().into_iter().flatten() {
        if let Some(patch) = file.get("patch").and_then(Value::as_str) {
            patch_chars += patch.chars().count();
            file["patch"] = json!("");
        }
    }
    let chars = octocode_engine::json_size::utf16_units(&meta).unwrap_or(0) + patch_chars;
    // The envelope around the row (identity, pagination, continuation,
    // warnings) takes well under 2k chars.
    assert!(chars <= page - 2_000, "row {chars} chars on a {page} page");
}

/// Code hunks are never minified: a PR patch page returns every context
/// line, so no row is marked for a re-read.
#[test]
fn patch_rows_are_never_minified() {
    let long = "@@ -1,41 +1,41 @@\n".to_owned()
        + &(1..=40)
            .map(|n| format!(" ctx {n}\n"))
            .chain(["-old\n".to_owned(), "+new\n".to_owned()])
            .collect::<String>();
    let page = shape_patch_page(vec![file("big.rs", &long)], &patch_request(json!({})));
    assert_eq!(page.rows[0]["patch"], number_patch(&long));
    assert!(!number_patch(&long).contains("\n...\n"));
    assert!(
        page.rows[0].get("fullPatchChars").is_none(),
        "{:?}",
        page.rows
    );
}

/// A hunk with GitHub's 3 context lines around a hit at line 53 of a
/// 100-line file.
fn hunk_at_53() -> (String, Vec<String>) {
    let patch = "@@ -50,7 +50,7 @@ fn f\n line 50\n line 51\n line 52\n-old 53\n+new needle 53\n line 54\n line 55\n line 56\n".to_owned();
    let head = (1..=100)
        .map(|n| {
            if n == 53 {
                "new needle 53".to_owned()
            } else {
                format!("line {n}")
            }
        })
        .collect();
    (patch, head)
}

/// contextLines past the hunk's own context: with the head text the run
/// reaches into the unchanged lines around the hunk (new and old line
/// numbers kept), so 3 and 10 no longer read the same lines.
#[test]
fn match_context_past_the_hunk_widens_from_the_head_text() {
    let (patch, head) = hunk_at_53();
    let found = matching_hunks(&patch, "needle", 10, Some(&head)).expect("hit");
    assert!(found.clipped.is_empty());
    assert!(
        found
            .text
            .starts_with("@@ -44,20 +44,20 @@ fn f\n line 44\n"),
        "{}",
        found.text
    );
    assert!(found.text.ends_with(" line 63\n"), "{}", found.text);
    assert!(found.text.contains("-old 53\n+new needle 53\n"));
    let narrow = matching_hunks(&patch, "needle", 3, Some(&head)).expect("hit");
    assert_ne!(narrow.text, found.text);
    // A head that disagrees with the diff is not this diff's head.
    let mut stale = head.clone();
    stale[52] = "something else".into();
    let fallback = matching_hunks(&patch, "needle", 10, Some(&stale)).expect("hit");
    assert_eq!(fallback.clipped, ["43-63"]);
}

/// GitHub ends a patch without a final newline: context widened from
/// the head text past the last hunk starts on its own line, so every
/// later line keeps its number (byte-exact numbered view).
#[test]
fn head_context_after_the_last_hunk_keeps_line_boundaries() {
    let (patch, head) = hunk_at_53();
    let patch = patch.trim_end_matches('\n');
    let found = matching_hunks(patch, "needle", 6, Some(&head)).expect("hit");
    assert_eq!(
        number_patch(&found.text),
        "@@ -48,12 +48,12 @@ fn f\n48\t line 48\n49\t line 49\n50\t line 50\n51\t line 51\n52\t line 52\n53\t-old 53\n53\t+new needle 53\n54\t line 54\n55\t line 55\n56\t line 56\n57\t line 57\n58\t line 58\n59\t line 59\n"
    );
}

/// Without the head text, a run the hunk cuts short is never silently
/// clipped: it names the new-side window it wanted, and the page flags
/// the file and lists it for `next.expandContext`.
#[test]
fn match_context_past_the_hunk_without_head_text_is_flagged() {
    let (patch, _) = hunk_at_53();
    let found = matching_hunks(&patch, "needle", 10, None).expect("hit");
    assert_eq!(found.clipped, ["43-63"]);
    assert!(head_text_needed(&patch, "needle", 10));
    assert!(!head_text_needed(&patch, "needle", 3));
    let query = patch_request(json!({"matchString":"needle","contextLines":10}));
    let page = shape_patch_page(vec![file("src/a.rs", &patch)], &query);
    assert_eq!(page.rows[0]["contextClipped"], true, "{:?}", page.rows);
    assert_eq!(
        page.clipped,
        vec![("src/a.rs".to_owned(), vec!["43-63".to_owned()])]
    );
    let mut sources = HeadSources::default();
    sources.insert("src/a.rs".into(), Some(hunk_at_53().1));
    let widened = HistoryItemRequest {
        head_sources: Some(std::sync::Arc::new(sources)),
        ..query
    };
    let page = shape_patch_page(vec![file("src/a.rs", &patch)], &widened);
    assert!(page.clipped.is_empty());
    assert!(page.rows[0].get("contextClipped").is_none());
}

/// `matchString` keeps the contract's default context lines
/// (`limits.defaultReadContextLines`) around each hit and pages every hit
/// file of the PR at once, not 30 per call.
#[test]
fn match_string_defaults_to_the_contract_context_and_one_page_of_hit_files() {
    assert_eq!(
        match_context(&patch_request(json!({"matchString":"miri"}))),
        crate::id::DEFAULT_READ_CONTEXT_LINES
    );
    let patch = "@@ -1,3 +1,3 @@\n a\n-old miri\n+new miri\n b\n";
    let query = patch_request(json!({"matchString":"miri"}));
    let view = history_patch_view(patch, "a.rs", &query);
    assert_eq!(
        (view.text.as_str(), view.narrowed, view.clipped.len()),
        (
            "@@ -1,3 +1,3 @@\n1\t a\n2\t-old miri\n2\t+new miri\n3\t b\n",
            true,
            0
        )
    );
    let hits_only = patch_request(json!({"matchString":"miri","contextLines":0}));
    assert_eq!(
        history_patch_view(patch, "a.rs", &hits_only).text,
        "@@ -2,1 +2,1 @@\n2\t-old miri\n2\t+new miri\n"
    );
    assert_eq!(
        file_page_size(&query, true),
        super::super::window::MAX_FILE_BATCHES * MAX_PER_PAGE
    );
    let explicit = patch_request(json!({"matchString":"miri","pageSize":5}));
    assert_eq!(file_page_size(&explicit, true), 5);
}

/// A patch page emits only rows that carry patch text (or say why a
/// file has none); files not reached yet ride the continuation, not
/// empty placeholder rows. PR rows carry one compact `stat`.
#[test]
fn patch_pages_emit_no_placeholder_rows_and_compact_headers() {
    let files = vec![
        json!({"filename":"a.rs","status":"modified","additions":2,"deletions":1,"patch":"A".repeat(10)}),
        json!({"filename":"b.rs","status":"added","additions":9,"deletions":0,"patch":"B".repeat(10)}),
        json!({"filename":"c.rs","status":"modified","additions":1,"deletions":1,"patch":"C".repeat(10)}),
    ];
    let page = shape_patch_page(
        files.clone(),
        &patch_request(json!({"offset":0,"length":10})),
    );
    assert_eq!(page.rows.len(), 1, "{:?}", page.rows);
    assert_eq!(page.unfinished, ["b.rs", "c.rs"]);
    assert_eq!(page.cursor, Some(10));
    let mut row = json!({});
    let mut pagination = Map::new();
    let shaped = shape_pr_files(
        &mut row,
        &mut pagination,
        files,
        WindowState::COMPLETE,
        &patch_request(json!({"offset":0,"length":15})),
        true,
        None,
    );
    // b.rs does not fit the rest of the window: it starts on the next
    // page whole instead of as a cut placeholder.
    assert_eq!(
        row["files"],
        json!([{"path":"a.rs","stat":"M +2 -1","patch":"AAAAAAAAAA"}])
    );
    assert_eq!(pagination["patches"]["nextOffset"], 10);
    assert_eq!(shaped.unfinished_patches, 2);
}

/// The review pick names every source file by churn within
/// one budget, skipping tests, docs and lockfiles.
#[test]
fn review_selection_packs_source_files_by_churn() {
    let f = |name: &str, changed: u64| json!({"filename":name,"additions":changed,"deletions":0});
    let files = vec![
        f(".changeset/x.md", 8),
        f("pydantic/json_schema.py", 40),
        f("tests/test_counter.py", 400),
        f("pydantic/_known_annotated_metadata.py", 12),
        f("pydantic/fields.py", 90),
        f("uv.lock", 900),
        f("pyproject.toml", 2),
        f("docs/img/diagram.png", 0),
    ];
    assert_eq!(
        review_selection(&files, 15_000),
        [
            "pydantic/fields.py",
            "pydantic/json_schema.py",
            "pydantic/_known_annotated_metadata.py",
            "pyproject.toml"
        ]
    );
    // A tight budget leads with the files that fit whole; the larger
    // ones follow (read through continuePatch), never dropped.
    assert_eq!(
        review_selection(&files, 2_000),
        [
            "pydantic/json_schema.py",
            "pydantic/fields.py",
            "pydantic/_known_annotated_metadata.py",
            "pyproject.toml"
        ]
    );
    assert!(review_selection(&[f("tests/a_test.py", 3)], 15_000).is_empty());
    // Tests named by their language's convention stay out of the review
    // pick wherever they live.
    let polyglot = vec![
        f("pkg/cmd/discussion/client/client_test.go", 3_842),
        f("pkg/cmd/discussion/create/create.go", 120),
        f("src/test/java/com/acme/WidgetTest.java", 300),
        f("src/main/java/com/acme/Latest.java", 10),
        f("spec/models/user_spec.rb", 80),
        f("app/test_api.py", 60),
    ];
    assert_eq!(
        review_selection(&polyglot, 15_000),
        [
            "pkg/cmd/discussion/create/create.go",
            "src/main/java/com/acme/Latest.java"
        ]
    );
    // A literal search fills the page share without the metadata reserve.
    assert_eq!(literal_patch_window(None), 16_000);
    assert_eq!(literal_patch_window(Some(20_000)), 16_000);
}

/// Commit rows use the PR header (`path` + `stat`), and a
/// commit's `files` scope takes paths and globs like a PR's.
#[test]
fn commit_rows_are_compact_and_files_scope_them() {
    let rows = shape_patch_page(
        vec![
            json!({"filename":"src/a.rs","status":"added","additions":3,"deletions":0,"patch":"+a"}),
        ],
        &window(json!({})),
    )
    .compact()
    .rows;
    assert_eq!(
        rows,
        vec![json!({"path":"src/a.rs","stat":"A +3 -0","patch":"+a"})]
    );
    let scoped = HistoryItemRequest::from_row(json!({
        "operation":"commit","mainGoal":"g","reasoning":"r","owner":"o","repo":"r",
        "ref":"abc","include":["*.md","src/"]
    }))
    .expect("commit files");
    let scope = FileScope::from_query(&scoped)
        .expect("valid")
        .expect("scope");
    let names = ["src/a.rs", "README.md", "lib/b.rs"]
        .into_iter()
        .filter(|name| scope.matches(&json!({"filename": name})))
        .collect::<Vec<_>>();
    assert_eq!(names, ["src/a.rs", "README.md"]);
}

fn inventory_request(fields: Value) -> HistoryItemRequest {
    patch_request(super::super::util::merge(
        json!({"sections":["files"]}),
        fields,
    ))
}

fn listed(name: &str, status: &str, additions: u64, deletions: u64, patch: bool) -> Value {
    let mut file = json!({"sha":"1","filename":name,"status":status,
        "additions":additions,"deletions":deletions});
    if patch {
        file["patch"] = json!("@@ -1 +1 @@");
    }
    file
}

fn inventory(files: Vec<Value>, query: &HistoryItemRequest) -> (Value, Map<String, Value>) {
    let mut row = json!({});
    let mut pagination = Map::new();
    let scope = FileScope::from_query(query).expect("valid file filter");
    shape_pr_files(
        &mut row,
        &mut pagination,
        files,
        WindowState::COMPLETE,
        query,
        false,
        scope.as_ref(),
    );
    (row, pagination)
}

/// A patch-free inventory row is one compact string; files of one
/// directory share a `{"dir/": [...]}` group so the prefix is written
/// once. Patchless reasons and rename origins survive.
#[test]
fn inventory_rows_are_compact_and_group_directory_files() {
    let mut renamed = listed("src/new.rs", "renamed", 0, 0, false);
    renamed["previous_filename"] = json!("lib/old.rs");
    let files = vec![
        listed(".eslintrc", "modified", 1, 1, true),
        listed("src/a.ts", "added", 8, 0, true),
        listed("src/checker.ts", "modified", 39_550, 39_342, false),
        renamed,
        listed("src/ui/logo.png", "added", 0, 0, false),
        listed("src/z.ts", "removed", 0, 5, true),
        listed("docs/omitted.md", "modified", 0, 0, false),
    ];
    let (row, pagination) = inventory(files, &inventory_request(json!({})));
    assert_eq!(
        row["files"],
        json!([
            "M +1 -1 .eslintrc",
            {"src/": [
                "A +8 -0 a.ts",
                "M +39550 -39342 !tooLarge checker.ts",
                "R +0 -0 new.rs <- lib/old.rs",
                "D +0 -5 z.ts"
            ]},
            "A +0 -0 !binary src/ui/logo.png",
            "M !omitted docs/omitted.md"
        ])
    );
    assert_eq!(pagination["files"]["totalItems"], 7);
}

/// A directory appears once in a page's inventory: files of one
/// directory that a deeper directory's files separate in provider order
/// still share the one group, which sits where the directory first appears.
#[test]
fn inventory_merges_a_directory_split_by_a_deeper_directory() {
    let files = vec![
        listed("src/a.ts", "modified", 1, 1, true),
        listed("src/sub/deep.ts", "added", 2, 0, true),
        listed("src/b.ts", "modified", 1, 1, true),
        listed("src/sub/other.ts", "modified", 1, 1, true),
        listed("tail.md", "modified", 1, 1, true),
        listed("src/c.ts", "removed", 0, 5, true),
    ];
    let (row, pagination) = inventory(files, &inventory_request(json!({})));
    assert_eq!(
        row["files"],
        json!([
            {"src/": ["M +1 -1 a.ts", "M +1 -1 b.ts", "D +0 -5 c.ts"]},
            {"src/sub/": ["A +2 -0 deep.ts", "M +1 -1 other.ts"]},
            "M +1 -1 tail.md"
        ])
    );
    assert_eq!(pagination["files"]["totalItems"], 6);
}

/// `include`/`status`/`minChanges` narrow the inventory by status, path glob or prefix and
/// change count; totals count matches only. A file GitHub sent without
/// line counts is not "small": it passes `minChanges`.
#[test]
fn file_filter_narrows_by_status_path_and_change_count() {
    let files = || {
        vec![
            listed("src/a.ts", "modified", 3, 1, true),
            listed("src/deep/b.ts", "added", 40, 0, true),
            listed("src/c.rs", "modified", 50, 0, true),
            listed("docs/readme.md", "modified", 9, 9, true),
            listed("src/huge.ts", "modified", 0, 0, false),
        ]
    };
    let names = |fields: Value| {
        let (row, pagination) = inventory(files(), &inventory_request(fields));
        let mut out = Vec::new();
        for item in row["files"].as_array().into_iter().flatten() {
            match item {
                Value::String(row) => out.push(row.rsplit(' ').next().unwrap_or("").to_owned()),
                Value::Object(group) => {
                    for (dir, rows) in group {
                        for row in rows.as_array().into_iter().flatten() {
                            let name = row.as_str().unwrap_or("").rsplit(' ').next().unwrap_or("");
                            out.push(format!("{dir}{name}"));
                        }
                    }
                }
                _ => {}
            }
        }
        assert_eq!(pagination["files"]["totalItems"], out.len());
        out
    };
    // "**" spans zero or more directories; "*" stays in one segment.
    assert_eq!(
        names(json!({"include":["src/**/*.ts"]})),
        ["src/a.ts", "src/huge.ts", "src/deep/b.ts"]
    );
    assert_eq!(
        names(json!({"include":["src/*.ts"]})),
        ["src/a.ts", "src/huge.ts"]
    );
    assert_eq!(
        names(json!({"include":["*.ts"]})),
        ["src/a.ts", "src/huge.ts", "src/deep/b.ts"]
    );
    assert_eq!(
        names(json!({"include":["src/deep", "docs/"]})),
        ["src/deep/b.ts", "docs/readme.md"]
    );
    assert_eq!(names(json!({"status":["added"]})), ["src/deep/b.ts"]);
    assert_eq!(
        names(json!({"include":["*.ts"],"status":["modified"],"minChanges":10})),
        ["src/huge.ts"]
    );
    let invalid = inventory_request(json!({"include":["src/[a"]}));
    assert!(FileScope::from_query(&invalid).is_err());
}

/// An omitted pageSize sizes a patch-free inventory to the response page
/// and a patch read to one provider batch (one continuation stream walks
/// every patch of such a PR); an explicit one may exceed a provider
/// batch only without patches.
#[test]
fn inventory_page_size_fills_the_response_page() {
    let mut query = inventory_request(json!({}));
    query.auto_page_chars = Some(50_000);
    assert_eq!(file_page_size(&query, false), 833);
    assert_eq!(file_page_size(&query, true), MAX_PER_PAGE);
    let small = inventory_request(json!({"pageSize":5}));
    assert_eq!(file_page_size(&small, true), 5);
    query.auto_page_chars = Some(1_000);
    assert_eq!(file_page_size(&query, false), MAX_PER_PAGE);
    let explicit = inventory_request(json!({"pageSize":1000}));
    assert_eq!(file_page_size(&explicit, false), 1_000);
    assert_eq!(file_page_size(&explicit, true), MAX_PER_PAGE);
    let files = (0..700)
        .map(|i| listed(&format!("d{}/f{i}.rs", i % 2), "modified", 1, 1, true))
        .collect::<Vec<_>>();
    let (row, pagination) = inventory(files, &explicit);
    // Two directories, each one group of 350 rows.
    let rows: usize = row["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|group| group.as_object().into_iter().flat_map(|g| g.values()))
        .filter_map(|rows| rows.as_array().map(Vec::len))
        .sum();
    assert_eq!(rows, 700);
    assert_eq!(row["files"].as_array().map(Vec::len), Some(2));
    assert_eq!(pagination["files"]["hasMore"], false);
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

/// A whole-file boundary that fills under 3/4 of the window does
/// not end it; the window packs on into the next file up to its last
/// line end, and the next window resumes there.
#[test]
fn history_b_windows_pack_past_a_low_file_boundary_at_a_line_end() {
    let lines = |tag: &str, count: usize| format!("+{tag}{}\n", "x".repeat(7)).repeat(count);
    // Two 10k files at the 15k default window: the boundary fills 2/3.
    let files = vec![
        file("a.rs", &lines("a", 1_000)),
        file("b.rs", &lines("b", 1_000)),
    ];
    let window_chars = patch_window(None, None);
    let first = shape_patch_page(files.clone(), &window(json!({})));
    let cursor = first.cursor.expect("b.rs continues");
    assert!(cursor > 10_000, "packed past the boundary: {cursor}");
    assert!(cursor * 4 >= window_chars * 3, "{cursor}");
    assert!(cursor <= window_chars);
    let b = &first.rows[1];
    let text = b["patch"].as_str().expect("b patch");
    assert!(text.ends_with('\n'), "cut at a line end");
    assert_eq!(b["patchPagination"]["hasMore"], true);
    let (calls, patches, _) = follow_commit_page(&files, None);
    assert_eq!(calls, 2);
    assert_eq!(patches["a.rs"], lines("a", 1_000));
    assert_eq!(patches["b.rs"], lines("b", 1_000));
    // A next file with no line end in reach keeps the boundary.
    let flat = vec![
        file("a.rs", &"a".repeat(10_000)),
        file("b.rs", &"b".repeat(10_000)),
    ];
    let page = shape_patch_page(flat, &window(json!({})));
    assert_eq!(page.cursor, Some(10_000));
}

/// A packed walk over many mid-size patches is lossless (no gap,
/// no repeat), every window but the last is at least 3/4 full, and the
/// call count is within one of the stream over the window.
#[test]
fn history_b_packed_walk_is_lossless_and_fills_its_windows() {
    let files = (0..40)
        .map(|i| {
            file(
                &format!("f{i}.rs"),
                &format!("+line {i} of the patch\n").repeat(150 + (i * 37) % 400),
            )
        })
        .collect::<Vec<_>>();
    let total = files
        .iter()
        .map(|f| f["patch"].as_str().map_or(0, |p| p.chars().count()))
        .sum::<usize>();
    let window_chars = patch_window(None, None);
    let mut offset = 0usize;
    let mut read = HashMap::<String, String>::new();
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls < 100, "cursor did not advance");
        let page = shape_patch_page(files.clone(), &window(json!({"offset":offset})));
        for row in &page.rows {
            let name = str_at(row, "/filename").unwrap_or("").to_owned();
            let have = read.entry(name).or_default();
            let at = row["patchPagination"]["offset"].as_u64().unwrap_or(0);
            assert_eq!(at as usize, have.chars().count(), "gap or repeat: {row}");
            have.push_str(row["patch"].as_str().unwrap_or(""));
        }
        match page.cursor {
            Some(next) => {
                assert!((next - offset) * 4 >= window_chars * 3, "{}", next - offset);
                offset = next;
            }
            None => break,
        }
    }
    assert!(calls <= total.div_ceil(window_chars) + 1, "{calls}");
    for source in &files {
        let name = source["filename"].as_str().unwrap_or("");
        assert_eq!(read.get(name).map(String::as_str), source["patch"].as_str());
    }
}

/// A `matchString` file row counts the hits its window shows
/// (`matchCount`), so "every hit shown" is checkable; a row the needle
/// matched by path only carries none.
#[test]
fn history_b_match_string_rows_count_the_hits_they_show() {
    let patch =
        "@@ -1,9 +1,9 @@ fn f\n a\n-old needle\n+new needle\n b\n c\n d\n e\n f\n+NEEDLE again\n";
    let query = patch_request(json!({"matchString":"needle","contextLines":1}));
    let page = shape_patch_page(
        vec![
            file("hit.rs", patch),
            file("needle_path.rs", "@@ -1 +1 @@\n-x\n+y\n"),
        ],
        &query,
    );
    assert_eq!(page.rows[0]["matchCount"], 3, "{}", page.rows[0]);
    assert!(page.rows[1].get("matchCount").is_none(), "{}", page.rows[1]);
    // A window cut between hits counts only the hits it shows.
    let shown = |offset: usize| {
        let query = patch_request(
            json!({"matchString":"needle","contextLines":1,"offset":offset,"length":60}),
        );
        let page = shape_patch_page(vec![file("hit.rs", patch)], &query);
        (
            page.rows[0]["matchCount"].as_u64().unwrap_or(0),
            page.cursor,
        )
    };
    let (first, cursor) = shown(0);
    let mut total = first;
    let mut offset = cursor;
    while let Some(at) = offset {
        let (count, next) = shown(at);
        total += count;
        offset = next;
    }
    assert!(first < 3, "the first window is cut: {first}");
    assert_eq!(total, 3);
}

/// A page whose patches all fit lists them alone, even when more file
/// pages follow: a summary would only repeat `files[]`.
#[test]
fn a_page_whose_patches_fit_has_no_summary_beside_later_pages() {
    let files = vec![
        json!({"filename":"src/a.rs","status":"modified","additions":1,"deletions":1,"sha":"1",
               "patch":"@@ -1 +1 @@\n-x\n+y"}),
        json!({"filename":"src/b.rs","status":"modified","additions":1,"deletions":1,"sha":"2",
               "patch":"@@ -1 +1 @@\n-p\n+q"}),
    ];
    let mut row = json!({});
    let mut pagination = Map::new();
    shape_pr_files(
        &mut row,
        &mut pagination,
        files,
        WindowState::COMPLETE,
        &patch_request(json!({"pageSize":1})),
        true,
        None,
    );
    assert_eq!(pagination["files"]["hasMore"], true, "{pagination:?}");
    assert!(row.get("fileSummary").is_none(), "{row}");
    assert_eq!(row["files"][0]["path"], "src/a.rs", "{row}");
}

/// A long patch line keeps every keyword window at the keyword itself,
/// whatever lowercasing does to the bytes before it: `İ` (+1 byte) and `ẞ`
/// (−1) cancel to an equal length yet shift every offset, a lone `İ` or
/// `ẞ` changes the length, and combining marks, a final `Σ` and emoji ride
/// along. No window is dropped or moved.
#[test]
fn clipped_patch_lines_keep_keyword_windows_in_any_script() {
    let pad = "x".repeat(300);
    for prefix in [
        "İéxẞ ",
        "DİYARBAKIR İstanbul ı I ",
        "STRAẞE straße ẞ ",
        "e\u{301}cole Ё\u{308} a\u{30a}\u{301} ",
        "ΟΔΟΣ Σ σ ",
        "👍🏽 👩‍👩‍👧 🎉 ",
    ] {
        let line = format!("+{prefix}{pad} NeedleOne {pad} {prefix}NEEDLEONE {pad}\n");
        let clipped = clip_line(&line, "needleone");
        assert_eq!(
            clipped.matches("NeedleOne").count(),
            1,
            "{prefix}: {clipped}"
        );
        assert_eq!(
            clipped.matches("NEEDLEONE").count(),
            1,
            "{prefix}: {clipped}"
        );
        assert!(
            clipped.contains(&format!(" {prefix}NEEDLEONE ")),
            "{clipped}"
        );
        assert!(
            clipped.starts_with('+') && clipped.ends_with('\n'),
            "{clipped}"
        );
        // Every cut states what it left out; the kept text is the line's own.
        let kept: usize = clipped
            .split("[… ")
            .enumerate()
            .map(|(index, part)| {
                if index == 0 {
                    part.chars().count()
                } else {
                    let (count, rest) = part.split_once(" chars …]").expect("cut");
                    count.parse::<usize>().expect("count") + rest.chars().count()
                }
            })
            .sum();
        assert_eq!(kept, line.chars().count(), "{prefix}: {clipped}");
    }
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

/// One distinctive added line per hunk (the longest of at most
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
