//! Windows cut by `maxEntries` and by time never overlap, and count what
//! the walk left out once, whatever the timing.

use super::*;

#[test]
fn windows_cut_by_maxentries_and_by_time_never_overlap_whatever_the_timing() {
    let big = UNSPENT;
    let small = STEP * 8 / 3;
    let schedules: [&[Duration]; 4] = [
        &[big, small, big],
        &[small, big],
        &[big, small, small, big],
        &[small],
    ];
    for (operation, extra) in [
        ("tree", json!({})),
        ("files", json!({})),
        ("files", json!({"sort":{"by":"traversal","order":"asc"}})),
    ] {
        let mut time_cuts = 0;
        for max_entries in [2, 3, 5] {
            for budgets in schedules {
                let root = Fixture::new();
                let mut start = flat_listing(operation, &root, extra.clone());
                let whole = {
                    let mut all = listed_entries(&run(&root.0, start.clone()).expect("whole"));
                    all.sort();
                    all
                };
                assert_eq!(whole.len(), 9, "{operation}: {whole:?}");
                start["maxEntries"] = json!(max_entries);
                let (windows, cuts) = follow_windows(&root.0, start, budgets);
                time_cuts += cuts;
                let mut chained = windows.concat();
                chained.sort();
                assert_eq!(
                    chained, whole,
                    "{operation} maxEntries {max_entries} {budgets:?}: every entry exactly once, windows {windows:?}"
                );
            }
        }
        // The properties above are about mixed cuts: the small budgets did
        // cut walks (each entry takes `STEP`, and the budget is a few).
        assert!(time_cuts > 0, "{operation}: no window was cut by time");
    }
}

/// What a window's page says its walk left out: entries `.gitignore` hid, dot
/// entries skipped, and entries the path policy withheld.
fn left_out(out: &Value) -> (u64, u64, u64) {
    (
        skipped_count(out, "ignored"),
        skipped_count(out, "hidden"),
        skipped_count(out, "withheld"),
    )
}

/// A walk's windows count what the walk left out once between them: a window
/// that ends at its `maxEntries`-th row leaves the entries after it to the
/// window that resumes there, and neither counts them twice. (A widening
/// `continueWalk` walks the same walk again, so it is no resume: only chains
/// of resuming windows, whose query carries `walkOffset`, add up.)
#[test]
fn windows_count_what_the_walk_left_out_once_whatever_the_timing() {
    let big = UNSPENT;
    let small = STEP * 8 / 3;
    // Budgets that span several entries past `maxEntries` rows cut a window
    // at its rows, with entries left out after the last of them.
    let (long, longer) = (STEP * 6, STEP * 11);
    let schedules: [&[Duration]; 6] = [
        &[small],
        &[big, small, big],
        &[small, big],
        &[long],
        &[longer],
        &[long, longer, small],
    ];
    for operation in ["tree", "files"] {
        let root = Fixture::new();
        root.write(".git/HEAD", "ref: refs/heads/main\n");
        root.write(".gitignore", "*.log\n");
        for index in 0..6 {
            root.write(&format!("s{index}.rs"), "fn f() {}\n");
            root.write(&format!("s{index}.log"), "x\n");
            root.write(&format!("s{index}.pem"), "x\n");
            root.write(&format!("t{index}/.hidden"), "x\n");
            root.write(&format!("t{index}/u.rs"), "fn f() {}\n");
        }
        let mut start = rows(operation, &root.0);
        start.as_object_mut().expect("row").remove("include");
        start["includeHidden"] = json!(false);
        let whole = left_out(&run(&root.0, start.clone()).expect("whole"));
        assert!(
            whole.0 > 0 && whole.1 > 0 && whole.2 > 0,
            "{operation}: the fixture leaves out all three: {whole:?}"
        );
        let (mut time_cuts, mut chains) = (0, 0);
        for max_entries in [2, 4] {
            for budgets in schedules {
                let mut start = start.clone();
                start["maxEntries"] = json!(max_entries);
                let (outs, resumed) = follow_outputs(&root.0, start, budgets);
                time_cuts += resumed.iter().filter(|resumed| **resumed).count();
                let resumes = resumed[..resumed.len() - 1].iter().all(|resumed| *resumed);
                if !resumes {
                    continue;
                }
                chains += 1;
                let summed = outs.iter().map(left_out).fold((0, 0, 0), |sum, window| {
                    (sum.0 + window.0, sum.1 + window.1, sum.2 + window.2)
                });
                assert_eq!(
                    summed,
                    whole,
                    "{operation} maxEntries {max_entries} {budgets:?}: counted once across {} windows",
                    outs.len()
                );
            }
        }
        assert!(time_cuts > 0, "{operation}: no window was cut by time");
        assert!(chains > 0, "{operation}: no chain of resumed windows");
    }
}
