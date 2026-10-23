//! Per-file summaries: unfinished patch pages open with one, a page whose patches fit has none.
use super::*;

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
