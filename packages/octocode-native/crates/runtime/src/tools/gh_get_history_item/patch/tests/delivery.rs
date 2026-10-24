//! Patch delivery: identical patches referenced once, binary and missing patches named, compact rows with no placeholders.
use super::*;

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
            &with_budget(window(json!({"offset":offset})), 500),
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

#[test]
fn binary_files_are_reported_once_and_never_block_the_cursor() {
    let files = vec![
        file("a.rs", &"a".repeat(10)),
        json!({"filename":"logo.png","status":"added"}),
        file("b.rs", &"b".repeat(10)),
        json!({"filename":"tail.bin","status":"added"}),
    ];
    let (calls, patches, patchless) = follow_commit_page(&files, Some(4));
    assert_eq!(calls, 20);
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
        &with_budget(patch_request(json!({"offset":0})), 25),
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
        &with_budget(patch_request(json!({"offset":0})), 38),
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
        "operation":"commit","owner":"o","repo":"r",
        "ref":"abc","include":["*.md","src/**"]
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
