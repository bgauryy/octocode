//! The default walk order is read as a stream: a page reads only the files
//! it needs, the next page continues the walk, and every row is delivered
//! once.
use super::*;
use octocode_test_support::json_fixture;

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
        if index < last {
            for name in unseen {
                assert!(page.get(name).is_none(), "{name} on page {index}: {page}");
            }
        } else {
            // The last page states the walk's totals: 121 files, each with
            // two hit lines and three occurrences.
            assert_eq!(page["matchCount"], 363, "{page}");
            assert_eq!(page["matchedLineCount"], 242, "{page}");
            assert_eq!(page["matchedFiles"], 121, "{page}");
            assert_eq!(page["filesScanned"], 121, "{page}");
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
    query["matchString"] = json_fixture(r#"["n"]"#);
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
    let bounded = crate::local_search::executor::test_held_limit(Some(96 * 1024));
    let mut query = start(root.path(), "content");
    query["matchString"] = json_fixture(r#"["n"]"#);
    // Every other order ranks the whole scope and is refused past the bound.
    let mut whole = query.clone();
    whole["sort"] = json_fixture(r#"{"by": "path", "order": "asc"}"#);
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

/// Every page of a traversal walk from `root` under `policy`, following each
/// page's `next.nextPage` to the end.
fn walk_pages(
    root: &std::path::Path,
    policy: &PathPolicy,
    security: &ContentSecurity,
    window: Option<usize>,
) -> Vec<serde_json::Value> {
    let mut query = start(root, "content");
    let mut pages = Vec::new();
    loop {
        let page = search_in(&ls_query(query, None), policy, security, window)
            .unwrap_or_else(|error| panic!("{error:?}"))
            .to_row()
            .expect("row");
        let next = page["next"]["nextPage"]["input"]["queries"][0].clone();
        pages.push(page);
        if next.is_null() {
            return pages;
        }
        assert!(pages.len() < 100, "the walk ends");
        query = next;
    }
}

/// A walk whose hits fill its last page exactly ends without a page that
/// claims an absence: when the page holding the last hits could not know the
/// walk ended, the page after it is a success that shows no rows, states no
/// further page and says nothing of a miss. Every row still arrives once,
/// inside the workspace and in an allowed root outside it.
#[test]
fn a_walk_whose_hits_fill_its_last_page_exactly_ends_without_an_absence() {
    let workspace = tempfile::tempdir().expect("workspace");
    let outside = tempfile::tempdir().expect("allowed root");
    let workspace_root = workspace.path().canonicalize().expect("workspace path");
    let outside_root = outside.path().canonicalize().expect("allowed path");
    let elsewhere = octocode_runtime_base::policy::path::PathPolicy::new(
        octocode_runtime_base::policy::path::PathPolicyConfig {
            workspace_root: Some(workspace_root.clone()),
            additional_roots: vec![outside_root.clone()],
            ..Default::default()
        },
    )
    .expect("policy");
    let (inside, security) = policy_for(&workspace_root);
    // One hit in every tenth file: the last hit is followed by nine files
    // without one. Each corpus spells its paths and lines alike, so the first
    // `hits` hits of any two read the same.
    let corpus = |dir: &std::path::Path, hits: usize| {
        fs::create_dir_all(dir).expect("dir");
        for file in 0..hits * 10 {
            let text = if file % 10 == 0 {
                format!("needle line {file}\n")
            } else {
                format!("other line {file}\n")
            };
            fs::write(dir.join(format!("f{file:04}.txt")), text).expect("file");
        }
    };
    let rows_on = |page: &serde_json::Value| {
        page["files"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|file| file["matches"].as_array().map_or(0, Vec::len))
            .sum::<usize>()
    };
    for (base, policy) in [(&workspace_root, &inside), (&outside_root, &elsewhere)] {
        for window in [Some(2_500), None] {
            // Where a page fills: the pages of the largest corpus cut at the
            // hit counts any smaller one's do, within a row either way (the
            // counts a page states grow by a digit). A walk at each count
            // near a cut covers every way its last page can end, and at each
            // cut the hits fill the last page exactly, the case this is for.
            let probe = walk_pages(
                &{
                    let dir = base.join(format!("w{}_p64", window.unwrap_or(0)));
                    corpus(&dir, 64);
                    dir
                },
                policy,
                &security,
                window,
            );
            let full = probe[..probe.len() - 1]
                .iter()
                .map(rows_on)
                .collect::<Vec<_>>();
            let mut cuts = Vec::new();
            let mut counts = std::collections::BTreeSet::from([1, 2, 64]);
            for rows in &full {
                let cut = cuts.last().copied().unwrap_or(0) + rows;
                cuts.push(cut);
                counts.extend(cut.saturating_sub(2)..=cut + 2);
            }
            let mut exact_fills = 0;
            for hits in counts.into_iter().filter(|hits| (1..=64).contains(hits)) {
                let dir = base.join(format!("w{}_h{hits:02}", window.unwrap_or(0)));
                corpus(&dir, hits);
                let pages = walk_pages(&dir, policy, &security, window);
                let rows = pages
                    .iter()
                    .flat_map(|page| page["files"].as_array().cloned().unwrap_or_default())
                    .map(|file| file["matches"].as_array().map_or(0, Vec::len))
                    .sum::<usize>();
                let case = format!("{} {window:?} {hits} hits", dir.display());
                assert_eq!(rows, hits, "{case}: {pages:?}");
                for page in &pages {
                    assert_ne!(page["status"], "empty", "{case}: {page}");
                    assert!(page.get("hints").is_none(), "{case}: {page}");
                }
                let last = pages.last().expect("a page");
                assert_ne!(last["pagination"]["hasMore"], true, "{case}: {last}");
                // At a cut the hits end on a full page: the walk ends there.
                if let Some(at) = cuts.iter().position(|cut| *cut == hits) {
                    assert_eq!(pages.len(), at + 1, "{case}: no page after a full one");
                    assert_eq!(rows_on(last), full[at], "{case}: {last}");
                    exact_fills += 1;
                }
            }
            // A window whose first page holds every hit has no cut to fill.
            assert!(
                cuts.is_empty() || exact_fills > 0,
                "{} {window:?}: no count filled its last page exactly",
                base.display()
            );
        }
    }
}

/// A walk that fills its last page exactly ends on that page: the stream
/// stops only on a file the page cannot show, so the page that shows the
/// last hits knows the walk ended and offers no page after it. The corpus is
/// cut where a first walk's second page ended, in the workspace and in an
/// allowed root outside it (whose rows spell longer paths), at the default
/// and a narrow window, under a `limit` and without one.
#[test]
fn a_walk_that_fills_its_last_page_exactly_offers_no_page_after_it() {
    let workspace = tempfile::tempdir().expect("workspace");
    let outside = tempfile::tempdir().expect("allowed root");
    let workspace_root = workspace.path().canonicalize().expect("workspace path");
    let outside_root = outside.path().canonicalize().expect("allowed path");
    let elsewhere = octocode_runtime_base::policy::path::PathPolicy::new(
        octocode_runtime_base::policy::path::PathPolicyConfig {
            workspace_root: Some(workspace_root.clone()),
            additional_roots: vec![outside_root.clone()],
            ..Default::default()
        },
    )
    .expect("policy");
    let (inside, security) = policy_for(&workspace_root);
    // The rows of each page of a walk, its first `pages` pages when given.
    let rows_of = |dir: &std::path::Path,
                   policy: &PathPolicy,
                   window,
                   limit: Option<usize>,
                   pages: Option<usize>| {
        let mut query = start(dir, "content");
        if let Some(limit) = limit {
            query["limit"] = serde_json::json!(limit);
        }
        let mut rows = Vec::new();
        loop {
            let page = search_in(&ls_query(query, None), policy, &security, window)
                .unwrap_or_else(|error| panic!("{error:?}"))
                .to_row()
                .expect("row");
            rows.push(
                page["files"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|file| file["matches"].as_array().map_or(0, Vec::len))
                    .sum::<usize>(),
            );
            let next = page["next"]["nextPage"]["input"]["queries"][0].clone();
            if next.is_null() || pages.is_some_and(|pages| rows.len() >= pages) {
                return rows;
            }
            assert!(rows.len() < 200, "the walk ends");
            query = next;
        }
    };
    for (base, policy) in [(&workspace_root, &inside), (&outside_root, &elsewhere)] {
        for window in [Some(20_000), Some(2_500)] {
            for limit in [None, Some(60)] {
                let dir = base.join(format!("w{}_{}", window.unwrap_or(0), limit.unwrap_or(0)));
                fs::create_dir_all(&dir).expect("dir");
                // One hit in every tenth file.
                let files = 8_000usize;
                for file in 0..files {
                    let text = if file % 10 == 0 {
                        format!("needle line {file}\n")
                    } else {
                        format!("other line {file}\n")
                    };
                    fs::write(dir.join(format!("f{file:05}.txt")), text).expect("file");
                }
                // Three pages say there are more than two, and how many hits
                // the first two hold.
                let first = rows_of(&dir, policy, window, limit, Some(3));
                assert!(first.len() > 2, "{dir:?}: {first:?}");
                // Keep the hits of the first two pages, each followed by its
                // nine files without one.
                let kept = first[0] + first[1];
                for file in kept * 10..files {
                    fs::remove_file(dir.join(format!("f{file:05}.txt"))).expect("remove");
                }
                let again = rows_of(&dir, policy, window, limit, None);
                assert_eq!(
                    again,
                    first[..2],
                    "{dir:?} window {window:?} limit {limit:?}: no page after the last hits"
                );
            }
        }
    }
}
