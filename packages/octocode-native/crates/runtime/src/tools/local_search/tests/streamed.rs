//! The default walk order is read as a stream: a page reads only the files
//! it needs, the next page continues the walk, and every row is delivered
//! once.
use super::*;

const NEEDLE_FILE: &str = "alpha needle\nbeta needle needle\nother\n";

fn start(root: &std::path::Path, view: &str) -> serde_json::Value {
    let term = if view == "filesWithout" {
        "missing"
    } else {
        "needle"
    };
    serde_json::json!({
        "path": root, "matchString": [term], "view": view,
        "sort": {"by": "traversal", "order": "asc"}
    })
}

fn fixture(files: usize) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("fixture");
    for file in 0..files {
        // Directories and names that order differently as strings and as
        // walk paths.
        let dir = root.path().join(format!("d{}", file % 5));
        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join(format!("f{file:03}.txt")), NEEDLE_FILE).expect("file");
    }
    fs::write(root.path().join("d0-extra.txt"), NEEDLE_FILE).expect("file");
    root
}

/// Every row the whole scan lists, in walk order.
fn expected_rows(root: &std::path::Path, view: &str) -> Vec<String> {
    let mut files = Vec::new();
    for entry in walkdir_sorted(root) {
        files.push(entry);
    }
    files
        .iter()
        .flat_map(|path| {
            let shown = path
                .strip_prefix(root)
                .expect("under root")
                .display()
                .to_string();
            match view {
                "files" => vec![shown],
                // Every file lacks the term, so each is listed.
                "filesWithout" => vec![shown],
                "matchOnly" => vec![
                    format!("{shown}:1:7"),
                    format!("{shown}:2:6"),
                    format!("{shown}:2:13"),
                ],
                _ => vec![format!("{shown}:1:null"), format!("{shown}:2:null")],
            }
        })
        .collect()
}

/// The files under `root` in walk order: siblings by name, depth first.
fn walkdir_sorted(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut entries = fs::read_dir(root)
        .expect("dir")
        .map(|entry| entry.expect("entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            out.extend(walkdir_sorted(&entry));
        } else {
            out.push(entry);
        }
    }
    out
}

fn first_page(
    root: &std::path::Path,
    fields: serde_json::Value,
    window: Option<usize>,
) -> serde_json::Value {
    let (policy, security) = policy_for(root);
    let mut query = start(root, "content");
    for (key, value) in fields.as_object().expect("fields") {
        query[key] = value.clone();
    }
    let request = ls_query(query, None);
    search_in(&request, &policy, &security, window)
        .expect("search")
        .to_row()
        .expect("row")
}

#[test]
fn page_one_reads_only_what_its_page_needs() {
    let root = fixture(400);
    for view in ["content", "matchOnly", "files"] {
        let page = first_page(root.path(), serde_json::json!({"view": view}), Some(2500));
        let searched = page["filesScanned"].as_u64().expect("filesScanned");
        assert!(searched >= 1, "{view}: {page}");
        // 401 files hold hits; a page of this budget needs a small part.
        assert!(searched < 100, "{view}: page one read {searched} files");
        assert_eq!(page["pagination"]["hasMore"], true, "{view}: {page}");
    }
}

#[test]
fn walking_every_traversal_page_equals_the_whole_scan_exactly_once() {
    let root = fixture(120);
    for view in ["content", "matchOnly", "files", "filesWithout"] {
        for (budget, limit) in [(2500, None), (9000, None), (200_000, None), (9000, Some(7))] {
            let mut query = start(root.path(), view);
            if let Some(limit) = limit {
                query["limit"] = serde_json::json!(limit);
            }
            let (walk, pages) = walk_search(root.path(), query, Some(budget));
            assert_eq!(
                walk.rows,
                expected_rows(root.path(), view),
                "{view} budget {budget} limit {limit:?}"
            );
            if budget < 200_000 && view != "filesWithout" {
                assert!(pages.len() > 1, "{view} budget {budget}: one page");
            }
        }
    }
}

#[test]
fn totals_are_floors_until_the_last_page() {
    let root = fixture(120);
    let (_, pages) = walk_search(root.path(), start(root.path(), "content"), Some(4000));
    assert!(pages.len() > 2);
    let last = pages.len() - 1;
    for (index, page) in pages.iter().enumerate() {
        let paging = &page["pagination"];
        let unseen = ["matchCount", "matchedLineCount", "matchedFiles"];
        if index < last {
            assert_eq!(paging["hasMore"], true, "{page}");
            assert_eq!(paging["countScope"], "loaded", "{page}");
            let total = paging["total"].as_u64().expect("total");
            assert_eq!(
                total,
                paging["offset"].as_u64().expect("offset")
                    + paging["length"].as_u64().expect("length")
            );
        } else {
            // The chain ended: its total is exact and no floor is claimed.
            assert!(paging.get("countScope").is_none(), "{page}");
            // 121 files, two hit lines each.
            assert_eq!(paging["total"], 242, "{page}");
        }
        if index > 0 || index < last {
            for name in unseen {
                assert!(page.get(name).is_none(), "{name} on page {index}: {page}");
            }
        }
    }
}

#[test]
fn a_file_changed_before_the_cut_restarts_and_one_after_it_shows_new_text() {
    let root = fixture(60);
    let (policy, security) = policy_for(root.path());
    let run = |query: serde_json::Value| {
        search_in(&ls_query(query, None), &policy, &security, Some(2500))
    };
    let first = run(start(root.path(), "content")).expect("page one");
    let first = first.to_row().expect("row");
    let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
    // A file before the cut changes: the cursor restarts.
    let shown = first["files"][0]["path"].as_str().expect("path");
    let before = root.path().join(shown);
    let kept = fs::read_to_string(&before).expect("read");
    fs::write(&before, format!("{kept}a longer line than before\n")).expect("rewrite");
    let stale = run(next.clone()).expect_err("a changed prefix restarts");
    assert_eq!(
        stale.code,
        octocode_contract_types::ErrorCode::StaleSnapshot
    );
    fs::write(&before, kept).expect("restore");
    // A file after the cut changes: the page shows its current text. The
    // restored file carries new stamps, so the search starts again.
    let again = run(start(root.path(), "content")).expect("page one again");
    let mut query = again.to_row().expect("row")["next"]["nextPage"]["input"]["queries"][0].clone();
    let last = walkdir_sorted(root.path()).pop().expect("a file");
    fs::write(&last, "needle now reads differently\n").expect("rewrite");
    let mut seen = String::new();
    loop {
        let page = run(query.clone()).expect("a page");
        let row = page.to_row().expect("row");
        seen.push_str(&row.to_string());
        match row["next"]["nextPage"]["input"]["queries"][0].clone() {
            serde_json::Value::Null => break,
            next => query = next,
        }
    }
    assert!(seen.contains("needle now reads differently"), "{seen}");
}

#[test]
fn a_page_that_splits_a_file_resumes_inside_it() {
    let root = tempfile::tempdir().expect("fixture");
    let hits = 500;
    fs::write(root.path().join("many.txt"), "n\n".repeat(hits)).expect("file");
    fs::write(root.path().join("zlast.txt"), "n\n").expect("file");
    let mut query = start(root.path(), "content");
    query["matchString"] = serde_json::json!(["n"]);
    let (walk, pages) = walk_search(root.path(), query, Some(2500));
    assert!(pages.len() > 3, "the file must cross page edges");
    let mut lines = walk
        .rows
        .iter()
        .filter(|row| row.contains("many.txt"))
        .map(|row| {
            row.split(':')
                .nth(1)
                .expect("line")
                .parse::<usize>()
                .expect("n")
        })
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), hits);
    lines.dedup();
    assert_eq!(lines, (1..=hits).collect::<Vec<_>>());
    assert!(walk.rows.iter().any(|row| row.contains("zlast.txt")));
}

#[test]
fn a_traversal_search_past_the_held_bound_pages_instead_of_refusing() {
    let root = tempfile::tempdir().expect("fixture");
    for file in 0..300 {
        fs::write(root.path().join(format!("f{file:03}.txt")), "n\n".repeat(8)).expect("file");
    }
    let (policy, security) = policy_for(root.path());
    let bounded = crate::tools::local_search::executor::test_held_limit(Some(96 * 1024));
    let mut query = start(root.path(), "content");
    query["matchString"] = serde_json::json!(["n"]);
    // Every other order ranks the whole scope and is refused past the bound.
    let mut whole = query.clone();
    whole["sort"] = serde_json::json!({"by": "path", "order": "asc"});
    let refused = search_in(&ls_query(whole, None), &policy, &security, Some(4000))
        .expect_err("the whole scope is past the bound");
    assert_eq!(
        refused.code,
        octocode_contract_types::ErrorCode::ScopeTooBroad
    );
    let page = search_in(&ls_query(query, None), &policy, &security, Some(4000))
        .expect("a stream holds only its page");
    let row = page.to_row().expect("row");
    assert!(
        row["files"]
            .as_array()
            .is_some_and(|files| !files.is_empty()),
        "{row}"
    );
    drop(bounded);
}

/// Pages that end inside many files deliver each row once and in order.
#[test]
fn pages_that_end_inside_files_deliver_every_row_once_in_order() {
    let root = tempfile::tempdir().expect("fixture");
    let (files, hits) = (40usize, 30usize);
    for file in 0..files {
        let body = (0..hits)
            .map(|hit| format!("const v{hit} = needle({file}, {hit});\n// plain line {hit}\n"))
            .collect::<String>();
        fs::write(root.path().join(format!("f{file:02}.ts")), body).expect("file");
    }
    for budget in [4_000, 9_000, 20_000] {
        let (walk, pages) = walk_search(root.path(), start(root.path(), "content"), Some(budget));
        let mut expected = Vec::new();
        for file in 0..files {
            for hit in 0..hits {
                expected.push(format!("f{file:02}.ts:{}:null", hit * 2 + 1));
            }
        }
        assert!(pages.len() > 2, "budget {budget}");
        assert_eq!(walk.rows, expected, "budget {budget}");
    }
}
