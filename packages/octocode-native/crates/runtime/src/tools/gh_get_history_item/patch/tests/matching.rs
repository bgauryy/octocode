//! `matchString` patch views: matching hunks, side numbering, context widening, hit counts and locators.
use super::*;

fn match_patch(
    patch: &str,
    pattern: &str,
    context: usize,
    head: Option<&[String]>,
) -> Option<super::super::hunks::HunkMatch> {
    let request = patch_request(json!({"matchString":[pattern],"caseMode":"insensitive"}));
    super::super::hunks::matching_hunks(
        patch,
        request.matcher.as_deref().expect("matcher"),
        (context, context),
        head,
    )
}

/// `matchString` narrows a patch to the matching lines plus context under
/// recomputed hunk headers; a path-only match keeps the whole patch.
#[test]
fn match_string_patch_view_keeps_only_match_patch() {
    let body = (1..=20)
        .map(|n| format!(" line {n}\r\n"))
        .collect::<String>();
    let patch = format!(
        "@@ -1,22 +1,22 @@ fn main\r\n{body}-old Needle\r\n+new needle\r\n{body}@@ -80,3 +80,3 @@\n x\n-y\n+z"
    );
    let view = match_patch(&patch, "needle", 3, None)
        .expect("a line matches")
        .text;
    assert_eq!(
        view,
        "@@ -18,4 +18,4 @@ fn main\n line 18\r\n line 19\r\n line 20\r\n-old Needle\r\n+new needle\r\n line 1\r\n line 2\r\n line 3\r\n"
            .replace("@@ -18,4 +18,4 @@", "@@ -18,7 +18,7 @@")
    );
    assert!(match_patch(&patch, "absent", 3, None).is_none());
    // contextLines 0: only the hit lines, under one header per run.
    assert_eq!(
        match_patch(&patch, "needle", 0, None)
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
    let clipped = match_patch(&long, "pointerevent", 3, None)
        .expect("hit")
        .text;
    assert_eq!(
        clipped,
        format!(
            "@@ -1,0 +1,1 @@\n+{}PointerEvent{}\r\n",
            "a".repeat(1_000),
            "b".repeat(1_000)
        )
    );
    let query = patch_request(
        json!({"matchString":["NEEDLE"],"caseMode":"insensitive","contextLines":{"before":3,"after":3}}),
    );
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
        match_patch(patch, needle, 0, None)
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
    let whole = match_patch(deleted, "needle", 0, None).expect("hit").text;
    assert!(whole.starts_with("@@ -2,1 +0,0 @@"), "{whole}");
    // A whole-file addition: old side 0.
    let added = "@@ -0,0 +1,2 @@\n+one\n+needle\n";
    let new_file = match_patch(added, "needle", 0, None).expect("hit").text;
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
    let found = match_patch(&patch, "needle", 10, Some(&head)).expect("hit");
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
    let narrow = match_patch(&patch, "needle", 3, Some(&head)).expect("hit");
    assert_ne!(narrow.text, found.text);
    // A head that disagrees with the diff is not this diff's head.
    let mut stale = head.clone();
    stale[52] = "something else".into();
    let fallback = match_patch(&patch, "needle", 10, Some(&stale)).expect("hit");
    assert_eq!(fallback.clipped, [(43, 63)]);
}

/// GitHub ends a patch without a final newline: context widened from
/// the head text past the last hunk starts on its own line, so every
/// later line keeps its number (byte-exact numbered view).
#[test]
fn head_context_after_the_last_hunk_keeps_line_boundaries() {
    let (patch, head) = hunk_at_53();
    let patch = patch.trim_end_matches('\n');
    let found = match_patch(patch, "needle", 6, Some(&head)).expect("hit");
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
    let found = match_patch(&patch, "needle", 10, None).expect("hit");
    assert_eq!(found.clipped, [(43, 63)]);
    let request = patch_request(json!({"matchString":["needle"]}));
    assert!(head_text_needed(
        &patch,
        request.matcher.as_deref().expect("matcher"),
        (10, 10)
    ));
    assert!(!head_text_needed(
        &patch,
        request.matcher.as_deref().expect("matcher"),
        (3, 3)
    ));
    let query =
        patch_request(json!({"matchString":["needle"],"contextLines":{"before":10,"after":10}}));
    let page = shape_patch_page(vec![file("src/a.rs", &patch)], &query);
    assert_eq!(page.rows[0]["contextClipped"], true, "{:?}", page.rows);
    assert_eq!(page.clipped, vec![("src/a.rs".to_owned(), vec![(43, 63)])]);
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

#[test]
fn context_ranges_past_the_read_limit_preserve_every_requested_window() {
    let patch = (0..12)
        .map(|index| {
            let line = 50 + index * 100;
            format!("@@ -{line},3 +{line},3 @@\n before\n-old\n+needle-{index}\n after\n")
        })
        .collect::<String>();
    let found = match_patch(&patch, "needle", 10, None).expect("all hunks match");
    assert_eq!(found.clipped, [(41, 1161)]);
    for index in 0..12 {
        assert!(found.text.contains(&format!("+needle-{index}\n")));
    }
}

/// `matchString` keeps the contract's default context lines
/// (`limits.defaultReadContextLines`) around each hit and pages every hit
/// file of the PR at once, not 30 per call.
#[test]
fn match_string_defaults_to_the_contract_context_and_one_page_of_hit_files() {
    assert_eq!(
        match_context(&patch_request(json!({"matchString":["miri"]}))),
        (0, 0)
    );
    let patch = "@@ -1,3 +1,3 @@\n a\n-old miri\n+new miri\n b\n";
    let query = patch_request(json!({"matchString":["miri"]}));
    let view = history_patch_view(patch, "a.rs", &query);
    assert_eq!(
        (view.text.as_str(), view.narrowed, view.clipped.len()),
        ("@@ -2,1 +2,1 @@\n2\t-old miri\n2\t+new miri\n", true, 0)
    );
    let hits_only =
        patch_request(json!({"matchString":["miri"],"contextLines":{"before":0,"after":0}}));
    assert_eq!(
        history_patch_view(patch, "a.rs", &hits_only).text,
        "@@ -2,1 +2,1 @@\n2\t-old miri\n2\t+new miri\n"
    );
    assert_eq!(
        file_page_size(&query, true),
        crate::tools::gh_get_history_item::window::MAX_FILE_BATCHES * MAX_PER_PAGE
    );
    let explicit = patch_request(json!({"matchString":["miri"],"pageSize":5}));
    assert_eq!(file_page_size(&explicit, true), 5);
}

/// A `matchString` file row counts the hits its window shows
/// (`matchCount`), so "every hit shown" is checkable; a row the needle
/// matched by path only carries none.
#[test]
fn history_b_match_string_rows_count_the_hits_they_show() {
    let patch =
        "@@ -1,9 +1,9 @@ fn f\n a\n-old needle\n+new needle\n b\n c\n d\n e\n f\n+NEEDLE again\n";
    let query =
        patch_request(json!({"matchString":["needle"],"contextLines":{"before":1,"after":1}}));
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
        let query = with_budget(
            patch_request(
                json!({"matchString":["needle"],"contextLines":{"before":1,"after":1},"offset":offset}),
            ),
            75,
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

/// A long patch line keeps every keyword window at the keyword itself,
/// whatever lowercasing does to the bytes before it: `İ` (+1 byte) and `ẞ`
/// (−1) cancel to an equal length yet shift every offset, a lone `İ` or
/// `ẞ` changes the length, and combining marks, a final `Σ` and emoji ride
/// along. No window is dropped or moved.
#[test]
fn matching_long_unicode_lines_keeps_every_character() {
    let line = format!(
        "+İéxẞ {} NeedleOne {} 👩‍👩‍👧\n",
        "x".repeat(1_000),
        "y".repeat(1_000)
    );
    let patch = format!("@@ -0,0 +1,1 @@\n{line}");
    let found = match_patch(&patch, "needleone", 0, None).expect("hit");
    assert!(found.text.ends_with(&line));
    assert!(!found.text.contains("[…"));
}

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
