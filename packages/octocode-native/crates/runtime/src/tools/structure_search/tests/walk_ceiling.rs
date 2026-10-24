//! Depth, the reused walk of continuation pages, and listings cut at the
//! walk ceiling.
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
                "operation": operation, "path": root.0, "maxDepth": 3, "maxEntries": 1
            });
            if let Some(kind) = kind {
                query["entryType"] = json!(kind);
            }
            let pages = walk_pages(&root.0, query);
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
        json!({"operation":"files","path":root.0.join("top.rs"),"maxEntries":1}),
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

/// A listing cut at the largest `maxEntries` keeps every listed row
/// reachable: each page with `hasMore` carries `next.nextPage`, and the
/// walk ceiling is a disclosed terminal limit with a narrowing lead, not a
/// silent end of paging.
#[test]
fn files_at_the_walk_ceiling_page_every_listed_row_and_lead_to_narrow() {
    let root = Fixture::new();
    for dir in 0..11 {
        let dir_path = root.0.join(format!("d{dir:02}"));
        std::fs::create_dir_all(&dir_path).expect("fixture dir");
        for file in 0..1_000 {
            std::fs::write(dir_path.join(format!("f{file:04}.ts")), "").expect("fixture file");
        }
    }
    // `walk_pages` fails on a row listed twice and on paging past its cap.
    let pages = walk_pages(
        &root.0,
        json!({"operation":"files","path":root.0,"extensions":["ts"]}),
    );
    let mut seen = std::collections::BTreeSet::new();
    for out in &pages {
        seen.extend(listed(out).iter().map(|row| row["path"].to_string()));
        if out["pagination"]["hasMore"] == true {
            assert!(
                out["next"]["nextPage"].is_object(),
                "hasMore without next: {}",
                out["pagination"]
            );
        }
        assert_eq!(out["terminalLimit"], true, "{}", out["pagination"]);
        // The walk ceiling is the stated cause, on every page.
        assert!(
            out["partialReasons"]
                .as_array()
                .is_some_and(|reasons| reasons
                    .iter()
                    .any(|reason| reason == "maxEntries" || reason == "walkLimit")),
            "{out}"
        );
    }
    let last = pages.last().expect("a page");
    let narrow = &related(last, "structureSearch")["input"]["queries"][0];
    assert_eq!(narrow["operation"], "tree", "{last}");
    assert_eq!(narrow["maxDepth"], 1, "{last}");
    assert_eq!(seen.len(), 10_000);
    assert!(pages.len() > 1);
}

/// A window's walk continues by one `continueWalk` at most, on its last page:
/// a limit cut widens (its query carries `maxEntries`), a time cut resumes
/// (its query carries `walkOffset`), whatever combination of cuts the window
/// met.
#[test]
fn a_window_offers_at_most_one_walk_continuation() {
    let ceiling = super::super::max_walk() as usize;
    let mut seen = std::collections::BTreeSet::new();
    for limit_cut in [false, true] {
        for scan_cut in [false, true] {
            for time_cut in [None, Some("src/a.rs".to_owned())] {
                for time_window in [
                    TimeWindow::Whole,
                    TimeWindow::EndedAtRows,
                    TimeWindow::SortedPastScanOffset,
                ] {
                    for requested in [10, ceiling] {
                        for total_pages in [1, 2] {
                            let cut = Cut {
                                page: 1,
                                total_pages,
                                total: requested,
                                requested,
                                available: requested + usize::from(limit_cut),
                                limit_cut,
                                scan_cut,
                                time_cut: time_cut.clone(),
                                time_window,
                                walk_position: requested + 5,
                                walk_offset: 0,
                                early_exit: false,
                                total_discovered: requested,
                                scan_offset: 0,
                                covered: requested,
                                ranked: false,
                                snapshot: "a".repeat(64),
                            };
                            let mut out = json!({});
                            cut.finish(
                                &mut out,
                                &json!({"operation":"files","path":"."}),
                                &mut Vec::new(),
                            );
                            let out = super::super::output::finish(out);
                            let lead = out["next"].get("continueWalk").map(|lead| {
                                if lead["input"]["queries"][0].get("walkOffset").is_some() {
                                    "resume"
                                } else {
                                    "widen"
                                }
                            });
                            let expected = cut.walk_lead().map(|(_, changes)| {
                                if changes.get("walkOffset").is_some() {
                                    "resume"
                                } else {
                                    "widen"
                                }
                            });
                            assert_eq!(lead, expected, "{out}");
                            if total_pages > 1 {
                                assert!(lead.is_none(), "the window's own pages first: {out}");
                            }
                            seen.extend(lead);
                        }
                    }
                }
            }
        }
    }
    assert_eq!(seen.len(), 2, "both continuations are reached: {seen:?}");
}

/// A sorted listing whose walk stopped at its ceiling knows only a lower
/// bound of its matches: it says `atLeast` (never an exact
/// `totalAvailable` of the stop count), and that its sort ranked only the
/// entries walked first.
#[test]
fn a_ranked_listing_cut_at_the_walk_ceiling_states_a_lower_bound() {
    let ceiling = super::super::max_walk() as usize;
    for ranked in [true, false] {
        let cut = Cut {
            page: 1,
            total_pages: 1,
            total: ceiling,
            requested: ceiling,
            available: ceiling + 1,
            limit_cut: true,
            scan_cut: true,
            time_cut: None,
            time_window: TimeWindow::Whole,
            walk_position: ceiling + 1,
            walk_offset: 0,
            early_exit: false,
            total_discovered: ceiling + 1,
            scan_offset: 0,
            covered: ceiling,
            ranked,
            snapshot: "snapshot".to_owned(),
        };
        let mut out = json!({});
        let mut warnings = Vec::new();
        cut.finish(
            &mut out,
            &json!({"operation":"files","path":"."}),
            &mut warnings,
        );
        assert_eq!(out["atLeast"], ceiling + 1, "{out}");
        assert!(out.get("totalAvailable").is_none(), "{out}");
        assert_eq!(out["terminalLimit"], true, "{out}");
        let warning = warnings.join(" ");
        assert!(
            warning.contains(&format!("with at least {} entries matched", ceiling + 1)),
            "{warning}"
        );
        assert_eq!(
            warning.contains("this sort ranks only the entries walked first"),
            ranked,
            "{warning}"
        );
    }
}

/// A ranked listing whose walk stopped at its ceiling ranks only the
/// entries walked first, whatever `maxEntries` it asked for: a small page
/// (`maxEntries:20`) says so too, as a terminal `walkLimit` (no widening
/// walks past the ceiling), keeps the widening of the walked ranking, and
/// leads into narrower scopes, which reach the entries past it.
#[test]
fn a_small_ranked_page_of_a_walk_cut_at_the_ceiling_says_the_ranking_is_partial() {
    let ceiling = super::super::max_walk() as usize;
    let cut = Cut {
        page: 1,
        total_pages: 1,
        total: 20,
        requested: 20,
        available: ceiling,
        limit_cut: true,
        scan_cut: true,
        time_cut: None,
        time_window: TimeWindow::Whole,
        walk_position: ceiling + 1,
        walk_offset: 0,
        early_exit: false,
        total_discovered: ceiling + 1,
        scan_offset: 0,
        covered: 20,
        ranked: true,
        snapshot: "a".repeat(64),
    };
    let mut out = json!({});
    let mut warnings = Vec::new();
    cut.finish(
        &mut out,
        &json!({"operation":"files","path":".","sort":{"by":"sizeBytes","order":"desc"},"maxEntries":20}),
        &mut warnings,
    );
    let out = super::super::output::finish(out);
    let warning = warnings.join(" ");
    assert!(
        warning.contains("this sort ranks only the entries walked first"),
        "{warning}"
    );
    assert_eq!(out["terminalLimit"], true, "{out}");
    assert!(!related(&out, "structureSearch").is_null(), "{out}");
    assert!(out["next"].get("continueWalk").is_some(), "{out}");
    assert_eq!(out["atLeast"], ceiling + 1, "{out}");
}
