//! Window packing: patch windows end at file or line boundaries and walk every file losslessly.
use super::*;

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
