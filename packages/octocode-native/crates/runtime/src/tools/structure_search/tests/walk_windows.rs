//! Depth, the reused walk of continuation pages, and listings that run
//! past one window: no walk ceiling ends them.
use super::super::cut::{Cut, TimeWindow};
use super::*;

#[test]
fn clipped_counts_match_all_reachable_entries_without_counting_the_listing_root() {
    let root = Fixture::new();
    for file in ["top.rs", "a/mid.rs", "a/b/low.rs"] {
        root.write(file, "pub fn item() {}\n");
    }
    for operation in ["tree", "files"] {
        for (kind, expected) in [(None, 5), (Some("file"), 3), (Some("directory"), 2)] {
            let mut query = json!({
                "operation": operation, "path": root.0, "maxDepth": 3
            });
            if let Some(kind) = kind {
                query["entryType"] = json!(kind);
            }
            let pages = with_window(1, || walk_pages(&root.0, query));
            assert_eq!(pages[0]["totalAvailable"], expected, "{}", pages[0]);
            let entries = pages
                .iter()
                .flat_map(paths)
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(entries.len(), expected, "{pages:?}");
        }
    }
    let single = run(
        &root.0,
        json!({"operation":"files","path":root.0.join("top.rs"),"limit":1}),
    )
    .expect("a file root lists itself");
    assert_eq!(paths(&single), ["./top.rs"]);
    assert!(single.get("clipped").is_none(), "{single}");
}

/// Continuation pages reuse the first page's walk while nothing listed
/// changed, and still restart when a listed file changes.
#[test]
fn continuation_pages_reuse_the_walk_and_restart_on_change() {
    let root = Fixture::new();
    for i in 0..6 {
        std::fs::write(root.0.join(format!("f{i}.rs")), "x\n").expect("file");
    }
    let first = run_budget(
        &root.0,
        json!({"operation":"files","path":root.0,"entryType": "file"}),
        1,
    )
    .expect("page 1");
    let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
    let second = run_budget(&root.0, next.clone(), 1).expect("page 2");
    assert_eq!(second["pagination"]["page"], 2, "{second}");
    let third = run_budget(
        &root.0,
        second["next"]["nextPage"]["input"]["queries"][0].clone(),
        1,
    )
    .expect("page 3");
    assert_eq!(third["pagination"]["page"], 3, "{third}");
    std::fs::write(root.0.join("f5.rs"), "changed size\n").expect("change");
    let stale = run_budget(&root.0, next, 1).expect("stale page");
    assert_eq!(stale["errorCode"], "staleSnapshot", "{stale}");
}

/// `maxDepth` counts the same in both operations: 1 lists `path`'s
/// children, 2 their children too.
#[test]
fn files_and_tree_count_max_depth_alike() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("a/b")).expect("dirs");
    std::fs::write(root.0.join("top.rs"), "x\n").expect("top");
    std::fs::write(root.0.join("a/mid.rs"), "x\n").expect("mid");
    std::fs::write(root.0.join("a/b/low.rs"), "x\n").expect("low");
    for (depth, expected) in [(1, vec!["top.rs"]), (2, vec!["mid.rs", "top.rs"])] {
        for operation in ["tree", "files"] {
            let out = run(
                &root.0,
                json!({"operation":operation,"path":root.0,"maxDepth":depth,"extensions":["rs"]}),
            )
            .expect("listing");
            let text = out.to_string();
            for name in ["top.rs", "mid.rs", "low.rs"] {
                assert_eq!(
                    text.contains(name),
                    expected.contains(&name),
                    "{operation} maxDepth {depth} {name}: {out}"
                );
            }
        }
    }
}

/// A listing past the internal scan window continues losslessly: its first window's
/// pages end in `next.nextPage`, the next window lists the rest after
/// `scanOffset`, no page is a terminal limit, and walking every page lists
/// each entry exactly once.
#[test]
fn files_past_the_scan_window_continue_to_every_entry() {
    let root = Fixture::new();
    let maximum = super::super::max_entries();
    let per_dir = maximum.div_ceil(10);
    let total = maximum + per_dir;
    for file in 0..total {
        root.write(&format!("d{:02}/f{file:05}.ts", file / per_dir), "");
    }
    // `walk_pages` fails on a row listed twice and on a repeated call.
    let pages = walk_pages(
        &root.0,
        json!({"operation":"files","path":root.0,"extensions":["ts"]}),
    );
    let mut seen = std::collections::BTreeSet::new();
    for out in &pages {
        seen.extend(listed(out).iter().map(|row| row["path"].to_string()));
        assert!(out.get("terminalLimit").is_none(), "{}", out["pagination"]);
    }
    assert_eq!(seen.len(), total);
    let resumed = pages
        .iter()
        .filter_map(|out| out["next"].get("nextPage"))
        .filter_map(|lead| {
            let query = &lead["input"]["queries"][0];
            query
                .get("scanOffset")
                .filter(|_| query.get("snapshot").is_none())
                .cloned()
        })
        .collect::<Vec<_>>();
    assert_eq!(resumed, [json!(maximum)], "one window boundary");
}

/// A ranked listing (sorted by size) walks its whole scope, so its sort
/// ranks every entry, and a window cut at `limit` continues with the
/// next window of the same ranking: the walk visits every entry, and the
/// windows together list each one once, in order.
#[test]
fn a_ranked_listing_continues_its_ranking_window_by_window() {
    let root = Fixture::new();
    for size in 0..30 {
        root.write(&format!("f{size:02}.txt"), "x".repeat(size));
    }
    let pages = with_window(4, || {
        walk_pages(
            &root.0,
            json!({"operation":"files","path":root.0,"entryType":"file",
                "sort":{"by":"sizeBytes","order":"desc"},"view":"paths"}),
        )
    });
    let order = pages.iter().flat_map(paths).collect::<Vec<_>>();
    let expected = (0..30)
        .rev()
        .map(|size| {
            format!(
                "{}/f{size:02}.txt",
                pages[0]["path"].as_str().expect("path")
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(order, expected);
    assert!(pages.iter().all(|out| out.get("terminalLimit").is_none()));
    assert_eq!(pages[0]["totalAvailable"], 30, "{}", pages[0]);
}

/// A window's walk continues by one `nextPage` at most, on its last page:
/// a limit cut lists the next window (its query carries `scanOffset`), a
/// time cut resumes (its query carries `walkOffset`), whatever combination of
/// cuts the window met; at the public `limit` maximum the limit cut
/// still continues.
#[test]
fn a_window_offers_at_most_one_walk_continuation() {
    use super::super::listing::Move;
    let maximum = super::super::max_entries();
    let mut seen = std::collections::BTreeSet::new();
    for limit_cut in [false, true] {
        for time_cut in [None, Some("src/a.rs".to_owned())] {
            for time_window in [
                TimeWindow::Whole,
                TimeWindow::EndedAtRows,
                TimeWindow::SortedPastScanOffset,
            ] {
                for requested in [10, maximum] {
                    for total_pages in [1, 2] {
                        let cut = Cut {
                            page: 1,
                            total_pages,
                            total: requested,
                            requested,
                            available: requested + usize::from(limit_cut),
                            limit_cut,
                            time_cut: time_cut.clone(),
                            time_window,
                            walk_position: requested + 5,
                            walk_offset: 0,
                            early_exit: false,
                            total_discovered: requested,
                            scan_offset: 0,
                            covered: requested,
                            snapshot: "a".repeat(64),
                        };
                        let mut out = json!({});
                        cut.finish(&mut out, &files_query(), &mut Vec::new());
                        let out = super::super::output::finish(out);
                        let kind = |query: &Value| {
                            if query.get("walkOffset").is_some() {
                                "resume"
                            } else {
                                "window"
                            }
                        };
                        let lead = out["next"]
                            .get("nextPage")
                            .map(|lead| &lead["input"]["queries"][0])
                            .filter(|query| query.get("snapshot").is_none())
                            .map(kind);
                        let expected = cut.walk_lead().map(|lead| match lead {
                            Move::Resume { .. } => "resume",
                            Move::Window { .. } => "window",
                            Move::Page { .. } => unreachable!("a walk lead never pages"),
                        });
                        assert_eq!(lead, expected, "{out}");
                        if total_pages > 1 {
                            assert!(lead.is_none(), "the window's own pages first: {out}");
                        } else if limit_cut && time_cut.is_none() {
                            assert_eq!(lead, Some("window"), "{out}");
                            assert!(out.get("terminalLimit").is_none(), "{out}");
                        }
                        seen.extend(lead);
                    }
                }
            }
        }
    }
    assert_eq!(seen.len(), 2, "both continuations are reached: {seen:?}");
}
